//! End-to-end Publisher orchestration test using all three mock adapters.

use document_indexing::{MetadataInputs, Publisher};
use std::path::PathBuf;
use std::sync::Arc;
use whistleblower_core::{cid_hash, CanonicalCid};
use whistleblower_mock_adapter::{MockDeliveryClient, MockRegistryClient, MockStorageClient};

fn inputs() -> MetadataInputs {
    MetadataInputs {
        title: "Test report".to_string(),
        description: "publisher e2e test".to_string(),
        content_type: "text/markdown".to_string(),
        size_bytes: 42,
        timestamp_unix: 1_725_000_000,
        tags: vec!["whistleblower".to_string(), "test".to_string()],
    }
}

#[tokio::test]
async fn publisher_uploads_broadcasts_and_anchors() {
    let storage = Arc::new(MockStorageClient::default());
    let delivery = Arc::new(MockDeliveryClient::default());
    let registry = Arc::new(MockRegistryClient::default());

    let publisher = Publisher::new(storage.clone(), delivery.clone(), registry.clone());
    let path = PathBuf::from("ignored-by-mock-storage.pdf");

    // Drive the publish in a background task so we can fire the mock events
    // that resolve the upload + publish futures.
    let publish_task = tokio::spawn({
        let publisher_inputs = inputs();
        async move { publisher.publish_and_anchor(path, publisher_inputs).await }
    });

    // Resolve the upload event with a known CID.
    let cid_str = "bafy-publisher-e2e-1";
    let upload_id = poll_until_ready(|| storage.pending_request_ids().pop()).await;
    storage.complete_upload(&upload_id, cid_str);

    // Resolve the delivery publish event.
    let publish_id = poll_until_ready(|| delivery.pending_request_ids().pop()).await;
    delivery.message_sent(&publish_id, "broadcast-hash-1");

    let (outcome, entry) = publish_task.await.unwrap().expect("publish_and_anchor");

    assert_eq!(outcome.cid, CanonicalCid::new(cid_str).unwrap());
    assert_eq!(outcome.envelope.title, "Test report");
    assert_eq!(outcome.envelope.size_bytes, 42);
    assert_eq!(outcome.publish_receipt.message_hash, "broadcast-hash-1");

    assert_eq!(entry.cid, outcome.cid);
    assert_eq!(entry.cid_hash, cid_hash(&outcome.cid));
    assert_eq!(entry.metadata_hash, outcome.metadata_hash);
}

#[tokio::test]
async fn publish_then_anchor_in_two_steps() {
    let storage = Arc::new(MockStorageClient::default());
    let delivery = Arc::new(MockDeliveryClient::default());
    let registry = Arc::new(MockRegistryClient::default());

    let publisher = Publisher::new(storage.clone(), delivery.clone(), registry.clone());

    // Step 1 — publish only.
    let publish_task = tokio::spawn({
        let publisher_inputs = inputs();
        let publisher = Publisher::new(storage.clone(), delivery.clone(), registry.clone());
        async move {
            publisher
                .publish_file(PathBuf::from("step1.pdf"), publisher_inputs)
                .await
        }
    });
    let upload_id = poll_until_ready(|| storage.pending_request_ids().pop()).await;
    storage.complete_upload(&upload_id, "bafy-step1");
    let publish_id = poll_until_ready(|| delivery.pending_request_ids().pop()).await;
    delivery.message_sent(&publish_id, "publish-hash");

    let outcome = publish_task.await.unwrap().expect("publish_file");
    assert_eq!(outcome.cid, CanonicalCid::new("bafy-step1").unwrap());

    // Step 2 — anchor the published outcome (caller-controlled — could be
    // the original publisher or any altruistic third party, per spec line 36).
    let entry = publisher
        .anchor_published(&outcome)
        .await
        .expect("anchor_published");
    assert_eq!(entry.cid, outcome.cid);
    assert_eq!(entry.metadata_hash, outcome.metadata_hash);

    // Re-anchoring is idempotent (RegistryClient contract).
    let entry_again = publisher
        .anchor_published(&outcome)
        .await
        .expect("re-anchor");
    assert_eq!(entry_again, entry);
}

/// Wait up to 1s for `f` to return Some — gives the publish task time
/// to register its pending request with the mock adapter.
async fn poll_until_ready<T>(mut f: impl FnMut() -> Option<T>) -> T {
    use tokio::time::{sleep, Duration};
    for _ in 0..100 {
        if let Some(v) = f() {
            return v;
        }
        sleep(Duration::from_millis(10)).await;
    }
    panic!("poll_until_ready: nothing pending after 1s")
}
