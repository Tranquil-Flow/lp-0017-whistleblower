//! Live integration test for `LezRegistryClient`.
//!
//! This test is `#[ignore]` because it requires a running local LEZ
//! sequencer with the `whistleblower-registry` program already deployed
//! and `NSSA_WALLET_HOME_DIR` pointing at a seeded wallet (typically
//! `.scaffold/wallet` from the project root).
//!
//! Run it with:
//!
//!     lgs localnet start
//!     lgs build && lgs deploy --program-path target/.../whistleblower_registry.bin
//!     export NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet
//!     cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
//!
//! Equivalent to the `anchor_spike` runner but exercised through the public
//! `RegistryClient` trait — proving that the indexing crate's adapter
//! boundary is wired up correctly.

use document_indexing::RegistryClient;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use wallet::WalletCore;
use whistleblower_core::{cid_hash as compute_cid_hash, CanonicalCid, MetadataHash};
use whistleblower_lez_adapter::LezRegistryClient;

fn run_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

#[tokio::test]
#[ignore = "requires lgs localnet + deployed program + NSSA_WALLET_HOME_DIR"]
async fn lez_adapter_anchor_one_then_query() {
    let wallet_core = Arc::new(
        WalletCore::from_env().expect("WalletCore::from_env failed — is NSSA_WALLET_HOME_DIR set?"),
    );
    let client = LezRegistryClient::new(wallet_core).expect("LezRegistryClient::new");
    let suffix = run_suffix();

    let cid = CanonicalCid::new(format!("bafy-adapter-test-{suffix}-one")).unwrap();
    let mh = MetadataHash([0xCD; 32]);

    let entry = client
        .anchor_one(cid.clone(), mh)
        .await
        .expect("anchor_one against live sequencer");
    assert_eq!(entry.cid, cid, "returned entry cid matches input");
    assert_eq!(entry.metadata_hash, mh, "metadata_hash matches");

    let queried = client
        .query_by_cid_hash(compute_cid_hash(&cid))
        .await
        .expect("query_by_cid_hash");
    assert_eq!(queried, Some(entry), "queried entry matches anchor result");
}

#[tokio::test]
#[ignore = "requires lgs localnet + deployed program + NSSA_WALLET_HOME_DIR"]
async fn lez_adapter_anchor_batch_idempotent() {
    let wallet_core = Arc::new(
        WalletCore::from_env().expect("WalletCore::from_env failed — is NSSA_WALLET_HOME_DIR set?"),
    );
    let client = LezRegistryClient::new(wallet_core).expect("LezRegistryClient::new");
    let suffix = run_suffix();

    let cid_a = CanonicalCid::new(format!("bafy-adapter-test-{suffix}-batch-a")).unwrap();
    let cid_b = CanonicalCid::new(format!("bafy-adapter-test-{suffix}-batch-b")).unwrap();
    let mh_a = MetadataHash([0xAA; 32]);
    let mh_b = MetadataHash([0xBB; 32]);

    // First anchor cid_a alone.
    client
        .anchor_one(cid_a.clone(), mh_a)
        .await
        .expect("anchor_one(cid_a)");

    // Then batch with both — cid_a must be skipped silently, cid_b inserted.
    let entries = client
        .anchor_batch(vec![(cid_a.clone(), mh_a), (cid_b.clone(), mh_b)])
        .await
        .expect("anchor_batch mixed");
    assert_eq!(entries.len(), 2, "batch should return 2 entries");
    assert_eq!(entries[0].cid, cid_a);
    assert_eq!(entries[1].cid, cid_b);
}
