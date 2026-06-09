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
use whistleblower_methods::{WHISTLEBLOWER_REGISTRY_ELF, WHISTLEBLOWER_REGISTRY_PATH};

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
        let elf = if WHISTLEBLOWER_REGISTRY_ELF.is_empty() {
            std::fs::read(WHISTLEBLOWER_REGISTRY_PATH).map_err(|e| {
                AdapterError::non_retryable(format!(
                    "embedded registry ELF is empty and fallback path {} could not be read: {e}",
                    WHISTLEBLOWER_REGISTRY_PATH
                ))
            })?
        } else {
            WHISTLEBLOWER_REGISTRY_ELF.to_vec()
        };
        Self::with_program_bytes(wallet_core, elf)
    }

    pub fn with_program_bytes(
        wallet_core: Arc<WalletCore>,
        elf: Vec<u8>,
    ) -> Result<Self, AdapterError> {
        let program = Program::new(elf)
            .map_err(|e| AdapterError::non_retryable(format!("parse program ELF: {e:?}")))?;
        // Localnet confirms in ~15s (one block); the public LEZ testnet is
        // slower and a submitted tx can take several blocks to surface via
        // get_transaction. 30s (~2 blocks) was too tight there — the tx lands
        // but the poll gives up first. Default to 180s and allow an override
        // via WHISTLEBLOWER_ANCHOR_CONFIRM_SECS for slower/faster environments.
        let confirm_secs = std::env::var("WHISTLEBLOWER_ANCHOR_CONFIRM_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|s| *s > 0)
            .unwrap_or(180);
        Ok(Self {
            wallet_core,
            program,
            confirmation_timeout: Duration::from_secs(confirm_secs),
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
        AccountId::for_public_pda(&self.program.id(), &PdaSeed::new(cid_hash.0))
    }

    pub fn entry_pda_for_hash(&self, cid_hash: CidHash) -> AccountId {
        AccountId::for_public_pda(&self.program.id(), &PdaSeed::new(cid_hash.0))
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

        // NOTE: we deliberately do NOT gate success on a get_transaction(hash)
        // poll. On the public LEZ testnet a submitted public tx frequently never
        // surfaces via get_transaction even after it has landed and mutated
        // state (verified: tx hashes return None indefinitely while the entry
        // PDA fills correctly). The authoritative success signal is the
        // populated entry PDA, which the caller polls via `read_entry_until`.
        // We still do a brief best-effort confirmation poll so localnet keeps
        // its fast path and the hash is observed when the sequencer does expose
        // it, but a miss here is not an error.
        let poll_interval = Duration::from_millis(750);
        let best_effort_attempts = 8; // ~6s
        for _ in 0..best_effort_attempts {
            if let Ok(Some(_)) = self
                .wallet_core
                .sequencer_client
                .get_transaction(hash)
                .await
            {
                break;
            }
            tokio::time::sleep(poll_interval).await;
        }
        Ok(())
    }

    /// Poll an entry PDA until it decodes to an `AnchorEntry` or the
    /// confirmation timeout elapses. This is the authoritative confirmation
    /// for an anchor on the public testnet (see `submit_and_wait`).
    async fn read_entry_until(
        &self,
        pda: AccountId,
        label: &str,
    ) -> Result<AnchorEntry, AdapterError> {
        let poll_interval = Duration::from_millis(750);
        let max_attempts =
            (self.confirmation_timeout.as_millis() / poll_interval.as_millis()).max(1) as usize;
        for _ in 0..max_attempts {
            if let Some(entry) = self.read_entry(pda).await? {
                return Ok(entry);
            }
            tokio::time::sleep(poll_interval).await;
        }
        Err(AdapterError::retryable(format!(
            "{label}: entry-PDA still empty after {:?} (tx submitted but not yet reflected on-chain)",
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
            vec![pda],
            RegistryInstruction::AnchorOne {
                cid: cid.clone(),
                metadata_hash,
                anchor_timestamp: timestamp,
            },
            "anchor_one",
        )
        .await?;
        self.read_entry_until(pda, "anchor_one").await
    }

    async fn anchor_batch(
        &self,
        entries: Vec<(CanonicalCid, MetadataHash)>,
    ) -> Result<Vec<AnchorEntry>, AdapterError> {
        let pdas: Vec<AccountId> = entries
            .iter()
            .map(|(cid, _)| self.entry_pda_for(cid))
            .collect();
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
            out.push(self.read_entry_until(pda, "anchor_batch").await?);
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
