//! `whistleblower-registry` LEZ program — Task 1.1 (registry program proper).
//!
//! Operates on a single registry-root PDA. Each transaction supplies one
//! `RegistryInstruction` (borsh-encoded as `Vec<u8>`); the guest decodes it,
//! reads the current `RegistryStateOnChain` from the PDA's data, applies
//! the instruction (idempotent upsert per CID), and writes the new state back.
//!
//! Claim type is `Claim::Pda(REGISTRY_PDA_SEED_BYTES)` — the runtime confirms
//! the input account ID equals the PDA derived from this program's id and
//! the same seed, then assigns the account to this program. After first
//! claim the program owns the PDA and subsequent calls just mutate state.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::program::{
    read_nssa_inputs, AccountPostState, Claim, PdaSeed, ProgramInput, ProgramOutput,
};
use whistleblower_core::{
    cid_hash as compute_cid_hash, AnchorEntry, CanonicalCid, CidHash, MetadataHash,
    RegistryInstruction, REGISTRY_PDA_SEED_BYTES,
};

/// Wire-format instruction the host sends. Borsh-encoded `RegistryInstruction`
/// from `whistleblower-core`.
type GuestInstruction = Vec<u8>;

/// On-chain state held in the single registry-root PDA. Capacity is bounded
/// by LEZ's per-account size limit; for LP-0017's spike scope this is fine.
/// Bucketing is a follow-up if we ever need to grow past a few thousand entries.
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct RegistryStateOnChain {
    pub entries: Vec<AnchorEntry>,
}

impl RegistryStateOnChain {
    fn contains(&self, cid_hash: &CidHash) -> bool {
        self.entries.iter().any(|e| &e.cid_hash == cid_hash)
    }

    /// Idempotent insert. Returns `true` iff the entry was newly inserted.
    fn upsert(
        &mut self,
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
        anchor_timestamp: u64,
    ) -> bool {
        let cid_hash = compute_cid_hash(&cid);
        if self.contains(&cid_hash) {
            return false;
        }
        self.entries.push(AnchorEntry {
            cid,
            cid_hash,
            metadata_hash,
            anchor_timestamp,
        });
        true
    }

    fn apply(&mut self, instruction: RegistryInstruction) {
        match instruction {
            RegistryInstruction::AnchorOne {
                cid,
                metadata_hash,
                anchor_timestamp,
            } => {
                self.upsert(cid, metadata_hash, anchor_timestamp);
            }
            RegistryInstruction::AnchorBatch {
                entries,
                anchor_timestamp,
            } => {
                for (cid, metadata_hash) in entries {
                    self.upsert(cid, metadata_hash, anchor_timestamp);
                }
            }
        }
    }
}

fn main() {
    let (
        ProgramInput {
            self_program_id,
            caller_program_id,
            pre_states,
            instruction: instruction_bytes,
        },
        instruction_data,
    ) = read_nssa_inputs::<GuestInstruction>();

    let pre_state = pre_states
        .into_iter()
        .next()
        .expect("WL-REG: pre_states must have at least 1 entry");

    let registry_instruction =
        RegistryInstruction::try_from_slice(&instruction_bytes).unwrap_or_else(|e| {
            panic!("WL-REG: malformed registry instruction: {}", e)
        });

    // Decode current registry state. Empty data == fresh PDA (first claim).
    let current_bytes: Vec<u8> = pre_state.account.data.clone().into();
    let mut state: RegistryStateOnChain = if current_bytes.is_empty() {
        RegistryStateOnChain::default()
    } else {
        BorshDeserialize::try_from_slice(&current_bytes)
            .unwrap_or_else(|e| panic!("WL-REG: corrupt registry state: {}", e))
    };

    // Apply (idempotent — duplicate CIDs are no-op success).
    state.apply(registry_instruction);

    // Encode and emit post-state.
    let new_bytes = borsh::to_vec(&state).expect("registry state encoding fits");
    let mut post_account = pre_state.account.clone();
    post_account.data = new_bytes
        .try_into()
        .expect("registry state size within account-data limit");

    // First call: PDA is in default state -> claim it via Claim::Pda(seed),
    //   the runtime confirms the input account equals the PDA derived from
    //   (self_program_id, REGISTRY_PDA_SEED_BYTES) and assigns ownership.
    // Subsequent calls: PDA already owned by us -> emit a state update without
    //   re-requesting a claim (re-claiming would be rejected because
    //   `post.account.program_owner != DEFAULT_PROGRAM_ID` per nssa validation).
    let post_state = AccountPostState::new_claimed_if_default(
        post_account,
        Claim::Pda(PdaSeed::new(REGISTRY_PDA_SEED_BYTES)),
    );

    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_data,
        vec![pre_state],
        vec![post_state],
    )
    .write();
}
