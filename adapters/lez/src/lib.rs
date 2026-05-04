//! LEZ-backed `RegistryClient` adapter — PDA-per-CID design.
//!
//! Drives the deployed `whistleblower-registry` LEZ program on a real
//! sequencer. Each anchored CID lives in its own PDA derived from
//! `(program_id, PdaSeed::new(cid_hash))`; the adapter computes those
//! PDAs upfront, includes them in the transaction's account list in the
//! same order as the instruction's entries, and the guest re-derives +
//! validates the match.
//!
//! Construction takes a `WalletCore` and the program ELF. After that:
//!   - `anchor_one(cid, metadata_hash)` derives one PDA, submits a one-account
//!     tx, polls until confirmed, then reads the PDA's account.data and
//!     decodes the `AnchorEntry`.
//!   - `anchor_batch(entries)` derives N PDAs, submits one tx with N accounts,
//!     polls until confirmed, decodes each PDA's entry.
//!   - `query_by_cid_hash(hash)` derives the PDA, fetches the account, and
//!     decodes — no transaction at all.
//!
//! Idempotency is built into the program (re-anchoring an existing PDA is a
//! no-op success), so the adapter doesn't need any special-case logic for
//! "already anchored".

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
    RegistryInstruction,
};
use whistleblower_methods::WHISTLEBLOWER_REGISTRY_ELF;

pub struct LezRegistryClient {
    wallet_core: Arc<WalletCore>,
    program: Program,
    /// Seconds we tolerate per submitted tx before giving up on confirmation.
    /// Localnet block interval is ~15s; defaults to 30s.
    confirmation_timeout: Duration,
    /// Default anchor timestamp source. Production should use the LEZ clock
    /// account (see ARCHITECTURE.md, "anchor_timestamp" risk row) but we
    /// don't yet read clock accounts. For now the host supplies unix-ms.
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
        Ok(Self {
            wallet_core,
            program,
            confirmation_timeout: Duration::from_secs(30),
            timestamp_source: TimestampSource::SystemTimeMs,
        })
    }

    /// Override the anchor timestamp source (useful for deterministic tests).
    pub fn with_fixed_timestamp(mut self, ts: u64) -> Self {
        self.timestamp_source = TimestampSource::Fixed(ts);
        self
    }

    /// Derive the deterministic PDA for a given CID. Public so callers can
    /// pre-compute PDAs (e.g. for the batch CLI's queue logic) without
    /// going through anchor.
    pub fn entry_pda_for(&self, cid: &CanonicalCid) -> AccountId {
        let cid_hash = compute_cid_hash(cid);
        (&self.program.id(), &PdaSeed::new(cid_hash.0)).into()
    }

    pub fn entry_pda_for_hash(&self, cid_hash: CidHash) -> AccountId {
        (&self.program.id(), &PdaSeed::new(cid_hash.0)).into()
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

    /// Submit a `RegistryInstruction` against an explicitly-supplied list of
    /// account PDAs (one per entry, in matching order — verified by guest).
    async fn submit_and_wait(
        &self,
        accounts: Vec<AccountId>,
        instruction: RegistryInstruction,
        label: &str,
    ) -> Result<(), AdapterError> {
        let payload = borsh::to_vec(&instruction)
            .map_err(|e| AdapterError::non_retryable(format!("encode instruction: {e}")))?;
        let message = Message::try_new(self.program.id(), accounts, vec![], payload)
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

    /// Read a single entry-PDA's account.data and decode the `AnchorEntry`.
    /// Returns `Ok(None)` if the PDA isn't yet claimed (data empty).
    async fn read_entry(&self, pda: AccountId) -> Result<Option<AnchorEntry>, AdapterError> {
        let account = self
            .wallet_core
            .get_account_public(pda)
            .await
            .map_err(|e| AdapterError::retryable(format!("fetch entry account: {e:?}")))?;
        let bytes: Vec<u8> = account.data.clone().into();
        if bytes.is_empty() {
            return Ok(None);
        }
        AnchorEntry::try_from_slice(&bytes)
            .map(Some)
            .map_err(|e| AdapterError::non_retryable(format!("decode AnchorEntry: {e}")))
    }
}

#[async_trait]
impl RegistryClient for LezRegistryClient {
    async fn anchor_one(
        &self,
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
    ) -> Result<AnchorEntry, AdapterError> {
        let pda = self.entry_pda_for(&cid);
        let timestamp = self.next_timestamp();
        self.submit_and_wait(
            vec![pda.clone()],
            RegistryInstruction::AnchorOne {
                cid: cid.clone(),
                metadata_hash,
                anchor_timestamp: timestamp,
            },
            "anchor_one",
        )
        .await?;
        self.read_entry(pda)
            .await?
            .ok_or_else(|| AdapterError::retryable(
                "anchor_one: entry-PDA still empty after tx confirmation",
            ))
    }

    async fn anchor_batch(
        &self,
        entries: Vec<(CanonicalCid, MetadataHash)>,
    ) -> Result<Vec<AnchorEntry>, AdapterError> {
        let pdas: Vec<AccountId> =
            entries.iter().map(|(cid, _)| self.entry_pda_for(cid)).collect();
        let timestamp = self.next_timestamp();
        self.submit_and_wait(
            pdas.clone(),
            RegistryInstruction::AnchorBatch {
                entries: entries.clone(),
                anchor_timestamp: timestamp,
            },
            "anchor_batch",
        )
        .await?;
        let mut out = Vec::with_capacity(pdas.len());
        for pda in pdas {
            let entry = self.read_entry(pda).await?.ok_or_else(|| {
                AdapterError::retryable(
                    "anchor_batch: entry-PDA still empty after tx confirmation",
                )
            })?;
            out.push(entry);
        }
        Ok(out)
    }

    async fn query_by_cid_hash(
        &self,
        cid_hash: CidHash,
    ) -> Result<Option<AnchorEntry>, AdapterError> {
        // No transaction needed — derive the PDA, read the account, decode.
        let pda = self.entry_pda_for_hash(cid_hash);
        self.read_entry(pda).await
    }
}
