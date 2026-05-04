//! `Publisher` — end-to-end orchestrator that ties the three adapters
//! together to satisfy LP-0017's "user picks a file → upload → broadcast →
//! optional anchor" flow.
//!
//! Sequence:
//! 1. `Publisher::publish_file(path, MetadataInputs)`:
//!    - StorageClient.upload_file(path) -> CID
//!    - Build canonical MetadataEnvelopeV1 from CID + inputs
//!    - DeliveryClient.publish(content_topic, envelope_bytes) -> PublishReceipt
//!    - Returns PublishOutcome (CID, metadata_hash, publish_receipt)
//! 2. `Publisher::anchor_published(outcome)` (optional, called by either
//!    the publisher or any altruistic third party): RegistryClient.anchor_one
//!    against the LEZ program.
//!
//! Retryable adapter errors at each step are retried with exponential backoff
//! (`RetryPolicy`); non-retryable errors propagate immediately. Subscriber-side
//! dedup is the `DurableDedupeStore` orchestration helper, used by the batch
//! anchor CLI rather than the Publisher itself — re-publishing is permissionless
//! and the subscribers are responsible for filtering duplicates per spec line 53.

use crate::retry::{with_retry, RetryPolicy};
use crate::traits::{
    AdapterError, DeliveryClient, PublishReceipt, RegistryClient, StorageClient,
};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use whistleblower_core::{
    AnchorEntry, CanonicalCid, CoreError, MetadataEnvelopeV1, MetadataHash, DEFAULT_CONTENT_TOPIC,
};

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("storage upload failed after retries: {0}")]
    Storage(AdapterError),
    #[error("delivery publish failed after retries: {0}")]
    Delivery(AdapterError),
    #[error("registry anchor failed after retries: {0}")]
    Registry(AdapterError),
    #[error("envelope construction failed: {0}")]
    Envelope(#[from] CoreError),
    #[error("invalid canonical CID returned by storage: {0}")]
    InvalidCid(String),
}

/// What the caller supplies in addition to the file itself.
#[derive(Debug, Clone)]
pub struct MetadataInputs {
    pub title: String,
    pub description: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub timestamp_unix: u64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub cid: CanonicalCid,
    pub envelope: MetadataEnvelopeV1,
    pub envelope_bytes: Vec<u8>,
    pub metadata_hash: MetadataHash,
    pub publish_receipt: PublishReceipt,
}

pub struct Publisher {
    storage: Arc<dyn StorageClient>,
    delivery: Arc<dyn DeliveryClient>,
    registry: Arc<dyn RegistryClient>,
    /// Defaults to `DEFAULT_CONTENT_TOPIC`. Overridable for tests / private deployments.
    pub content_topic: String,
    retry_policy: RetryPolicy,
}

impl Publisher {
    pub fn new(
        storage: Arc<dyn StorageClient>,
        delivery: Arc<dyn DeliveryClient>,
        registry: Arc<dyn RegistryClient>,
    ) -> Self {
        Self {
            storage,
            delivery,
            registry,
            content_topic: DEFAULT_CONTENT_TOPIC.to_string(),
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.content_topic = topic.into();
        self
    }

    /// Override the retry policy. Defaults to 5 attempts, exponential backoff
    /// 200ms..10s. Use `RetryPolicy::no_retry()` for tests that must fail-fast.
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Step 1: upload + broadcast (no on-chain anchor).
    pub async fn publish_file(
        &self,
        path: PathBuf,
        inputs: MetadataInputs,
    ) -> Result<PublishOutcome, PublisherError> {
        let storage = self.storage.clone();
        let path_for_retry = path.clone();
        let upload = with_retry(self.retry_policy, move || {
            let storage = storage.clone();
            let path = path_for_retry.clone();
            async move { storage.upload_file(path).await }
        })
        .await
        .map_err(PublisherError::Storage)?;

        let cid = CanonicalCid::new(upload.cid.clone())
            .map_err(|_| PublisherError::InvalidCid(upload.cid))?;

        let envelope = MetadataEnvelopeV1 {
            version: 1,
            cid: cid.clone(),
            title: inputs.title,
            description: inputs.description,
            content_type: inputs.content_type,
            size_bytes: inputs.size_bytes,
            timestamp: inputs.timestamp_unix,
            tags: inputs.tags,
        };
        let envelope_bytes = envelope.canonical_json_bytes()?;
        let metadata_hash = envelope.metadata_hash()?;

        let delivery = self.delivery.clone();
        let topic = self.content_topic.clone();
        let bytes_for_retry = envelope_bytes.clone();
        let publish_receipt = with_retry(self.retry_policy, move || {
            let delivery = delivery.clone();
            let topic = topic.clone();
            let bytes = bytes_for_retry.clone();
            async move { delivery.publish(&topic, bytes).await }
        })
        .await
        .map_err(PublisherError::Delivery)?;

        Ok(PublishOutcome {
            cid,
            envelope,
            envelope_bytes,
            metadata_hash,
            publish_receipt,
        })
    }

    /// Step 2 (optional): anchor a previously-published outcome on-chain.
    /// Idempotent — re-anchoring an already-anchored CID returns the existing
    /// AnchorEntry (per RegistryClient contract).
    pub async fn anchor_published(
        &self,
        outcome: &PublishOutcome,
    ) -> Result<AnchorEntry, PublisherError> {
        let registry = self.registry.clone();
        let cid = outcome.cid.clone();
        let mh = outcome.metadata_hash;
        with_retry(self.retry_policy, move || {
            let registry = registry.clone();
            let cid = cid.clone();
            async move { registry.anchor_one(cid, mh).await }
        })
        .await
        .map_err(PublisherError::Registry)
    }

    /// Convenience — full pipeline (upload + broadcast + anchor) in one call.
    pub async fn publish_and_anchor(
        &self,
        path: PathBuf,
        inputs: MetadataInputs,
    ) -> Result<(PublishOutcome, AnchorEntry), PublisherError> {
        let outcome = self.publish_file(path, inputs).await?;
        let entry = self.anchor_published(&outcome).await?;
        Ok((outcome, entry))
    }
}
