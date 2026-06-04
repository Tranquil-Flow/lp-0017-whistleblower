//! Parse-only SPEL program definition for `whistleblower-registry`.
//!
//! This file is the source for `spel generate-idl` and is **never compiled**:
//! it is not a crate, not a workspace member, and not a `[[bin]]`. The
//! production guest at `methods/guest/src/bin/whistleblower_registry.rs` stays
//! raw `nssa_core` (it is the deployed ELF, ImageID `54c7f793…aa91`).
//!
//! `spel generate-idl` AST-parses this file (it does not compile it — see
//! `spel-framework-core/src/idl_gen.rs`), so depending on `spel-framework`
//! here pulls nothing into the guest build. The earlier claim that an IDL
//! could not be generated "because spel-framework forces bonsai-sdk into the
//! riscv32im build" was a non-sequitur: IDL generation never compiles the guest.
//!
//! The instruction/account shapes below mirror `core/src/lib.rs` exactly so the
//! generated IDL is wire-faithful:
//!   - `RegistryInstruction::AnchorOne   { cid, metadata_hash, anchor_timestamp }`
//!   - `RegistryInstruction::AnchorBatch { entries: Vec<(CanonicalCid, MetadataHash)>, anchor_timestamp }`
//!   - `AnchorEntry { cid, cid_hash, metadata_hash, anchor_timestamp }`
//! `CanonicalCid` / `MetadataHash` / `CidHash` are borsh-transparent newtypes
//! (`String` / `[u8;32]` / `[u8;32]`), so they are written as their inner types.
//! `BatchItem` is borsh-identical to the `(CanonicalCid, MetadataHash)` tuple
//! element (SPEL has no anonymous-tuple type, so a named struct is used).
//!
//! Regenerate the JSON with: `bash scripts/regen-idl.sh`
//!
//! KNOWN LIMITATION (filed upstream — see BUGS_FILED.md): the `anchor_one`
//! entry PDA seed is `sha256(CID_HASH_DOMAIN || cid)` = `core::cid_hash(cid)`.
//! SPEL's `IdlSeed` enum is `const | account | arg` only — it cannot express a
//! hashed seed, so the emitted seed shows `{kind:arg, path:cid}`. Derive the
//! real PDA via `whistleblower_core::cid_hash()` or the LEZ adapter's
//! `entry_pda_for(...)`, not `spel pda`.

use spel_framework::prelude::*;

pub const CID_HASH_DOMAIN: &str = "lp0017:cid:v1\0";
pub const DEFAULT_CONTENT_TOPIC: &str = "/lp0017-whistleblower/1/cids/json";

/// One anchored CID's on-chain account (one PDA per CID).
#[account_type]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct AnchorEntry {
    pub cid: String,
    pub cid_hash: [u8; 32],
    pub metadata_hash: [u8; 32],
    pub anchor_timestamp: u64,
}

/// One `(cid, metadata_hash)` pair in an `anchor_batch` call. Borsh-identical
/// to the deployed `(CanonicalCid, MetadataHash)` tuple element.
#[account_type]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct BatchItem {
    pub cid: String,
    pub metadata_hash: [u8; 32],
}

#[lez_program]
mod whistleblower_registry {
    #[allow(unused_imports)]
    use super::*;

    /// Anchor a single CID into its own PDA. Idempotent: re-anchoring an
    /// already-populated PDA is a no-op (handled in the guest).
    #[instruction]
    pub fn anchor_one(
        #[account(init, pda = arg("cid"))]
        entry: AccountWithMetadata,
        cid: String,
        metadata_hash: [u8; 32],
        anchor_timestamp: u64,
    ) -> SpelResult {
        Ok(SpelOutput::execute(vec![entry], vec![]))
    }

    /// Anchor >=10 CIDs in one transaction. The host lists the entry-account
    /// PDAs (`entry_pdas`) in the same order as `entries`.
    #[instruction]
    pub fn anchor_batch(
        #[account(mut)]
        entry_pdas: Vec<AccountWithMetadata>,
        entries: Vec<BatchItem>,
        anchor_timestamp: u64,
    ) -> SpelResult {
        Ok(SpelOutput::execute(entry_pdas, vec![]))
    }
}
