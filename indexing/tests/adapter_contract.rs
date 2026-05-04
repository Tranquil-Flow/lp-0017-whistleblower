use document_indexing::{
    AdapterErrorKind, DeliveryClient, DurableDedupeStore, IngestOutcome, StorageClient,
};
use futures::{executor::block_on, FutureExt, StreamExt};
use std::path::PathBuf;
use whistleblower_mock_adapter::{MockDeliveryClient, MockStorageClient};

#[test]
fn upload_waits_for_cid_bearing_completion_event() {
    let storage = MockStorageClient::default();
    let mut upload = Box::pin(storage.upload_file(PathBuf::from("report.pdf")));

    assert!(
        upload.as_mut().now_or_never().is_none(),
        "upload must not complete synchronously before storageUploadDone"
    );
    let request_id = storage
        .pending_request_ids()
        .pop()
        .expect("upload request id");
    storage.complete_upload(&request_id, "bafy-real-cid");

    let receipt = block_on(upload).unwrap();
    assert_eq!(receipt.request_id, request_id);
    assert_eq!(receipt.cid, "bafy-real-cid");
}

#[test]
fn publish_waits_until_message_sent_event() {
    let delivery = MockDeliveryClient::default();
    let mut publish =
        Box::pin(delivery.publish("/lp0017-whistleblower/1/cids/json", b"{}".to_vec()));

    assert!(
        publish.as_mut().now_or_never().is_none(),
        "publish must wait for messageSent, not just send() request id"
    );
    let request_id = delivery
        .pending_request_ids()
        .pop()
        .expect("publish request id");
    delivery.message_sent(&request_id, "0xmessage");

    let receipt = block_on(publish).unwrap();
    assert_eq!(receipt.request_id, request_id);
    assert_eq!(receipt.message_hash, "0xmessage");
}

#[test]
fn message_error_surfaces_retryability() {
    let delivery = MockDeliveryClient::default();
    let mut publish =
        Box::pin(delivery.publish("/lp0017-whistleblower/1/cids/json", b"{}".to_vec()));

    assert!(publish.as_mut().now_or_never().is_none());
    let request_id = delivery
        .pending_request_ids()
        .pop()
        .expect("publish request id");
    delivery.message_error(&request_id, true, "temporary peer failure");

    let err = block_on(publish).unwrap_err();
    assert_eq!(err.kind, AdapterErrorKind::Retryable);
    assert_eq!(err.message, "temporary peer failure");
}

#[test]
fn subscription_decodes_received_payload_stream_boundary() {
    let delivery = MockDeliveryClient::default();
    let mut stream = block_on(delivery.subscribe("/lp0017-whistleblower/1/cids/json")).unwrap();

    delivery.emit_received(
        "/lp0017-whistleblower/1/cids/json",
        br#"{"cid":"bafy"}"#.to_vec(),
    );
    let envelope = block_on(stream.next()).unwrap().unwrap();
    assert_eq!(envelope.topic, "/lp0017-whistleblower/1/cids/json");
    assert_eq!(envelope.payload, br#"{"cid":"bafy"}"#);
}

#[test]
fn durable_dedupe_skips_duplicate_envelope_across_restarts() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("dedupe.log");
    let envelope = document_indexing::ReceivedEnvelope {
        message_hash: "message-hash-from-delivery".to_string(),
        topic: "/lp0017-whistleblower/1/cids/json".to_string(),
        payload: br#"{"cid":"bafy"}"#.to_vec(),
        timestamp_ns: 1,
    };

    let mut first = DurableDedupeStore::open(&path).unwrap();
    assert!(matches!(
        first.ingest(&envelope).unwrap(),
        IngestOutcome::New { .. }
    ));
    drop(first);

    let mut restarted = DurableDedupeStore::open(&path).unwrap();
    assert!(matches!(
        restarted.ingest(&envelope).unwrap(),
        IngestOutcome::Duplicate { .. }
    ));
}
