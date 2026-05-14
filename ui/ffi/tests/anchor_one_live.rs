//! Live integration smoke for the FFI surface that the QML "Anchor on chain"
//! button drives via WhistleblowerBackend::anchorLast → anchorOneFfi.
//!
//! The unit tests in lib.rs exercise the FFI argument parsing and error paths.
//! The `whistleblower-lez-adapter` live tests exercise `Publisher::anchor_published`
//! against the deployed registry. This file closes the seam between them by
//! driving the C ABI exactly the way the C++ backend does — JSON args in,
//! JSON result out — against a running local sequencer.
//!
//! `#[ignore]` for the same reason as the adapter live tests: requires
//!   - `lgs localnet start` with the whistleblower_registry program deployed
//!   - `NSSA_WALLET_HOME_DIR` pointing at a seeded wallet (e.g. .scaffold/wallet)
//!
//! Run with:
//!
//!     NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
//!       cargo test -p whistleblower_ffi --release \
//!         --test anchor_one_live -- --ignored --nocapture

use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::time::{SystemTime, UNIX_EPOCH};

use whistleblower_ffi::{
    whistleblower_anchor_one, whistleblower_compute_metadata_hash, whistleblower_free_string,
    whistleblower_query_by_cid,
};

/// Call an FFI fn with a JSON string, return the JSON response as an owned String.
/// Mirrors `WhistleblowerBackend::callFfiRaw` on the C++ side.
fn call_ffi(
    f: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    args: &Value,
) -> Value {
    let cstr = CString::new(args.to_string()).expect("args contained null byte");
    let raw_out = unsafe { f(cstr.as_ptr()) };
    assert!(!raw_out.is_null(), "FFI returned null");
    let out = unsafe { CStr::from_ptr(raw_out) }
        .to_str()
        .expect("FFI returned non-UTF-8")
        .to_owned();
    unsafe { whistleblower_free_string(raw_out) };
    serde_json::from_str(&out).expect("FFI returned invalid JSON")
}

fn run_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string()
}

#[test]
#[ignore = "requires lgs localnet + deployed whistleblower_registry + NSSA_WALLET_HOME_DIR"]
fn anchor_one_via_ffi_against_live_sequencer() {
    // 1. Reproduce what WhistleblowerBackend::computeEnvelope feeds the hash fn.
    //    A fresh suffix per run keeps the CID unique so subsequent runs don't
    //    hit the "no-op idempotency" path (which would still pass but skip the
    //    fresh-anchor proof).
    let suffix = run_suffix();
    let cid = format!("bafy-ffi-live-{suffix}");
    let metadata_args = json!({
        "cid": cid,
        "title": "anchor button smoke",
        "description": "FFI-level proof of the QML Anchor button path",
        "content_type": "text/plain",
        "size_bytes": 42_u64,
        "timestamp_unix": 1_725_000_000_u64,
        "tags": ["lp-0017", "smoke"],
    });

    let hash_resp = call_ffi(whistleblower_compute_metadata_hash, &metadata_args);
    assert_eq!(hash_resp["success"], true, "compute_metadata_hash failed: {hash_resp}");
    let metadata_hash_hex = hash_resp["metadata_hash_hex"]
        .as_str()
        .expect("metadata_hash_hex missing")
        .to_owned();
    assert_eq!(metadata_hash_hex.len(), 64, "metadata_hash_hex must be 32-byte hex");

    // 2. Reproduce what WhistleblowerBackend::anchorOneFfi (line 298) sends.
    //    `wallet_path` / `sequencer_url` are optional — when omitted, init_wallet
    //    falls through to the existing env vars, which is how the C++ side
    //    runs inside Basecamp (env is already exported).
    let anchor_args = json!({
        "cid": cid,
        "metadata_hash_hex": metadata_hash_hex,
    });
    let anchor_resp = call_ffi(whistleblower_anchor_one, &anchor_args);
    assert_eq!(
        anchor_resp["success"], true,
        "whistleblower_anchor_one failed: {anchor_resp}"
    );

    // 3. Validate the exact JSON shape the C++ unpacks at WhistleblowerBackend.cpp:285-291.
    let entry = &anchor_resp["entry"];
    assert!(entry.is_object(), "entry must be an object: {anchor_resp}");
    let cid_hash = entry["cid_hash"]
        .as_str()
        .expect("entry.cid_hash must be a hex string");
    assert_eq!(cid_hash.len(), 64, "cid_hash must be 32-byte hex");
    let returned_metadata = entry["metadata_hash"]
        .as_str()
        .expect("entry.metadata_hash must be a hex string");
    assert_eq!(
        returned_metadata.to_lowercase(),
        metadata_hash_hex.to_lowercase(),
        "metadata_hash round-trip mismatch"
    );
    let anchor_timestamp = entry["anchor_timestamp"]
        .as_u64()
        .expect("anchor_timestamp must be numeric");
    assert!(
        anchor_timestamp > 0,
        "anchor_timestamp must be > 0: {anchor_resp}"
    );

    println!(
        "anchored {cid} via FFI; cid_hash={cid_hash}, anchor_timestamp={anchor_timestamp}"
    );

    // 4. Confirm the entry is queryable on-chain via the read path the
    //    Anchor button's follow-up display flow uses.
    let query_args = json!({ "cid": cid });
    let query_resp = call_ffi(whistleblower_query_by_cid, &query_args);
    assert_eq!(query_resp["success"], true, "query_by_cid failed: {query_resp}");
    assert_eq!(
        query_resp["found"], true,
        "freshly anchored CID was not found via query: {query_resp}"
    );
    let queried_entry = &query_resp["entry"];
    assert_eq!(queried_entry["cid_hash"], entry["cid_hash"]);
    assert_eq!(queried_entry["anchor_timestamp"], entry["anchor_timestamp"]);
}

#[test]
#[ignore = "requires lgs localnet + deployed whistleblower_registry + NSSA_WALLET_HOME_DIR"]
fn anchor_one_via_ffi_is_idempotent() {
    // Spec compliance for F4 idempotency through the same C ABI the UI uses:
    // re-anchoring an existing CID must return success, not error.
    let suffix = run_suffix();
    let cid = format!("bafy-ffi-idem-{suffix}");
    let metadata_args = json!({
        "cid": cid,
        "title": "idempotency smoke",
        "description": "",
        "content_type": "text/plain",
        "size_bytes": 1_u64,
        "timestamp_unix": 1_725_000_000_u64,
        "tags": [],
    });
    let hash_resp = call_ffi(whistleblower_compute_metadata_hash, &metadata_args);
    let metadata_hash_hex = hash_resp["metadata_hash_hex"].as_str().unwrap().to_owned();

    let anchor_args = json!({
        "cid": cid,
        "metadata_hash_hex": metadata_hash_hex,
    });

    let first = call_ffi(whistleblower_anchor_one, &anchor_args);
    assert_eq!(first["success"], true, "first anchor failed: {first}");
    let first_ts = first["entry"]["anchor_timestamp"].as_u64().unwrap();

    let second = call_ffi(whistleblower_anchor_one, &anchor_args);
    assert_eq!(second["success"], true, "second anchor must succeed: {second}");
    let second_ts = second["entry"]["anchor_timestamp"].as_u64().unwrap();
    assert_eq!(
        second_ts, first_ts,
        "idempotent re-anchor must return the original timestamp, not overwrite it"
    );
}
