use crate::traits::ReceivedEnvelope;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    New { envelope_hash: String },
    Duplicate { envelope_hash: String },
}

#[derive(Debug)]
pub struct DurableDedupeStore {
    path: PathBuf,
    seen: BTreeSet<String>,
}

impl DurableDedupeStore {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut seen = BTreeSet::new();
        if path.exists() {
            for line in fs::read_to_string(&path)?.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    seen.insert(trimmed.to_string());
                }
            }
        }
        Ok(Self { path, seen })
    }

    pub fn ingest(&mut self, envelope: &ReceivedEnvelope) -> io::Result<IngestOutcome> {
        let hash = envelope_hash(envelope);
        if self.seen.contains(&hash) {
            return Ok(IngestOutcome::Duplicate {
                envelope_hash: hash,
            });
        }
        self.seen.insert(hash.clone());
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{hash}")?;
        file.sync_all()?;
        Ok(IngestOutcome::New {
            envelope_hash: hash,
        })
    }
}

pub fn envelope_hash(envelope: &ReceivedEnvelope) -> String {
    let mut hasher = Sha256::new();
    hasher.update(envelope.topic.as_bytes());
    hasher.update(b"\0");
    hasher.update(&envelope.payload);
    hex::encode(hasher.finalize())
}
