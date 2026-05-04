//! LEZ-backed `RegistryClient` adapter.
//!
//! Drives the deployed `whistleblower-registry` LEZ program on a real
//! sequencer. Bridges the Qt-free indexing crate to the on-chain LEZ
//! transaction submission machinery (wallet + nssa).
//!
//! Construction takes a `WalletCore` and the program ELF; the adapter
//! derives the registry-root PDA from the program ID + the shared
//! `REGISTRY_PDA_SEED_BYTES` constant. After that, `anchor_one` /
//! `anchor_batch` build a `RegistryInstruction`, borsh-encode it,
//! submit via `sequencer_client.send_transaction(NSSATransaction::Public(_))`,
//! poll `get_transaction(hash)` until landed, then re-read the PDA and
//! return the canonical `AnchorEntry` from the on-chain state.

use async_trait::async_trait;
use borsh::BorshDeserialize;
use common::transaction::NSSATransaction;
use document_indexing::{AdapterError, RegistryClient};
use nssa::{
    program::Program,
    public_transaction::{Message, WitnessSet},
    AccountId, PublicTransaction,
};
use nssa_core::program::PdaSeed;
use sequencer_service_rpc::RpcClient;
use std::sync::Arc;
use std::time::Duration;
use wallet::WalletCore;
use whistleblower_core::{
    cid_hash as compute_cid_hash, AnchorEntry, CanonicalCid, CidHash, MetadataHash,
    RegistryInstruction, REGISTRY_PDA_SEED_BYTES,
};
use whistleblower_methods::WHISTLEBLOWER_REGISTRY_ELF;

/// Mirror of the on-chain state struct. Defined here so this crate doesn't
/// depend on the guest crate.
#[derive(Debug, Clone, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct RegistryStateOnChain {
    entries: Vec<AnchorEntry>,
}

pub struct LezRegistryClient {
    wallet_core: Arc<WalletCore>,
    program: Program,
    registry_pda: AccountId,
    /// Seconds we tolerate per submitted tx before giving up on confirmation.
    /// Localnet block interval is ~15s; defaults to 30s.
    confirmation_timeout: Duration,
    /// Default anchor timestamp — production should use the LEZ clock account
    /// (see ARCHITECTURE.md) but we don't yet read clock accounts. For now
    /// the host supplies a unix-ms timestamp via SystemTime.
    timestamp_source: TimestampSource,
}

#[derive(Clone)]
enum TimestampSource {
    SystemTimeMs,
    Fixed(u64),
}

impl LezRegistryClient {
    /// Create a client using the embedded `WHISTLEBLOWER_REGISTRY_ELF`.
    /// Uses system unix-ms for `anchor_timestamp`.
    pub fn new(wallet_core: Arc<WalletCore>) -> Result<Self, AdapterError> {
        Self::with_program_bytes(wallet_core, WHISTLEBLOWER_REGISTRY_ELF.to_vec())
    }

    pub fn with_program_bytes(
        wallet_core: Arc<WalletCore>,
        elf: Vec<u8>,
    ) -> Result<Self, AdapterError> {
        let program = Program::new(elf)
            .map_err(|e| AdapterError::non_retryable(format!("parse program ELF: {e:?}")))?;
        let registry_pda: AccountId =
            (&program.id(), &PdaSeed::new(REGISTRY_PDA_SEED_BYTES)).into();
        Ok(Self {
            wallet_core,
            program,
            registry_pda,
            confirmation_timeout: Duration::from_secs(30),
            timestamp_source: TimestampSource::SystemTimeMs,
        })
    }

    /// Override the anchor timestamp source (useful for deterministic tests).
    pub fn with_fixed_timestamp(mut self, ts: u64) -> Self {
        self.timestamp_source = TimestampSource::Fixed(ts);
        self
    }

    pub fn registry_pda(&self) -> AccountId {
        self.registry_pda.clone()
    }

    fn next_timestamp(&self) -> u64 {
        match self.timestamp_source {
            TimestampSource::Fixed(ts) => ts,
            TimestampSource::SystemTimeMs => {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }
        }
    }

    async fn submit_and_wait(
        &self,
        instruction: RegistryInstruction,
        label: &str,
    ) -> Result<(), AdapterError> {
        let payload = borsh::to_vec(&instruction)
            .map_err(|e| AdapterError::non_retryable(format!("encode instruction: {e}")))?;
        let message = Message::try_new(
            self.program.id(),
            vec![self.registry_pda.clone()],
            vec![],
            payload,
        )
        .map_err(|e| AdapterError::retryable(format!("build message: {e:?}")))?;
        let witness_set = WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let hash = self
            .wallet_core
            .sequencer_client
            .send_transaction(NSSATransaction::Public(tx))
            .await
            .map_err(|e| AdapterError::retryable(format!("submit {label}: {e:?}")))?;

        // Poll get_transaction(hash) until it lands or we exhaust the timeout.
        let poll_interval = Duration::from_millis(750);
        let max_attempts = (self.confirmation_timeout.as_millis() / poll_interval.as_millis())
            as usize;
        for _ in 0..max_attempts {
            tokio::time::sleep(poll_interval).await;
            if let Ok(Some(_)) = self
                .wallet_core
                .sequencer_client
                .get_transaction(hash)
                .await
            {
                return Ok(());
            }
        }
        Err(AdapterError::retryable(format!(
            "{label}: tx {hash:?} did not confirm within {:?}",
            self.confirmation_timeout
        )))
    }

    async fn read_state(&self) -> Result<RegistryStateOnChain, AdapterError> {
        let account = self
            .wallet_core
            .get_account_public(self.registry_pda.clone())
            .await
            .map_err(|e| AdapterError::retryable(format!("fetch registry account: {e:?}")))?;
        let bytes: Vec<u8> = account.data.clone().into();
        if bytes.is_empty() {
            return Ok(RegistryStateOnChain::default());
        }
        BorshDeserialize::try_from_slice(&bytes)
            .map_err(|e| AdapterError::non_retryable(format!("decode registry state: {e}")))
    }
}

#[async_trait]
impl RegistryClient for LezRegistryClient {
    async fn anchor_one(
        &self,
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
    ) -> Result<AnchorEntry, AdapterError> {
        let cid_hash = compute_cid_hash(&cid);
        let timestamp = self.next_timestamp();
        self.submit_and_wait(
            RegistryInstruction::AnchorOne {
                cid: cid.clone(),
                metadata_hash,
                anchor_timestamp: timestamp,
            },
            "anchor_one",
        )
        .await?;
        let state = self.read_state().await?;
        state
            .entries
            .into_iter()
            .find(|e| e.cid_hash == cid_hash)
            .ok_or_else(|| {
                AdapterError::retryable("anchor_one: entry missing from registry after confirmation")
            })
    }

    async fn anchor_batch(
        &self,
        entries: Vec<(CanonicalCid, MetadataHash)>,
    ) -> Result<Vec<AnchorEntry>, AdapterError> {
        let timestamp = self.next_timestamp();
        let target_hashes: Vec<CidHash> =
            entries.iter().map(|(cid, _)| compute_cid_hash(cid)).collect();
        self.submit_and_wait(
            RegistryInstruction::AnchorBatch {
                entries: entries.clone(),
                anchor_timestamp: timestamp,
            },
            "anchor_batch",
        )
        .await?;
        let state = self.read_state().await?;
        let mut out = Vec::with_capacity(target_hashes.len());
        for hash in target_hashes {
            let found = state.entries.iter().find(|e| e.cid_hash == hash).cloned();
            match found {
                Some(e) => out.push(e),
                None => {
                    return Err(AdapterError::retryable(
                        "anchor_batch: entry missing from registry after confirmation",
                    ))
                }
            }
        }
        Ok(out)
    }

    async fn query_by_cid_hash(
        &self,
        cid_hash: CidHash,
    ) -> Result<Option<AnchorEntry>, AdapterError> {
        let state = self.read_state().await?;
        Ok(state.entries.into_iter().find(|e| e.cid_hash == cid_hash))
    }
}
