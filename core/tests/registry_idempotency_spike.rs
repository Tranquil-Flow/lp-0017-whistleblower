use whistleblower_core::{cid_hash, CanonicalCid, MetadataHash, RegistryState};

fn cid(n: usize) -> CanonicalCid {
    CanonicalCid::new(format!("bafy-spike-{n:02}")).unwrap()
}

fn metadata(n: u8) -> MetadataHash {
    MetadataHash([n; 32])
}

#[test]
fn duplicate_anchor_one_is_successful_no_op() {
    let mut registry = RegistryState::default();
    let cid = cid(1);
    let first = registry.anchor_one(cid.clone(), metadata(1), 100).unwrap();
    let second = registry.anchor_one(cid.clone(), metadata(9), 999).unwrap();

    assert!(first.inserted);
    assert!(!second.inserted);
    assert_eq!(second.entry, first.entry);
    assert_eq!(
        registry.query_by_cid_hash(cid_hash(&cid)).unwrap(),
        Some(first.entry)
    );
}

#[test]
fn anchor_batch_with_ten_new_cids_succeeds_in_one_call() {
    let mut registry = RegistryState::default();
    let entries = (0..10)
        .map(|n| (cid(n), metadata(n as u8)))
        .collect::<Vec<_>>();

    let result = registry.anchor_batch(entries, 200).unwrap();

    assert_eq!(result.entries.len(), 10);
    assert_eq!(result.inserted, 10);
    assert_eq!(result.skipped_existing, 0);
}

#[test]
fn anchor_batch_with_mixed_existing_and_new_cids_succeeds_and_only_creates_missing_entries() {
    let mut registry = RegistryState::default();
    let existing = cid(1);
    let original = registry
        .anchor_one(existing.clone(), metadata(1), 100)
        .unwrap()
        .entry;

    let result = registry
        .anchor_batch(
            vec![
                (existing.clone(), metadata(8)),
                (cid(2), metadata(2)),
                (cid(3), metadata(3)),
            ],
            300,
        )
        .unwrap();

    assert_eq!(result.entries.len(), 3);
    assert_eq!(result.inserted, 2);
    assert_eq!(result.skipped_existing, 1);
    assert_eq!(result.entries[0], original);
    assert_eq!(
        registry.query_by_cid_hash(cid_hash(&existing)).unwrap(),
        Some(original)
    );
}
