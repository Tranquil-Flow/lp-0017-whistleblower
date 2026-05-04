use async_trait::async_trait;
use document_indexing::{
    AdapterError, DeliveryClient, PublishReceipt, ReceivedEnvelope, RegistryClient, StorageClient,
    UploadReceipt,
};
use futures::channel::{mpsc, oneshot};
use futures::stream::BoxStream;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use whistleblower_core::{AnchorEntry, CanonicalCid, CidHash, MetadataHash};

#[derive(Clone, Default)]
pub struct MockStorageClient {
    inner: Arc<Mutex<StorageState>>,
}

#[derive(Default)]
struct StorageState {
    next_id: u64,
    pending: HashMap<String, oneshot::Sender<Result<UploadReceipt, AdapterError>>>,
}

impl MockStorageClient {
    pub fn pending_request_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().pending.keys().cloned().collect()
    }

    pub fn complete_upload(&self, request_id: &str, cid: impl Into<String>) {
        let sender = self
            .inner
            .lock()
            .unwrap()
            .pending
            .remove(request_id)
            .expect("unknown upload request id");
        let _ = sender.send(Ok(UploadReceipt {
            request_id: request_id.to_string(),
            cid: cid.into(),
        }));
    }
}

#[async_trait]
impl StorageClient for MockStorageClient {
    async fn upload_file(&self, _path: PathBuf) -> Result<UploadReceipt, AdapterError> {
        let (request_id, receiver) = {
            let mut state = self.inner.lock().unwrap();
            state.next_id += 1;
            let request_id = format!("upload-{}", state.next_id);
            let (sender, receiver) = oneshot::channel();
            state.pending.insert(request_id.clone(), sender);
            (request_id, receiver)
        };
        receiver
            .await
            .map_err(|_| AdapterError::retryable(format!("upload {request_id} cancelled")))?
    }
}

#[derive(Clone, Default)]
pub struct MockDeliveryClient {
    inner: Arc<Mutex<DeliveryState>>,
}

#[derive(Default)]
struct DeliveryState {
    next_id: u64,
    pending: HashMap<String, oneshot::Sender<Result<PublishReceipt, AdapterError>>>,
    subscribers:
        HashMap<String, Vec<mpsc::UnboundedSender<Result<ReceivedEnvelope, AdapterError>>>>,
}

impl MockDeliveryClient {
    pub fn pending_request_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().pending.keys().cloned().collect()
    }

    pub fn message_sent(&self, request_id: &str, message_hash: impl Into<String>) {
        let sender = self
            .inner
            .lock()
            .unwrap()
            .pending
            .remove(request_id)
            .expect("unknown publish request id");
        let _ = sender.send(Ok(PublishReceipt {
            request_id: request_id.to_string(),
            message_hash: message_hash.into(),
            timestamp: "2026-05-04T00:00:00Z".to_string(),
        }));
    }

    pub fn message_error(&self, request_id: &str, retryable: bool, message: impl Into<String>) {
        let sender = self
            .inner
            .lock()
            .unwrap()
            .pending
            .remove(request_id)
            .expect("unknown publish request id");
        let err = if retryable {
            AdapterError::retryable(message)
        } else {
            AdapterError::non_retryable(message)
        };
        let _ = sender.send(Err(err));
    }

    pub fn emit_received(&self, topic: &str, payload: Vec<u8>) {
        let envelope = ReceivedEnvelope {
            message_hash: format!("rx-{}", payload.len()),
            topic: topic.to_string(),
            payload,
            timestamp_ns: 1,
        };
        if let Some(subscribers) = self.inner.lock().unwrap().subscribers.get_mut(topic) {
            subscribers.retain(|sender| sender.unbounded_send(Ok(envelope.clone())).is_ok());
        }
    }
}

#[async_trait]
impl DeliveryClient for MockDeliveryClient {
    async fn publish(&self, _topic: &str, _bytes: Vec<u8>) -> Result<PublishReceipt, AdapterError> {
        let (request_id, receiver) = {
            let mut state = self.inner.lock().unwrap();
            state.next_id += 1;
            let request_id = format!("publish-{}", state.next_id);
            let (sender, receiver) = oneshot::channel();
            state.pending.insert(request_id.clone(), sender);
            (request_id, receiver)
        };
        receiver
            .await
            .map_err(|_| AdapterError::retryable(format!("publish {request_id} cancelled")))?
    }

    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<ReceivedEnvelope, AdapterError>>, AdapterError> {
        let (sender, receiver) = mpsc::unbounded();
        self.inner
            .lock()
            .unwrap()
            .subscribers
            .entry(topic.to_string())
            .or_default()
            .push(sender);
        Ok(receiver.boxed())
    }
}

#[derive(Clone, Default)]
pub struct MockRegistryClient {
    entries: Arc<Mutex<HashMap<CidHash, AnchorEntry>>>,
}

#[async_trait]
impl RegistryClient for MockRegistryClient {
    async fn anchor_one(
        &self,
        cid_hash: CidHash,
        metadata_hash: MetadataHash,
    ) -> Result<AnchorEntry, AdapterError> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(existing) = entries.get(&cid_hash) {
            return Ok(existing.clone());
        }
        let entry = AnchorEntry {
            cid: CanonicalCid::new(format!("mock-cid-{}", entries.len())).unwrap(),
            cid_hash,
            metadata_hash,
            anchor_timestamp: 1,
        };
        entries.insert(cid_hash, entry.clone());
        Ok(entry)
    }

    async fn anchor_batch(
        &self,
        entries_in: Vec<(CidHash, MetadataHash)>,
    ) -> Result<Vec<AnchorEntry>, AdapterError> {
        let mut out = Vec::with_capacity(entries_in.len());
        for (cid_hash, metadata_hash) in entries_in {
            out.push(self.anchor_one(cid_hash, metadata_hash).await?);
        }
        Ok(out)
    }

    async fn query_by_cid_hash(
        &self,
        cid_hash: CidHash,
    ) -> Result<Option<AnchorEntry>, AdapterError> {
        Ok(self.entries.lock().unwrap().get(&cid_hash).cloned())
    }
}
