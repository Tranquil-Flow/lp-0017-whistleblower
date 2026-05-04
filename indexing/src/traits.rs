use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use whistleblower_core::{AnchorEntry, CanonicalCid, CidHash, MetadataHash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSession {
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadReceipt {
    pub request_id: String,
    pub cid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    pub request_id: String,
    pub message_hash: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivedEnvelope {
    pub message_hash: String,
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterErrorKind {
    Retryable,
    NonRetryable,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{kind:?} adapter error: {message}")]
pub struct AdapterError {
    pub kind: AdapterErrorKind,
    pub message: String,
}

impl AdapterError {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: AdapterErrorKind::Retryable,
            message: message.into(),
        }
    }

    pub fn non_retryable(message: impl Into<String>) -> Self {
        Self {
            kind: AdapterErrorKind::NonRetryable,
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait StorageClient: Send + Sync {
    async fn upload_file(&self, path: PathBuf) -> Result<UploadReceipt, AdapterError>;
}

#[async_trait]
pub trait DeliveryClient: Send + Sync {
    async fn publish(&self, topic: &str, bytes: Vec<u8>) -> Result<PublishReceipt, AdapterError>;
    async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<BoxStream<'static, Result<ReceivedEnvelope, AdapterError>>, AdapterError>;
}

#[async_trait]
pub trait RegistryClient: Send + Sync {
    /// Anchor a single CID. Idempotent — re-anchoring an already-registered
    /// CID returns the existing AnchorEntry, never errors.
    async fn anchor_one(
        &self,
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
    ) -> Result<AnchorEntry, AdapterError>;

    /// Anchor multiple CIDs in one transaction. Idempotent per-entry.
    async fn anchor_batch(
        &self,
        entries: Vec<(CanonicalCid, MetadataHash)>,
    ) -> Result<Vec<AnchorEntry>, AdapterError>;

    /// Query an existing entry by its hash (Storage CID -> on-chain hash).
    async fn query_by_cid_hash(
        &self,
        cid_hash: CidHash,
    ) -> Result<Option<AnchorEntry>, AdapterError>;
}
