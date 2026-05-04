use borsh::BorshDeserialize;
use whistleblower_core::{CanonicalCid, MetadataHash, RegistryInstruction, RegistryState};

fn cid(n: usize) -> CanonicalCid {
    CanonicalCid::new(format!("bafy-instruction-{n:02}")).unwrap()
}

fn metadata(n: u8) -> MetadataHash {
    MetadataHash([n; 32])
}

#[test]
fn anchor_one_instruction_borsh_round_trips_for_guest_input() {
    let instruction = RegistryInstruction::AnchorOne {
        cid: cid(1),
        metadata_hash: metadata(1),
        anchor_timestamp: 100,
    };

    let bytes = borsh::to_vec(&instruction).unwrap();
    let decoded = RegistryInstruction::try_from_slice(&bytes).unwrap();

    assert_eq!(decoded, instruction);
}

#[test]
fn anchor_batch_instruction_borsh_round_trips_for_guest_input() {
    let instruction = RegistryInstruction::AnchorBatch {
        entries: (0..10).map(|n| (cid(n), metadata(n as u8))).collect(),
        anchor_timestamp: 200,
    };

    let bytes = borsh::to_vec(&instruction).unwrap();
    let decoded = RegistryInstruction::try_from_slice(&bytes).unwrap();

    assert_eq!(decoded, instruction);
}

#[test]
fn registry_state_applies_anchor_batch_instruction() {
    let mut registry = RegistryState::default();
    let instruction = RegistryInstruction::AnchorBatch {
        entries: (0..10).map(|n| (cid(n), metadata(n as u8))).collect(),
        anchor_timestamp: 300,
    };

    let outcome = registry.apply(instruction).unwrap().into_batch().unwrap();

    assert_eq!(outcome.inserted, 10);
    assert_eq!(outcome.skipped_existing, 0);
    assert_eq!(outcome.entries.len(), 10);
}

#[test]
fn registry_state_apply_preserves_duplicate_no_op_semantics() {
    let mut registry = RegistryState::default();
    let cid = cid(42);
    let first = registry
        .apply(RegistryInstruction::AnchorOne {
            cid: cid.clone(),
            metadata_hash: metadata(1),
            anchor_timestamp: 100,
        })
        .unwrap()
        .into_one()
        .unwrap();
    let duplicate = registry
        .apply(RegistryInstruction::AnchorOne {
            cid,
            metadata_hash: metadata(9),
            anchor_timestamp: 999,
        })
        .unwrap()
        .into_one()
        .unwrap();

    assert!(first.inserted);
    assert!(!duplicate.inserted);
    assert_eq!(duplicate.entry, first.entry);
}
