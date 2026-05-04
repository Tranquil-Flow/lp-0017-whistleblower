use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const DEFAULT_CONTENT_TOPIC: &str = "/lp0017-whistleblower/1/cids/json";
pub const CID_HASH_DOMAIN: &str = "lp0017:cid:v1\0";

/// PDA seed for the single registry-root account (32 bytes). Shared between
/// host and guest so both compute the same PDA from the same program ID.
pub const REGISTRY_PDA_SEED_BYTES: [u8; 32] = [0xAB; 32];

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
    PartialOrd,
    Ord,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorOneOutcome {
    pub entry: AnchorEntry,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorBatchOutcome {
    pub entries: Vec<AnchorEntry>,
    pub inserted: usize,
    pub skipped_existing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RegistryInstruction {
    AnchorOne {
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
        anchor_timestamp: u64,
    },
    AnchorBatch {
        entries: Vec<(CanonicalCid, MetadataHash)>,
        anchor_timestamp: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryOutcome {
    AnchorOne(AnchorOneOutcome),
    AnchorBatch(AnchorBatchOutcome),
}

impl RegistryOutcome {
    pub fn into_one(self) -> Option<AnchorOneOutcome> {
        match self {
            Self::AnchorOne(outcome) => Some(outcome),
            Self::AnchorBatch(_) => None,
        }
    }

    pub fn into_batch(self) -> Option<AnchorBatchOutcome> {
        match self {
            Self::AnchorOne(_) => None,
            Self::AnchorBatch(outcome) => Some(outcome),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct RegistryState {
    entries: BTreeMap<CidHash, AnchorEntry>,
}

impl RegistryState {
    pub fn apply(
        &mut self,
        instruction: RegistryInstruction,
    ) -> Result<RegistryOutcome, CoreError> {
        match instruction {
            RegistryInstruction::AnchorOne {
                cid,
                metadata_hash,
                anchor_timestamp,
            } => self
                .anchor_one(cid, metadata_hash, anchor_timestamp)
                .map(RegistryOutcome::AnchorOne),
            RegistryInstruction::AnchorBatch {
                entries,
                anchor_timestamp,
            } => self
                .anchor_batch(entries, anchor_timestamp)
                .map(RegistryOutcome::AnchorBatch),
        }
    }

    pub fn anchor_one(
        &mut self,
        cid: CanonicalCid,
        metadata_hash: MetadataHash,
        anchor_timestamp: u64,
    ) -> Result<AnchorOneOutcome, CoreError> {
        let cid_hash = cid_hash(&cid);
        if let Some(entry) = self.entries.get(&cid_hash) {
            return Ok(AnchorOneOutcome {
                entry: entry.clone(),
                inserted: false,
            });
        }

        let entry = AnchorEntry {
            cid,
            cid_hash,
            metadata_hash,
            anchor_timestamp,
        };
        self.entries.insert(cid_hash, entry.clone());
        Ok(AnchorOneOutcome {
            entry,
            inserted: true,
        })
    }

    pub fn anchor_batch(
        &mut self,
        entries: Vec<(CanonicalCid, MetadataHash)>,
        anchor_timestamp: u64,
    ) -> Result<AnchorBatchOutcome, CoreError> {
        let mut anchored = Vec::with_capacity(entries.len());
        let mut inserted = 0;
        let mut skipped_existing = 0;

        for (cid, metadata_hash) in entries {
            let outcome = self.anchor_one(cid, metadata_hash, anchor_timestamp)?;
            if outcome.inserted {
                inserted += 1;
            } else {
                skipped_existing += 1;
            }
            anchored.push(outcome.entry);
        }

        Ok(AnchorBatchOutcome {
            entries: anchored,
            inserted,
            skipped_existing,
        })
    }

    pub fn query_by_cid_hash(&self, cid_hash: CidHash) -> Result<Option<AnchorEntry>, CoreError> {
        Ok(self.entries.get(&cid_hash).cloned())
    }
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
