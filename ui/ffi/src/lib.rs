//! C FFI for the LP-0017 Whistleblower Basecamp plugin.
//!
//! Architectural split:
//! - **C++ Backend** (in `ui/src/WhistleblowerBackend.{h,cpp}`) holds the
//!   `LogosAPI*` and uses it to call into the **storage_module** and
//!   **delivery_module** Q_INVOKABLE methods. That side uploads files,
//!   gets CIDs back via `storageUploadDone`, broadcasts envelopes via
//!   `delivery_module.send`, and listens for incoming envelopes.
//! - **This FFI** is the Rust side. It receives `(cid, metadata)` from C++
//!   after the upload+broadcast has completed, anchors the CID on the
//!   LEZ registry via the `LezRegistryClient` adapter, and returns the
//!   `AnchorEntry` JSON. It also exposes a query path for reading existing
//!   entries from the registry without a transaction.
//!
//! Why split this way? The Logos modules are Qt6 plugins and live in the
//! Basecamp host process. They're naturally accessed from C++. The
//! on-chain side (build a `RegistryInstruction`, sign, submit, poll for
//! confirmation, decode response) is much cleaner in Rust where the
//! `nssa`/`wallet` crates are first-class — and we already have a tested
//! `LezRegistryClient` doing exactly that.
//!
//! Wire format:
//!   Every fn takes a JSON args string, returns a JSON result string.
//!   Args at minimum include:
//!     { "wallet_path": "...", "sequencer_url": "..." }
//!   Callers may also pass `program_bin`; the Basecamp plugin passes the
//!   bundled deployed registry `.bin` so PDA derivation matches testnet.
//!
//! Caller MUST free returned strings via `whistleblower_free_string`.

use document_indexing::RegistryClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Arc;
use wallet::WalletCore;
use whistleblower_core::{cid_hash as compute_cid_hash, CanonicalCid, CidHash, MetadataHash};
use whistleblower_lez_adapter::LezRegistryClient;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err("null pointer".into());
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| format!("invalid UTF-8: {}", e))
}

fn to_cstring(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| {
            CString::new(r#"{"success":false,"error":"null byte in output"}"#).unwrap()
        })
        .into_raw()
}

fn error_json(msg: &str) -> *mut c_char {
    to_cstring(json!({"success": false, "error": msg}).to_string())
}

fn ffi_call(f: impl FnOnce() -> Result<String, String> + std::panic::UnwindSafe) -> *mut c_char {
    match std::panic::catch_unwind(f) {
        Ok(Ok(r)) => to_cstring(r),
        Ok(Err(e)) => error_json(&e),
        Err(e) => {
            let msg = e
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("<unknown panic>");
            error_json(&format!("panic: {}", msg))
        }
    }
}

fn init_wallet(v: &Value) -> Result<Arc<WalletCore>, String> {
    if let Some(p) = v["wallet_path"].as_str() {
        std::env::set_var("NSSA_WALLET_HOME_DIR", p);
    }
    if let Some(u) = v["sequencer_url"].as_str() {
        std::env::set_var("NSSA_SEQUENCER_URL", u);
    }
    WalletCore::from_env()
        .map(Arc::new)
        .map_err(|e| format!("wallet init: {}", e))
}

fn registry_client(v: &Value, wallet: Arc<WalletCore>) -> Result<LezRegistryClient, String> {
    let program_bin = v["program_bin"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| std::env::var("WHISTLEBLOWER_PROGRAM_BIN").ok())
        .or_else(|| std::env::var("WL_PROGRAM_BIN").ok());

    match program_bin {
        Some(path) if !path.trim().is_empty() => {
            let path = PathBuf::from(path);
            let elf = std::fs::read(&path)
                .map_err(|e| format!("read program_bin {}: {}", path.display(), e))?;
            if elf.is_empty() {
                return Err(format!("program_bin {} is empty", path.display()));
            }
            LezRegistryClient::with_program_bytes(wallet, elf).map_err(|e| {
                format!(
                    "LezRegistryClient::with_program_bytes({}): {}",
                    path.display(),
                    e.message
                )
            })
        }
        _ => LezRegistryClient::new(wallet)
            .map_err(|e| format!("LezRegistryClient::new: {}", e.message)),
    }
}

fn parse_metadata_hash(s: &str) -> Result<MetadataHash, String> {
    let s = s.trim_start_matches("0x");
    if s.len() != 64 {
        return Err(format!(
            "metadata_hash must be 64 hex chars, got {}",
            s.len()
        ));
    }
    let bytes = hex::decode(s).map_err(|e| format!("invalid hex: {}", e))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(MetadataHash(out))
}

fn parse_cid_hash(s: &str) -> Result<CidHash, String> {
    let s = s.trim_start_matches("0x");
    if s.len() != 64 {
        return Err(format!("cid_hash must be 64 hex chars, got {}", s.len()));
    }
    let bytes = hex::decode(s).map_err(|e| format!("invalid hex: {}", e))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(CidHash(out))
}

fn entry_to_json(entry: whistleblower_core::AnchorEntry) -> Value {
    json!({
        "cid": entry.cid.as_str(),
        "cid_hash": hex::encode(entry.cid_hash.0),
        "metadata_hash": hex::encode(entry.metadata_hash.0),
        "anchor_timestamp": entry.anchor_timestamp,
    })
}

// ── anchor_one ────────────────────────────────────────────────────────────────
//
// C++ Backend, after a successful storage upload + delivery broadcast,
// hands the resulting CID + metadata_hash to this fn to anchor on-chain.
// Args JSON shape:
//   { "wallet_path": "...", "sequencer_url": "...",
//     "cid": "bafy...", "metadata_hash_hex": "<64 hex>" }

#[no_mangle]
pub extern "C" fn whistleblower_anchor_one(args_json: *const c_char) -> *mut c_char {
    let args = match cstr_to_str(args_json) {
        Ok(s) => s.to_owned(),
        Err(e) => return error_json(&e),
    };
    ffi_call(move || anchor_one_impl(&args))
}

fn anchor_one_impl(args: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(args).map_err(|e| format!("invalid JSON: {}", e))?;
    let wallet = init_wallet(&v)?;
    let cid = CanonicalCid::new(v["cid"].as_str().ok_or("missing cid")?.to_string())
        .map_err(|e| format!("invalid cid: {}", e))?;
    let metadata_hash = parse_metadata_hash(
        v["metadata_hash_hex"]
            .as_str()
            .ok_or("missing metadata_hash_hex")?,
    )?;

    let client = registry_client(&v, wallet)?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {}", e))?;
    let entry = rt
        .block_on(client.anchor_one(cid, metadata_hash))
        .map_err(|e| format!("anchor_one: {}", e.message))?;

    Ok(json!({"success": true, "entry": entry_to_json(entry)}).to_string())
}

// ── query_by_cid ──────────────────────────────────────────────────────────────
//
// Read-path: derive PDA, fetch + decode AnchorEntry. No tx. Used by the QML
// to confirm a CID is on-chain without broadcasting another publish.

#[no_mangle]
pub extern "C" fn whistleblower_query_by_cid(args_json: *const c_char) -> *mut c_char {
    let args = match cstr_to_str(args_json) {
        Ok(s) => s.to_owned(),
        Err(e) => return error_json(&e),
    };
    ffi_call(move || query_by_cid_impl(&args))
}

fn query_by_cid_impl(args: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(args).map_err(|e| format!("invalid JSON: {}", e))?;
    let wallet = init_wallet(&v)?;
    let cid = CanonicalCid::new(v["cid"].as_str().ok_or("missing cid")?.to_string())
        .map_err(|e| format!("invalid cid: {}", e))?;
    let cid_hash = compute_cid_hash(&cid);

    let client = registry_client(&v, wallet)?;
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {}", e))?;
    let result = rt
        .block_on(client.query_by_cid_hash(cid_hash))
        .map_err(|e| format!("query: {}", e.message))?;

    Ok(json!({
        "success": true,
        "found": result.is_some(),
        "entry": result.map(entry_to_json),
    })
    .to_string())
}

// ── query_by_cid_hash ─────────────────────────────────────────────────────────
//
// Hash-keyed query (when the caller already has the cid_hash, e.g. from the
// delivery broadcast it's listening on).

#[no_mangle]
pub extern "C" fn whistleblower_query_by_cid_hash(args_json: *const c_char) -> *mut c_char {
    let args = match cstr_to_str(args_json) {
        Ok(s) => s.to_owned(),
        Err(e) => return error_json(&e),
    };
    ffi_call(move || query_by_cid_hash_impl(&args))
}

fn query_by_cid_hash_impl(args: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(args).map_err(|e| format!("invalid JSON: {}", e))?;
    let wallet = init_wallet(&v)?;
    let cid_hash = parse_cid_hash(v["cid_hash_hex"].as_str().ok_or("missing cid_hash_hex")?)?;

    let client = registry_client(&v, wallet)?;
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {}", e))?;
    let result = rt
        .block_on(client.query_by_cid_hash(cid_hash))
        .map_err(|e| format!("query: {}", e.message))?;

    Ok(json!({
        "success": true,
        "found": result.is_some(),
        "entry": result.map(entry_to_json),
    })
    .to_string())
}

// ── compute_metadata_hash ─────────────────────────────────────────────────────
//
// Helper for the C++ side: build the canonical envelope JSON locally then
// hand it here to get the hash. This keeps the canonical-encoding logic
// in one place (whistleblower-core).

#[derive(Debug, Serialize, Deserialize)]
struct MetadataInputs {
    cid: String,
    title: String,
    description: String,
    content_type: String,
    size_bytes: u64,
    timestamp_unix: u64,
    tags: Vec<String>,
}

#[no_mangle]
pub extern "C" fn whistleblower_compute_metadata_hash(args_json: *const c_char) -> *mut c_char {
    let args = match cstr_to_str(args_json) {
        Ok(s) => s.to_owned(),
        Err(e) => return error_json(&e),
    };
    ffi_call(move || compute_metadata_hash_impl(&args))
}

fn compute_metadata_hash_impl(args: &str) -> Result<String, String> {
    use whistleblower_core::MetadataEnvelopeV1;
    let inputs: MetadataInputs =
        serde_json::from_str(args).map_err(|e| format!("invalid JSON: {}", e))?;
    let cid = CanonicalCid::new(inputs.cid.clone()).map_err(|e| format!("invalid cid: {}", e))?;
    let envelope = MetadataEnvelopeV1 {
        version: 1,
        cid,
        title: inputs.title,
        description: inputs.description,
        content_type: inputs.content_type,
        size_bytes: inputs.size_bytes,
        timestamp: inputs.timestamp_unix,
        tags: inputs.tags,
    };
    let envelope_bytes = envelope
        .canonical_json_bytes()
        .map_err(|e| format!("envelope encode: {}", e))?;
    let metadata_hash = envelope
        .metadata_hash()
        .map_err(|e| format!("hash: {}", e))?;
    Ok(json!({
        "success": true,
        "envelope_bytes_b64": base64_encode(&envelope_bytes),
        "metadata_hash_hex": hex::encode(metadata_hash.0),
    })
    .to_string())
}

// Tiny base64 encoder so we don't pull a base64 dep just for one call.
// Standard MIME alphabet, no line wrapping.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHA[(b0 >> 2) as usize] as char);
        out.push(ALPHA[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHA[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHA[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ── version + free ────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn whistleblower_version() -> *mut c_char {
    to_cstring("0.1.0".to_string())
}

/// # Safety
///
/// `s` must be a pointer previously returned by one of this crate's FFI
/// functions (which all hand out `CString`s allocated by Rust), and must
/// not have already been freed. Passing a null pointer is a no-op.
#[no_mangle]
pub unsafe extern "C" fn whistleblower_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn metadata_hash_is_deterministic() {
        let args = r#"{
            "cid": "bafy-test",
            "title": "doc",
            "description": "",
            "content_type": "text/plain",
            "size_bytes": 10,
            "timestamp_unix": 1725000000,
            "tags": []
        }"#;
        let r1 = compute_metadata_hash_impl(args).unwrap();
        let r2 = compute_metadata_hash_impl(args).unwrap();
        assert_eq!(r1, r2);
        let v: Value = serde_json::from_str(&r1).unwrap();
        let hash = v["metadata_hash_hex"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn parse_metadata_hash_strips_0x() {
        let h = "0x".to_string() + &"a".repeat(64);
        let parsed = parse_metadata_hash(&h).unwrap();
        assert_eq!(parsed.0, [0xaa; 32]);
    }

    #[test]
    fn null_args_return_error() {
        let raw = whistleblower_anchor_one(std::ptr::null());
        let s = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_owned();
        unsafe { whistleblower_free_string(raw) };
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["success"], false);
    }
}
