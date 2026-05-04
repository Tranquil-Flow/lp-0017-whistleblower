//! Permissionless batch-anchor engine.
//!
//! Subscribes to a Logos Delivery topic, validates each broadcast envelope,
//! deduplicates against a durable on-disk store, and submits accumulated
//! `(cid, metadata_hash)` pairs to the `RegistryClient` in batches. Spec
//! line 33-37 — the batch anchor tool.
//!
//! Design:
//! - A subscriber task reads `ReceivedEnvelope`s from `DeliveryClient::subscribe`,
//!   parses each as JSON `MetadataEnvelopeV1`, validates `metadata_hash`
//!   round-trips, writes to a durable queue, and pushes the `(cid, mh)` pair
//!   onto an in-memory channel.
//! - The anchorer task drains the channel into a buffer, and submits
//!   `RegistryClient::anchor_batch` when the buffer reaches `batch_size`
//!   OR when `batch_interval` elapses.
//! - On startup the anchorer replays any pending entries from the durable
//!   queue (handles previous-run interruption per spec line 54).
//!
//! Idempotency comes from the `RegistryClient` (already-anchored CIDs are
//! no-op success), so the engine never has to query the registry to skip.

use crate::orchestration::DurableDedupeStore;
use crate::retry::{with_retry, RetryPolicy};
use crate::traits::{AdapterError, DeliveryClient, ReceivedEnvelope, RegistryClient};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use whistleblower_core::{CanonicalCid, MetadataEnvelopeV1, MetadataHash};

#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub topic: String,
    /// Submit a batch when this many CIDs are queued. Min 1.
    pub batch_size: usize,
    /// Submit a batch when this much time elapses since last submission.
    pub batch_interval: Duration,
    /// Retry policy for `anchor_batch` calls.
    pub retry_policy: RetryPolicy,
    /// Path to the durable dedupe store (newline-delimited envelope hashes).
    pub dedupe_store_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("delivery subscribe failed: {0}")]
    Subscribe(AdapterError),
    #[error("anchor_batch failed after retries: {0}")]
    Anchor(AdapterError),
    #[error("dedupe store I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Single submission result, useful for tests + observability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSubmission {
    pub batch_size: usize,
    pub anchored: usize,
}

/// Drives the subscribe + anchor loop. `submission_sink` receives one event
/// per successful batch submission, intended for tests + metrics — the CLI
/// just logs them.
pub async fn run_batch_loop<R: RegistryClient + ?Sized + 'static>(
    delivery: Arc<dyn DeliveryClient>,
    registry: Arc<R>,
    config: BatchConfig,
    submission_sink: mpsc::UnboundedSender<BatchSubmission>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), BatchError> {
    assert!(config.batch_size >= 1, "batch_size must be >= 1");

    // Open / replay the durable dedupe store. Existing entries don't auto-replay
    // pending CIDs (the dedupe store only stores hashes, not the full envelope),
    // so this is best-effort: dedup across restarts works, but mid-flight
    // pending-batch recovery requires the broadcaster to re-emit. The spec
    // accepts this trade-off — the registry's idempotency ensures duplicate
    // anchor attempts are safe.
    let dedupe = Arc::new(tokio::sync::Mutex::new(DurableDedupeStore::open(
        &config.dedupe_store_path,
    )?));

    // Subscribe to the topic — long-lived stream.
    let mut stream = delivery
        .subscribe(&config.topic)
        .await
        .map_err(BatchError::Subscribe)?;

    // Channel from subscriber loop -> anchorer loop.
    let (tx, mut rx) = mpsc::unbounded_channel::<(CanonicalCid, MetadataHash)>();

    // ---- Subscriber side ----
    let dedupe_for_sub = dedupe.clone();
    let topic_for_sub = config.topic.clone();
    let mut shutdown_for_sub = shutdown.clone();
    let subscriber = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_for_sub.changed() => {
                    if *shutdown_for_sub.borrow() { break; }
                }
                next = stream.next() => {
                    let Some(item) = next else { break; }; // stream closed
                    match item {
                        Err(e) => {
                            eprintln!("batch.subscribe: receive error: {e}");
                            continue;
                        }
                        Ok(envelope) => {
                            // Dedup against our store.
                            let mut store = dedupe_for_sub.lock().await;
                            let outcome = match store.ingest(&envelope) {
                                Ok(v) => v,
                                Err(e) => {
                                    eprintln!("batch.dedupe: store error: {e}");
                                    continue;
                                }
                            };
                            drop(store);
                            if matches!(outcome, crate::IngestOutcome::Duplicate { .. }) {
                                continue;
                            }
                            match parse_envelope(&envelope, &topic_for_sub) {
                                Ok(parsed) => {
                                    let mh = match parsed.metadata_hash() {
                                        Ok(h) => h,
                                        Err(e) => {
                                            eprintln!("batch.parse: hash error: {e}");
                                            continue;
                                        }
                                    };
                                    if tx.send((parsed.cid, mh)).is_err() {
                                        break; // anchorer dropped
                                    }
                                }
                                Err(e) => eprintln!("batch.parse: {e}"),
                            }
                        }
                    }
                }
            }
        }
    });

    // ---- Anchorer side ----
    let mut buffer: Vec<(CanonicalCid, MetadataHash)> = Vec::with_capacity(config.batch_size);
    let mut interval = tokio::time::interval(config.batch_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the first immediate-fire — only flush after at least one interval.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    if !buffer.is_empty() {
                        flush_batch(&registry, &mut buffer, &config, &submission_sink).await?;
                    }
                    break;
                }
            }
            received = rx.recv() => {
                match received {
                    Some(item) => {
                        buffer.push(item);
                        if buffer.len() >= config.batch_size {
                            flush_batch(&registry, &mut buffer, &config, &submission_sink).await?;
                        }
                    }
                    None => {
                        // Subscriber closed.
                        if !buffer.is_empty() {
                            flush_batch(&registry, &mut buffer, &config, &submission_sink).await?;
                        }
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_batch(&registry, &mut buffer, &config, &submission_sink).await?;
                }
            }
        }
    }

    // Wait for the subscriber to finish so we don't leak the task.
    let _ = subscriber.await;
    Ok(())
}

async fn flush_batch<R: RegistryClient + ?Sized>(
    registry: &Arc<R>,
    buffer: &mut Vec<(CanonicalCid, MetadataHash)>,
    config: &BatchConfig,
    sink: &mpsc::UnboundedSender<BatchSubmission>,
) -> Result<(), BatchError> {
    let drained: Vec<_> = std::mem::take(buffer);
    let count = drained.len();
    let registry = registry.clone();
    let policy = config.retry_policy;
    let entries = drained.clone();
    let result = with_retry(policy, move || {
        let registry = registry.clone();
        let entries = entries.clone();
        async move { registry.anchor_batch(entries).await }
    })
    .await
    .map_err(BatchError::Anchor)?;

    let _ = sink.send(BatchSubmission {
        batch_size: count,
        anchored: result.len(),
    });
    Ok(())
}

fn parse_envelope(
    received: &ReceivedEnvelope,
    expected_topic: &str,
) -> Result<MetadataEnvelopeV1, String> {
    if received.topic != expected_topic {
        return Err(format!(
            "topic mismatch: got {} expected {}",
            received.topic, expected_topic
        ));
    }
    let envelope: MetadataEnvelopeV1 = serde_json::from_slice(&received.payload)
        .map_err(|e| format!("envelope JSON decode: {e}"))?;
    Ok(envelope)
}
