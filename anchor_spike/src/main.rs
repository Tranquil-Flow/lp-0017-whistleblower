//! Task 1.0B — Registry idempotency spike against a real LEZ sequencer.
//!
//! Updated to PDA-per-CID semantics (one account per anchored CID, derived
//! from `(program_id, PdaSeed::new(cid_hash))`). Now exercised through the
//! `LezRegistryClient` adapter so the spike doubles as the adapter's smoke
//! test in addition to the live integration tests in `tests/live_registry.rs`.
//!
//! 1. First `anchor_one(cid_a)` creates the entry.
//! 2. Second `anchor_one(cid_a)` is a no-op success.
//! 3. `anchor_batch` with mixed existing/new CIDs is a partial-success no-error.
//! 4. `anchor_batch` with 10 fresh CIDs lands in one transaction.
//!
//! Run order:
//!     lgs localnet start
//!     lgs build && lgs deploy --program-path target/.../whistleblower_registry.bin
//!     export NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet
//!     cargo run --release -p anchor-spike

use anyhow::{bail, Context, Result};
use document_indexing::RegistryClient;
use nssa_core::program::ProgramId;
use std::sync::Arc;
use wallet::WalletCore;
use whistleblower_core::{cid_hash as compute_cid_hash, CanonicalCid, MetadataHash};
use whistleblower_lez_adapter::LezRegistryClient;

/// Per-run unique suffix so the CIDs we anchor don't collide with CIDs from
/// prior runs (each anchored CID lives in its own PDA — collisions would
/// just exercise the no-op path, but distinct CIDs make the spike's
/// before/after deltas easier to read).
fn run_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

fn _program_id_hex(id: &ProgramId) -> String {
    let mut out = String::with_capacity(64);
    for word in id {
        for byte in word.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

fn make_cid(suffix: &str) -> CanonicalCid {
    CanonicalCid::new(format!("bafy-spike-{suffix}")).expect("non-empty test cid")
}

fn make_metadata_hash(seed: u8) -> MetadataHash {
    MetadataHash([seed; 32])
}

#[tokio::main]
async fn main() -> Result<()> {
    let wallet_core = Arc::new(
        WalletCore::from_env().context("WalletCore::from_env failed — set NSSA_WALLET_HOME_DIR")?,
    );
    let client = LezRegistryClient::new(wallet_core.clone()).context("LezRegistryClient::new")?;

    let suffix = run_suffix();
    let cid_a = make_cid(&format!("{suffix}-alpha"));
    let cid_b = make_cid(&format!("{suffix}-bravo"));
    let mh_a = make_metadata_hash(0x01);
    let mh_b = make_metadata_hash(0x02);

    println!("PDA-per-CID design — each anchored CID lives in its own LEZ account.");
    println!("entry_pda(cid_a) = {}", client.entry_pda_for(&cid_a));
    println!("entry_pda(cid_b) = {}", client.entry_pda_for(&cid_b));
    println!();

    println!("[1/4] anchor_one(cid_a) — first anchor, fresh PDA");
    let entry1 = client
        .anchor_one(cid_a.clone(), mh_a)
        .await
        .context("anchor_one(cid_a)")?;
    if entry1.cid != cid_a {
        bail!(
            "test 1: returned entry cid mismatch ({} vs {})",
            entry1.cid.as_str(),
            cid_a.as_str()
        );
    }
    if entry1.metadata_hash != mh_a {
        bail!("test 1: returned metadata_hash mismatch");
    }
    println!(
        "  ✓ entry stored at PDA, anchor_timestamp = {}\n",
        entry1.anchor_timestamp
    );

    println!("[2/4] anchor_one(cid_a) again — duplicate, expect no-op success");
    let entry2 = client
        .anchor_one(cid_a.clone(), mh_a)
        .await
        .context("duplicate anchor_one(cid_a)")?;
    if entry2.anchor_timestamp != entry1.anchor_timestamp {
        bail!(
            "test 2: duplicate anchor must preserve original timestamp (got {} vs {})",
            entry2.anchor_timestamp,
            entry1.anchor_timestamp
        );
    }
    println!(
        "  ✓ original timestamp preserved ({})\n",
        entry2.anchor_timestamp
    );

    println!("[3/4] anchor_batch([cid_a, cid_b]) — mixed existing + new in one tx");
    let mixed = client
        .anchor_batch(vec![(cid_a.clone(), mh_a), (cid_b.clone(), mh_b)])
        .await
        .context("anchor_batch mixed")?;
    if mixed.len() != 2 {
        bail!("test 3: expected 2 entries returned, got {}", mixed.len());
    }
    if mixed[0].cid != cid_a || mixed[1].cid != cid_b {
        bail!("test 3: entry order or content mismatch");
    }
    // cid_a should still have the ORIGINAL timestamp (no-op), cid_b is fresh.
    if mixed[0].anchor_timestamp != entry1.anchor_timestamp {
        bail!("test 3: cid_a's original timestamp was overwritten by the batch");
    }
    println!("  ✓ cid_a no-op'd, cid_b inserted in same tx\n");

    println!("[4/4] anchor_batch(10 fresh CIDs) — single tx with 10 distinct accounts");
    let fresh: Vec<(CanonicalCid, MetadataHash)> = (10..20)
        .map(|i| {
            (
                make_cid(&format!("{suffix}-{i}")),
                make_metadata_hash(i as u8),
            )
        })
        .collect();
    let fresh_count = fresh.len();
    let result = client
        .anchor_batch(fresh.clone())
        .await
        .context("anchor_batch(10 fresh)")?;
    if result.len() != fresh_count {
        bail!(
            "test 4: expected {fresh_count} entries returned, got {}",
            result.len()
        );
    }
    println!("  ✓ {} entries anchored in one tx\n", result.len());

    println!("[bonus] query_by_cid_hash(cid_a) — direct PDA read, no tx");
    let queried = client
        .query_by_cid_hash(compute_cid_hash(&cid_a))
        .await
        .context("query_by_cid_hash")?;
    match queried {
        Some(entry) if entry.cid == cid_a => {
            println!("  ✓ query returned cid_a's entry without a tx\n");
        }
        Some(other) => bail!("query returned wrong entry: {:?}", other),
        None => bail!("query returned None for known anchored cid"),
    }

    println!("✅ Task 1.0B spike PASSED on PDA-per-CID design");
    println!("   Anchored {} fresh CIDs total this run.", fresh_count + 2);
    Ok(())
}
