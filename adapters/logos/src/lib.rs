//! Placeholder for a future headless Rust adapter against the Logos
//! Storage / Delivery modules.
//!
//! **This crate is intentionally empty.** It is not in the workspace
//! `members` list; nothing depends on it. The canonical real-Storage /
//! real-Delivery integration ships in `ui/` (the Basecamp UI plugin) via
//! Logos Core's in-process `LogosAPIClient`. See `README.md` in this
//! directory for the design discussion of the three viable paths to
//! headless integration and why they're all deferred.

#![deny(unsafe_code)]
