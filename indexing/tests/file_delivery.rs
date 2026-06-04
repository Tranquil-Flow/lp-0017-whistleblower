//! `FileDeliveryClient` drives the real `run_batch_loop` pipeline (parse →
//! dedupe → batch → anchor) against a mock registry, with no live transport and
//! no mock delivery client. This is the headless code path the demo + clean-clone
//! evaluation exercise: the only non-production element is the mock *registry*
//! (a real run targets the deployed LEZ program); delivery is a real file replay.

use document_indexing::{
    run_batch_loop, BatchConfig, BatchSubmission, FileDeliveryClient, RegistryClient, RetryPolicy,
};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use whistleblower_core::{cid_hash, CanonicalCid, MetadataEnvelopeV1, DEFAULT_CONTENT_TOPIC};
use whistleblower_mock_adapter::MockRegistryClient;

fn envelope_json(suffix: &str) -> String {
    let env = MetadataEnvelopeV1 {
        version: 1,
        cid: CanonicalCid::new(format!("bafy-file-delivery-{suffix}")).unwrap(),
        title: format!("doc-{suffix}"),
        description: "file delivery test".into(),
        content_type: "text/markdown".into(),
        size_bytes: 42,
        timestamp: 1_780_000_000_000,
        tags: vec!["leak".into(), "test".into()],
    };
    serde_json::to_string(&env).unwrap()
}

#[test]
fn from_file_filters_comments_and_blank_lines() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("envelopes.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "# this is a comment, ignored").unwrap();
    writeln!(f, "{}", envelope_json("a")).unwrap();
    writeln!(f).unwrap(); // blank line, ignored
    writeln!(f, "  {}  ", envelope_json("b")).unwrap(); // surrounding whitespace trimmed
    drop(f);

    let client = FileDeliveryClient::from_file(&path).unwrap();
    assert_eq!(client.len(), 2, "comment + blank line must be filtered out");
    assert!(!client.is_empty());
}

#[tokio::test]
async fn file_delivery_drives_real_batch_pipeline_with_dedupe() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("envelopes.jsonl");

    // Two byte-identical "A" records (must dedupe to one) + one distinct "B".
    let a = envelope_json("alpha");
    let b = envelope_json("bravo");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "# operator-supplied broadcast envelopes").unwrap();
    writeln!(f, "{a}").unwrap();
    writeln!(f, "{a}").unwrap(); // duplicate broadcast — deduped by the store
    writeln!(f, "{b}").unwrap();
    drop(f);

    let delivery = Arc::new(FileDeliveryClient::from_file(&path).unwrap());
    assert_eq!(delivery.len(), 3, "three raw records loaded (one is a dup)");

    let registry = Arc::new(MockRegistryClient::default());
    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel::<BatchSubmission>();
    // Shutdown channel is required by the signature, but the finite file stream
    // ends on its own, which flushes + exits — we never signal shutdown.
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);

    let config = BatchConfig {
        topic: DEFAULT_CONTENT_TOPIC.to_string(),
        batch_size: 10, // larger than the input, so the stream-end flush is what fires
        batch_interval: Duration::from_secs(60),
        retry_policy: RetryPolicy::no_retry(),
        dedupe_store_path: tmp.path().join("dedupe.log"),
    };

    let registry_for_loop = registry.clone();
    let loop_task = tokio::spawn(async move {
        run_batch_loop(delivery, registry_for_loop, config, sub_tx, shutdown_rx).await
    });

    // Drain all submissions until the loop completes and drops the sender.
    let mut submissions = Vec::new();
    while let Some(s) = sub_rx.recv().await {
        submissions.push(s);
    }

    let outcome = tokio::time::timeout(Duration::from_secs(5), loop_task)
        .await
        .expect("batch loop joins")
        .expect("join ok");
    outcome.expect("batch loop returns Ok on finite stream");

    let total_anchored: usize = submissions.iter().map(|s| s.anchored).sum();
    assert_eq!(
        total_anchored, 2,
        "duplicate envelope must dedupe to 2 unique on-chain anchors"
    );

    // Both unique CIDs are now in the registry; query is by cid_hash, no tx.
    for suffix in ["alpha", "bravo"] {
        let cid = CanonicalCid::new(format!("bafy-file-delivery-{suffix}")).unwrap();
        let got = registry.query_by_cid_hash(cid_hash(&cid)).await.unwrap();
        assert!(got.is_some(), "{suffix} should be anchored and queryable");
    }
}

#[tokio::test]
async fn publish_is_unsupported_on_read_only_source() {
    use document_indexing::DeliveryClient;
    let client = FileDeliveryClient::from_jsonl("");
    let err = client
        .publish(DEFAULT_CONTENT_TOPIC, b"x".to_vec())
        .await
        .expect_err("publish must be rejected on a read-only replay source");
    assert!(err.to_string().contains("read-only"));
}
