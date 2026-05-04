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
//! Status: Logos Delivery integration is currently mocked behind `--mock-delivery`
//! pending Phase 1.7 (real Logos Core module bindings, requires nix). The Registry
//! side uses the real `LezRegistryClient` against a running sequencer.

use anyhow::{Context, Result};
use clap::Parser;
use document_indexing::{
    run_batch_loop, BatchConfig, BatchSubmission, DeliveryClient, RetryPolicy,
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

    /// Use a mock Delivery client instead of the real Logos Delivery module.
    /// Pending Phase 1.7 (nix-built Logos Core module bindings); without this
    /// flag the binary refuses to start.
    #[arg(long, default_value_t = false, env = "WL_MOCK_DELIVERY")]
    mock_delivery: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let wallet_core = Arc::new(WalletCore::from_env().context(
        "WalletCore::from_env failed — set NSSA_WALLET_HOME_DIR to a seeded wallet dir",
    )?);
    let registry = Arc::new(LezRegistryClient::new(wallet_core).context("LezRegistryClient::new")?);

    if !cli.mock_delivery {
        anyhow::bail!(
            "Real Logos Delivery integration is not yet wired (Phase 1.7 — needs nix). \
             Run with --mock-delivery to exercise the engine against an in-memory \
             delivery client (no actual messages will be subscribed to)."
        );
    }

    let delivery: Arc<dyn DeliveryClient> = Arc::new(MockDeliveryClient::default());

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
        "whistleblower-batch starting:\n  topic = {}\n  batch_size = {}\n  batch_interval = {}s\n  delivery = MOCK (Phase 1.7 placeholder)",
        cli.topic, cli.batch_size, cli.batch_interval_secs
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
