//! `whistleblower-registry` LEZ program — production-shaped PDA-per-CID design.
//!
//! Each anchored CID lives in its own LEZ account, derived as a PDA from
//! `(self_program_id, PdaSeed::new(cid_hash))`. This gives:
//!
//!   - O(1) per-anchor cost (each tx touches only the entries it adds)
//!   - Unbounded registry capacity (new account per CID, no shared blob)
//!   - Built-in idempotency: re-anchoring an existing PDA finds it already
//!     program-owned (data non-empty), returns success without state change
//!   - Trivial off-chain query: derive the PDA from `cid_hash`, fetch
//!     account.data, decode `AnchorEntry` directly. No tx needed.
//!
//! Wire format: host borsh-encodes a `RegistryInstruction`, sends it as
//! `Vec<u8>` via nssa's serde. The host is responsible for pre-deriving
//! all the entry-account PDAs and including them in the transaction's
//! `account_ids` in the SAME ORDER as `RegistryInstruction::AnchorBatch.entries`
//! (or the single PDA for `AnchorOne`). The guest verifies that order by
//! re-deriving each PDA from the entry's CID and asserting it matches the
//! pre_state's `account_id`.

use borsh::BorshDeserialize;
use nssa_core::account::{AccountId, AccountWithMetadata};
use nssa_core::program::{
    read_nssa_inputs, AccountPostState, Claim, PdaSeed, ProgramInput, ProgramOutput,
};
use whistleblower_core::{
    cid_hash as compute_cid_hash, AnchorEntry, CanonicalCid, MetadataHash, RegistryInstruction,
};

type GuestInstruction = Vec<u8>;

/// Process one (cid, metadata_hash) entry against its corresponding pre_state
/// account. Returns the post-state to emit. Idempotent: if the account is
/// already program-owned (data non-empty), returns it unchanged.
fn process_entry(
    self_program_id: &[u32; 8],
    pre: &AccountWithMetadata,
    cid: CanonicalCid,
    metadata_hash: MetadataHash,
    anchor_timestamp: u64,
) -> AccountPostState {
    let cid_hash = compute_cid_hash(&cid);

    // Verify the host pre-derived the right PDA for this CID. The runtime
    // also checks PDA-derivation when the claim is processed, but doing it
    // here gives a clearer error message and prevents wasted writes.
    // rc3 (v0.2.0-rc3) replaced the `(&ProgramId, &PdaSeed) -> AccountId` From
    // impl with the explicit `AccountId::for_public_pda` constructor.
    let expected_pda: AccountId =
        AccountId::for_public_pda(self_program_id, &PdaSeed::new(cid_hash.0));
    assert!(
        pre.account_id == expected_pda,
        "WL-REG: pre_state account_id does not match expected PDA for cid",
    );

    // Already-anchored detection: program-owned PDAs have non-empty data.
    let pre_data: Vec<u8> = pre.account.data.clone().into();
    if !pre_data.is_empty() {
        // No-op success — return the existing state unchanged, no claim needed.
        // This is the idempotency guarantee the spike + spec require.
        return AccountPostState::new(pre.account.clone());
    }

    // Fresh PDA — encode the AnchorEntry, claim the account.
    let entry = AnchorEntry {
        cid,
        cid_hash,
        metadata_hash,
        anchor_timestamp,
    };
    let bytes = borsh::to_vec(&entry).expect("AnchorEntry encoding fits");
    let mut post_account = pre.account.clone();
    post_account.data = bytes
        .try_into()
        .expect("AnchorEntry size within account-data limit");

    AccountPostState::new_claimed_if_default(post_account, Claim::Pda(PdaSeed::new(cid_hash.0)))
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

    let registry_instruction = RegistryInstruction::try_from_slice(&instruction_bytes)
        .unwrap_or_else(|e| panic!("WL-REG: malformed instruction: {}", e));

    let post_states: Vec<AccountPostState> = match &registry_instruction {
        RegistryInstruction::AnchorOne {
            cid,
            metadata_hash,
            anchor_timestamp,
        } => {
            assert!(
                pre_states.len() == 1,
                "WL-REG: AnchorOne expects exactly 1 pre_state, got {}",
                pre_states.len()
            );
            vec![process_entry(
                &self_program_id,
                &pre_states[0],
                cid.clone(),
                *metadata_hash,
                *anchor_timestamp,
            )]
        }
        RegistryInstruction::AnchorBatch {
            entries,
            anchor_timestamp,
        } => {
            assert!(
                entries.len() == pre_states.len(),
                "WL-REG: AnchorBatch entry count {} != pre_states count {}",
                entries.len(),
                pre_states.len()
            );
            entries
                .iter()
                .zip(pre_states.iter())
                .map(|((cid, metadata_hash), pre)| {
                    process_entry(
                        &self_program_id,
                        pre,
                        cid.clone(),
                        *metadata_hash,
                        *anchor_timestamp,
                    )
                })
                .collect()
        }
    };

    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_data,
        pre_states,
        post_states,
    )
    .write();
}
