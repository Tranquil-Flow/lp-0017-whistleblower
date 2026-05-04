use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::program::{
    read_nssa_inputs, AccountPostState, Claim, ProgramInput, ProgramOutput,
};
use whistleblower_core::{
    cid_hash as compute_cid_hash, AnchorEntry, CanonicalCid, CidHash, MetadataHash,
    RegistryInstruction,
};

/// Wire-format instruction the host sends. Borsh-encoded `RegistryInstruction`
/// from `whistleblower-core`. We accept it as raw bytes so the guest controls
/// its own deserialization (and so the wire format stays explicit, not coupled
/// to the framework's serde encoding).
type GuestInstruction = Vec<u8>;

/// On-chain state held in the single registry-root account. Capacity is
/// bounded by LEZ's per-account size limit; for LP-0017's spike scope this is
/// fine. If we need to grow past a few thousand entries we'll switch to
/// bucketed accounts (see ARCHITECTURE.md).
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct RegistryStateOnChain {
    pub entries: Vec<AnchorEntry>,
}

impl RegistryStateOnChain {
    fn contains(&self, cid_hash: &CidHash) -> bool {
        self.entries.iter().any(|e| &e.cid_hash == cid_hash)
    }

    /// Idempotent insert. Returns `true` if the entry was newly inserted,
    /// `false` if a matching `cid_hash` was already present.
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

    // The registry program operates on exactly one account — the registry-root PDA.
    let [pre_state] = pre_states
        .try_into()
        .unwrap_or_else(|_| panic!("expected exactly one registry account in pre_states"));

    // Decode the host-supplied instruction.
    let registry_instruction = RegistryInstruction::try_from_slice(&instruction_bytes)
        .unwrap_or_else(|e| panic!("malformed registry instruction: {}", e));

    // Decode current registry state. An uninitialized account has empty data.
    let current_bytes: Vec<u8> = pre_state.account.data.clone().into();
    let mut state: RegistryStateOnChain = if current_bytes.is_empty() {
        RegistryStateOnChain::default()
    } else {
        BorshDeserialize::try_from_slice(&current_bytes)
            .unwrap_or_else(|e| panic!("corrupt registry state: {}", e))
    };

    // Apply the instruction. `upsert` is idempotent — re-anchoring an existing
    // CID is a no-op success, satisfying spec line 37.
    state.apply(registry_instruction);

    // Encode the new state into a fresh post-state account.
    let new_bytes = borsh::to_vec(&state).expect("registry state encoding fits");
    let mut post_account = pre_state.account.clone();
    post_account.data = new_bytes
        .try_into()
        .expect("registry state size within account-data limit");

    // `new_claimed_if_default` is the magic: claims the account if it's still
    // in default (unowned) state, otherwise just emits the state update.
    // This means the same handler handles both first-call init and subsequent
    // mutations — no separate `initialize` instruction needed.
    let post_state = AccountPostState::new_claimed_if_default(post_account, Claim::Authorized);

    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_data,
        vec![pre_state],
        vec![post_state],
    )
    .write();
}
