//! Test the batch-anchor engine end-to-end against mocks: feed envelopes
//! through the mock delivery, watch them get accumulated and anchored in
//! batches according to the configured size + interval thresholds.

use document_indexing::{run_batch_loop, BatchConfig, BatchSubmission, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use whistleblower_core::{CanonicalCid, MetadataEnvelopeV1, DEFAULT_CONTENT_TOPIC};
use whistleblower_mock_adapter::{MockDeliveryClient, MockRegistryClient};

fn make_envelope(suffix: &str) -> MetadataEnvelopeV1 {
    MetadataEnvelopeV1 {
        version: 1,
        cid: CanonicalCid::new(format!("bafy-batch-test-{suffix}")).unwrap(),
        title: format!("doc-{suffix}"),
        description: "batch loop test".into(),
        content_type: "text/plain".into(),
        size_bytes: 10,
        timestamp: 1_725_000_000,
        tags: vec![],
    }
}

#[tokio::test]
async fn batches_are_flushed_at_size_threshold() {
    let delivery = Arc::new(MockDeliveryClient::default());
    let registry = Arc::new(MockRegistryClient::default());
    let tmp = TempDir::new().unwrap();

    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel::<BatchSubmission>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let config = BatchConfig {
        topic: DEFAULT_CONTENT_TOPIC.to_string(),
        batch_size: 3,
        // Long interval so size-threshold is what triggers the flush.
        batch_interval: Duration::from_secs(60),
        retry_policy: RetryPolicy::no_retry(),
        dedupe_store_path: tmp.path().join("dedupe.log"),
    };

    let delivery_for_loop = delivery.clone();
    let registry_for_loop = registry.clone();
    let loop_task = tokio::spawn(async move {
        run_batch_loop(
            delivery_for_loop,
            registry_for_loop,
            config,
            sub_tx,
            shutdown_rx,
        )
        .await
    });

    // Give the loop a tick to call subscribe + register the channel.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Emit 5 envelopes; size-threshold of 3 should trigger one batch (of 3),
    // then 2 more pending. Then we shut down — the leftover 2 flush on shutdown.
    for i in 0..5 {
        let env = make_envelope(&format!("{i}"));
        let bytes = serde_json::to_vec(&env).unwrap();
        delivery.emit_received(DEFAULT_CONTENT_TOPIC, bytes);
    }

    // Wait for first batch to land.
    let first = tokio::time::timeout(Duration::from_secs(2), sub_rx.recv())
        .await
        .expect("first batch within 2s")
        .expect("submission channel not closed");
    assert_eq!(first.batch_size, 3, "first flush should be exactly 3");
    assert_eq!(first.anchored, 3);

    // Trigger shutdown to flush the remaining 2.
    shutdown_tx.send(true).unwrap();
    let second = tokio::time::timeout(Duration::from_secs(2), sub_rx.recv())
        .await
        .expect("shutdown flush within 2s")
        .expect("submission channel not closed");
    assert_eq!(
        second.batch_size, 2,
        "shutdown flush should drain remaining 2"
    );

    let outcome = tokio::time::timeout(Duration::from_secs(2), loop_task)
        .await
        .expect("batch loop joins")
        .expect("join ok");
    outcome.expect("batch loop returns Ok");
}

#[tokio::test]
async fn batches_flush_on_interval_when_below_size_threshold() {
    let delivery = Arc::new(MockDeliveryClient::default());
    let registry = Arc::new(MockRegistryClient::default());
    let tmp = TempDir::new().unwrap();

    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel::<BatchSubmission>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let config = BatchConfig {
        topic: DEFAULT_CONTENT_TOPIC.to_string(),
        batch_size: 100, // huge — never reached in this test
        batch_interval: Duration::from_millis(150),
        retry_policy: RetryPolicy::no_retry(),
        dedupe_store_path: tmp.path().join("dedupe.log"),
    };

    let delivery_for_loop = delivery.clone();
    let registry_for_loop = registry.clone();
    let loop_task = tokio::spawn(async move {
        run_batch_loop(
            delivery_for_loop,
            registry_for_loop,
            config,
            sub_tx,
            shutdown_rx,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    for i in 0..2 {
        let env = make_envelope(&format!("interval-{i}"));
        delivery.emit_received(DEFAULT_CONTENT_TOPIC, serde_json::to_vec(&env).unwrap());
    }

    let flush = tokio::time::timeout(Duration::from_secs(2), sub_rx.recv())
        .await
        .expect("interval flush within 2s")
        .expect("channel open");
    assert_eq!(flush.batch_size, 2, "interval flush should drain pending 2");

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_task).await;
}

#[tokio::test]
async fn duplicate_envelopes_are_skipped_via_dedupe_store() {
    let delivery = Arc::new(MockDeliveryClient::default());
    let registry = Arc::new(MockRegistryClient::default());
    let tmp = TempDir::new().unwrap();

    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel::<BatchSubmission>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let config = BatchConfig {
        topic: DEFAULT_CONTENT_TOPIC.to_string(),
        batch_size: 3,
        batch_interval: Duration::from_secs(60),
        retry_policy: RetryPolicy::no_retry(),
        dedupe_store_path: tmp.path().join("dedupe.log"),
    };

    let delivery_for_loop = delivery.clone();
    let registry_for_loop = registry.clone();
    let loop_task = tokio::spawn(async move {
        run_batch_loop(
            delivery_for_loop,
            registry_for_loop,
            config,
            sub_tx,
            shutdown_rx,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Emit the SAME envelope 3 times — dedup should keep only 1 in flight.
    let env = make_envelope("dup");
    let bytes = serde_json::to_vec(&env).unwrap();
    for _ in 0..3 {
        delivery.emit_received(DEFAULT_CONTENT_TOPIC, bytes.clone());
    }
    // Then emit 2 distinct ones.
    delivery.emit_received(
        DEFAULT_CONTENT_TOPIC,
        serde_json::to_vec(&make_envelope("a")).unwrap(),
    );
    delivery.emit_received(
        DEFAULT_CONTENT_TOPIC,
        serde_json::to_vec(&make_envelope("b")).unwrap(),
    );

    // Should land 3 (1 dedup'd + 2 distinct = 3 total) in one batch.
    let flush = tokio::time::timeout(Duration::from_secs(2), sub_rx.recv())
        .await
        .expect("dedup flush within 2s")
        .expect("channel open");
    assert_eq!(
        flush.batch_size, 3,
        "duplicate envelopes should be deduplicated to 3 unique entries"
    );

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_task).await;
}
