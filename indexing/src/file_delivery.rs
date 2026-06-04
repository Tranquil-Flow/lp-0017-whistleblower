//! File-backed [`DeliveryClient`] — replays broadcast envelopes from a file as a
//! finite [`ReceivedEnvelope`] stream.
//!
//! ## Why this exists
//!
//! The permissionless batch anchor tool consumes CIDs through the
//! [`DeliveryClient`] trait. In production that trait is backed by a **live
//! Logos Delivery subscription** — but real Logos Delivery is Waku + RLN behind
//! the Logos Core `delivery_module`, reachable only as a Qt `logos_host` process
//! over QtRemoteObjects (see `adapters/logos/README.md`). That transport is not
//! available headless / in CI / from a clean clone without the full Basecamp
//! stack.
//!
//! `FileDeliveryClient` is the dependency-free alternative: it reads the **exact
//! records the Delivery topic carries** — newline-delimited
//! [`MetadataEnvelopeV1`] JSON — from an operator-supplied file and feeds them
//! through the *same* `run_batch_loop` → parse → dedupe → batch → on-chain
//! `anchor_batch` pipeline. Nothing about the on-chain path is mocked: the batch
//! tool does its real dedupe + batching + idempotent anchoring against the
//! deployed registry. Only the *transport* of the CID list is a file handoff
//! instead of a live Waku subscription.
//!
//! This is a legitimate operating mode in its own right (an NGO or guardian who
//! already holds a list of broadcast envelopes — e.g. exported from the explorer
//! or shared out-of-band — can bulk-anchor them permissionlessly), and it is the
//! mode the reproducible demo + clean-clone evaluation use so the run is fully
//! headless with no mock delivery client.

use crate::traits::{AdapterError, DeliveryClient, PublishReceipt, ReceivedEnvelope};
use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use std::path::Path;

/// A read-only [`DeliveryClient`] that yields a fixed set of envelopes once.
///
/// `subscribe` returns a **finite** stream: it emits every loaded envelope and
/// then ends, which drives `run_batch_loop` to flush any buffered entries and
/// exit cleanly — exactly the shape of a one-shot "anchor this list" run.
pub struct FileDeliveryClient {
    /// Each element is the raw JSON bytes of one `MetadataEnvelopeV1` record,
    /// passed through verbatim as `ReceivedEnvelope::payload` (the batch loop
    /// parses + validates it, mirroring the live-subscription path).
    envelopes: Vec<Vec<u8>>,
}

impl FileDeliveryClient {
    /// Load newline-delimited `MetadataEnvelopeV1` JSON from `path`.
    ///
    /// Blank lines and lines beginning with `#` (comments) are ignored. The
    /// records are **not** parsed here — they are validated downstream by the
    /// batch loop exactly as live-subscribed envelopes are, so a malformed line
    /// surfaces through the same code path rather than a bespoke one.
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::from_jsonl(&text))
    }

    /// Build from an in-memory newline-delimited JSON string (test/embed helper).
    pub fn from_jsonl(text: &str) -> Self {
        let envelopes = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.as_bytes().to_vec())
            .collect();
        Self { envelopes }
    }

    /// Build directly from raw envelope-payload byte vectors.
    pub fn from_payloads(envelopes: Vec<Vec<u8>>) -> Self {
        Self { envelopes }
    }

    /// Number of envelope records loaded.
    pub fn len(&self) -> usize {
        self.envelopes.len()
    }

    /// True if no envelope records were loaded.
    pub fn is_empty(&self) -> bool {
        self.envelopes.is_empty()
    }
}

#[async_trait]
impl DeliveryClient for FileDeliveryClient {
    /// Publishing is not supported — this client is a read-only replay source.
    /// The batch tool only ever calls `subscribe`; surfacing a clear error keeps
    /// any accidental publish attempt honest rather than silently succeeding.
    async fn publish(&self, _topic: &str, _bytes: Vec<u8>) -> Result<PublishReceipt, AdapterError> {
        Err(AdapterError::non_retryable(
            "FileDeliveryClient is a read-only envelope replay source; publish is unsupported \
             (use the Basecamp UI plugin / a live Delivery client to broadcast)",
        ))
    }

    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<ReceivedEnvelope, AdapterError>>, AdapterError> {
        let topic = topic.to_string();
        // Own the data so the returned stream is 'static.
        let items: Vec<Result<ReceivedEnvelope, AdapterError>> = self
            .envelopes
            .iter()
            .enumerate()
            .map(|(i, payload)| {
                Ok(ReceivedEnvelope {
                    // Dedup keys on (topic, payload), not message_hash, so a
                    // synthetic index-based hash is fine and duplicate payloads
                    // still dedupe correctly.
                    message_hash: format!("file-{i}"),
                    topic: topic.clone(),
                    payload: payload.clone(),
                    timestamp_ns: (i as u64) + 1,
                })
            })
            .collect();
        Ok(Box::pin(stream::iter(items)))
    }
}
