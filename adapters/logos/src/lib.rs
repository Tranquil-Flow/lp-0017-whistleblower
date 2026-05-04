//! Real Logos Storage + Delivery adapters backed by the `logoscore` runtime.
//!
//! **Status: scaffold only.** This crate is not yet wired into the workspace
//! `members` list — it exists as a deliberate placeholder for the next
//! session that takes Phase 1.7 forward (real Logos Core integration).
//! See `adapters/logos/README.md` for the bring-up plan.
//!
//! ## Design
//!
//! The Logos Core C++ modules (storage_module, delivery_module) are loaded
//! into the `logoscore` binary at runtime. We invoke `logoscore` as a
//! subprocess, send commands via either `-c` flags (one-shot) or stdin
//! (long-lived), and parse the events emitted to stderr.
//!
//! ## Why subprocess and not direct FFI
//!
//! The modules are Qt6 plugins (Q_INVOKABLE / Q_PLUGIN_METADATA). Calling
//! them from Rust would require:
//!   1. A Qt event loop bound into the Rust process — heavy build, fragile
//!      across Qt versions
//!   2. A C++ shim wrapping the Qt API in a C ABI — extra surface to maintain
//!   3. Subprocess + parse stderr — simple, robust, narrow contract
//!
//! Option 3 is sufficient for the headless batch-anchor CLI. The Basecamp UI
//! plugin (Phase 1.7 proper, still pending) targets a different consumer
//! (Qt/QML) and uses option 2's spirit on the C++ side.
//!
//! ## Build prerequisites (out of tree)
//!
//! - nix (Determinate) installed and configured
//! - `nix build github:logos-co/logos-liblogos --out-link ./logos`
//!     — provides `./logos/bin/logoscore`
//! - `nix build` of `logos-co/logos-storage-module` and `-delivery-module`
//!   produces the `.dylib` files
//! - `storage_config.json` and `delivery_config.json` with peer/IPFS/RLN
//!   configuration
//!
//! See repo `BUGS_FILED.md` and `~/.claude/projects/-Users-evinova-Projects/memory/reference_logos_repos.md`
//! for the verified install order.

#![deny(unsafe_code)]

// Stub. Real impl lands when the next session has logoscore + module dylibs
// available — likely after building them on the M4 Pro and SCPing back the
// artifacts. See README.md in this dir for the bring-up sequence.

pub struct LogoscoreStorageAdapter;
pub struct LogoscoreDeliveryAdapter;

impl LogoscoreStorageAdapter {
    pub fn new() -> Self {
        unimplemented!("see adapters/logos/README.md — Phase 1.7 work");
    }
}

impl LogoscoreDeliveryAdapter {
    pub fn new() -> Self {
        unimplemented!("see adapters/logos/README.md — Phase 1.7 work");
    }
}
