//! `whistleblower-batch` — permissionless batch anchor CLI.
//!
//! Subscribes to a Logos Delivery topic, dedupes, and submits accumulated
//! `(cid, metadata_hash)` pairs to the LEZ registry program in batches.
//!
//! Spec line 33-37: "any party can run [the batch tool] to gather broadcasted
//! CIDs and commit them on-chain in bulk. The tool must:
//!   - subscribe to the Logos Delivery topic and accumulate (CID, metadata_hash) tuples,
//!   - submit them to the on-chain registry in a single batch transaction,
//!   - be permissionless — no coordination with the original publisher required,
//!   - be idempotent — re-submitting an already-registered CID does not fail."
//!
//! Delivery source (CID input) is pluggable via the `DeliveryClient` trait:
//!   - `--envelopes-from <file>` — real, headless replay of broadcast
//!     `MetadataEnvelopeV1` records (the demo / CI / clean-clone path); no mock.
//!   - live Logos Delivery (Waku) subscription — the production transport,
//!     provided by the Basecamp UI plugin / a QtRemoteObjects `logos_host`
//!     adapter (see `adapters/logos/README.md`); not available headless.
//!   - `--mock-delivery` — in-memory dev client only.
//! The Registry side always uses the real `LezRegistryClient` against the
//! deployed LEZ program (public testnet by default).

use anyhow::{Context, Result};
use clap::Parser;
use document_indexing::{
    run_batch_loop, BatchConfig, BatchSubmission, DeliveryClient, FileDeliveryClient, RetryPolicy,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use wallet::WalletCore;
use whistleblower_core::DEFAULT_CONTENT_TOPIC;
use whistleblower_lez_adapter::LezRegistryClient;
use whistleblower_mock_adapter::MockDeliveryClient;

#[derive(Parser, Debug)]
#[command(
    name = "whistleblower-batch",
    about = "LP-0017 permissionless batch anchor: subscribe to a Logos Delivery topic and bulk-anchor CIDs to the LEZ registry."
)]
struct Cli {
    /// Logos Delivery topic to subscribe to.
    #[arg(long, default_value = DEFAULT_CONTENT_TOPIC, env = "WL_TOPIC")]
    topic: String,

    /// Submit a batch tx when this many CIDs are queued.
    #[arg(long, default_value_t = 10, env = "WL_BATCH_SIZE")]
    batch_size: usize,

    /// Submit a batch tx every this-many seconds, even if below batch_size.
    #[arg(long, default_value_t = 30, env = "WL_BATCH_INTERVAL_SECS")]
    batch_interval_secs: u64,

    /// Where to persist the dedup ledger so duplicate broadcasts don't get
    /// re-anchored across restarts.
    #[arg(long, default_value = "queue.db", env = "WL_DEDUPE_PATH")]
    dedupe_store_path: PathBuf,

    /// Real, headless delivery source: a file of newline-delimited
    /// `MetadataEnvelopeV1` JSON records (the exact envelopes broadcast over the
    /// Logos Delivery topic). The tool replays them through the real dedupe +
    /// batch + on-chain anchor pipeline — no mock, no Qt/Waku dependency. This
    /// is the mode the reproducible demo + CI use. Blank lines and `#` comments
    /// are ignored.
    #[arg(long, value_name = "FILE", env = "WL_ENVELOPES_FROM")]
    envelopes_from: Option<PathBuf>,

    /// Path to the deployed registry program `.bin` (the ELF that was actually
    /// deployed on chain). PDAs are derived from the program id, and a docker
    /// `cargo risczero build` and the in-process `embed_methods` build can
    /// produce different ImageIDs — so to anchor against the public-testnet
    /// program (`1c8a08b6…`) you MUST point here at the deployed `.bin`
    /// (`target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin`).
    /// If omitted, the embedded ELF is used (correct only when the embedded
    /// build is what was deployed, e.g. some localnet flows).
    #[arg(long, value_name = "FILE", env = "WL_PROGRAM_BIN")]
    program_bin: Option<PathBuf>,

    /// Dev-only: use an in-memory mock Delivery client (no real messages are
    /// subscribed to). Prefer --envelopes-from for a real headless run. The
    /// live Waku Delivery subscription is provided by the Basecamp UI plugin /
    /// a QtRemoteObjects `logos_host` adapter — see `adapters/logos/README.md`.
    #[arg(long, default_value_t = false, env = "WL_MOCK_DELIVERY")]
    mock_delivery: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let wallet_core = Arc::new(WalletCore::from_env().context(
        "WalletCore::from_env failed — set NSSA_WALLET_HOME_DIR to a seeded wallet dir",
    )?);
    let registry = Arc::new(match cli.program_bin.as_ref() {
        Some(path) => {
            let elf = std::fs::read(path)
                .with_context(|| format!("read program bin {}", path.display()))?;
            LezRegistryClient::with_program_bytes(wallet_core, elf)
                .context("LezRegistryClient::with_program_bytes")?
        }
        None => LezRegistryClient::new(wallet_core).context("LezRegistryClient::new")?,
    });

    let (delivery, delivery_desc): (Arc<dyn DeliveryClient>, String) =
        if let Some(path) = cli.envelopes_from.as_ref() {
            let client = FileDeliveryClient::from_file(path)
                .with_context(|| format!("read envelopes file {}", path.display()))?;
            let desc = format!(
                "FILE replay ({} envelope(s) from {})",
                client.len(),
                path.display()
            );
            (Arc::new(client), desc)
        } else if cli.mock_delivery {
            (
                Arc::new(MockDeliveryClient::default()),
                "MOCK (in-memory, dev-only — no real messages)".to_string(),
            )
        } else {
            anyhow::bail!(
                "No delivery source selected. Choose one:\n  \
                 --envelopes-from <file>  replay broadcast MetadataEnvelopeV1 JSONL through the \
                 real dedupe+batch+anchor pipeline (headless, no mock)\n  \
                 --mock-delivery          in-memory dev client (no real messages)\n\n\
                 A live Logos Delivery (Waku) subscription is provided by the Basecamp UI plugin \
                 or a QtRemoteObjects logos_host adapter — see adapters/logos/README.md."
            );
        };

    let config = BatchConfig {
        topic: cli.topic.clone(),
        batch_size: cli.batch_size,
        batch_interval: Duration::from_secs(cli.batch_interval_secs),
        retry_policy: RetryPolicy::default(),
        dedupe_store_path: cli.dedupe_store_path,
    };

    let (sub_tx, mut sub_rx) = mpsc::unbounded_channel::<BatchSubmission>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    eprintln!(
        "whistleblower-batch starting:\n  topic = {}\n  batch_size = {}\n  batch_interval = {}s\n  delivery = {}",
        cli.topic, cli.batch_size, cli.batch_interval_secs, delivery_desc
    );

    // Sigint handler -> shutdown signal.
    let shutdown_for_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                eprintln!(
                    "\n[whistleblower-batch] SIGINT received — flushing pending and stopping."
                );
                let _ = shutdown_for_signal.send(true);
            }
            Err(e) => eprintln!("[whistleblower-batch] signal handler error: {e}"),
        }
    });

    // Submission logger.
    let logger = tokio::spawn(async move {
        while let Some(s) = sub_rx.recv().await {
            eprintln!(
                "[whistleblower-batch] anchored batch: {}/{} entries",
                s.anchored, s.batch_size
            );
        }
    });

    let result = run_batch_loop(delivery, registry, config, sub_tx, shutdown_rx).await;
    let _ = logger.await;
    result.context("batch loop")?;
    eprintln!("[whistleblower-batch] clean exit.");
    Ok(())
}
