use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_CONTENT_TOPIC: &str = "/lp0017-whistleblower/1/cids/json";
pub const CID_HASH_DOMAIN: &str = "lp0017:cid:v1\0";

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct CanonicalCid(String);

impl CanonicalCid {
    pub fn new(cid: impl Into<String>) -> Result<Self, CoreError> {
        let cid = cid.into();
        let canonical = cid.trim().to_string();
        if canonical.is_empty() {
            return Err(CoreError::EmptyCid);
        }
        Ok(Self(canonical))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CidHash(pub [u8; 32]);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MetadataHash(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MetadataEnvelopeV1 {
    pub version: u8,
    pub cid: CanonicalCid,
    pub title: String,
    pub description: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub timestamp: u64,
    pub tags: Vec<String>,
}

impl MetadataEnvelopeV1 {
    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, CoreError> {
        serde_json::to_vec(self).map_err(CoreError::Json)
    }

    pub fn metadata_hash(&self) -> Result<MetadataHash, CoreError> {
        let digest = Sha256::digest(self.canonical_json_bytes()?);
        Ok(MetadataHash(digest.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AnchorEntry {
    pub cid: CanonicalCid,
    pub cid_hash: CidHash,
    pub metadata_hash: MetadataHash,
    pub anchor_timestamp: u64,
}

pub fn cid_hash(cid: &CanonicalCid) -> CidHash {
    let mut hasher = Sha256::new();
    hasher.update(CID_HASH_DOMAIN.as_bytes());
    hasher.update(cid.as_str().as_bytes());
    CidHash(hasher.finalize().into())
}

pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("CID cannot be empty")]
    EmptyCid,
    #[error("canonical JSON serialization failed: {0}")]
    Json(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cid_hash_uses_domain_separator() {
        let cid = CanonicalCid::new("bafy-test").unwrap();
        let actual = cid_hash(&cid);

        let mut hasher = Sha256::new();
        hasher.update(CID_HASH_DOMAIN.as_bytes());
        hasher.update(b"bafy-test");
        assert_eq!(actual, CidHash(hasher.finalize().into()));
    }

    #[test]
    fn anchor_entry_borsh_round_trip() {
        let cid = CanonicalCid::new("bafy-roundtrip").unwrap();
        let entry = AnchorEntry {
            cid: cid.clone(),
            cid_hash: cid_hash(&cid),
            metadata_hash: MetadataHash([7; 32]),
            anchor_timestamp: 42,
        };
        let bytes = borsh::to_vec(&entry).unwrap();
        let decoded = AnchorEntry::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, entry);
    }
}
