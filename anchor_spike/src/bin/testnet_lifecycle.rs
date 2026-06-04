//! LP-0017 public-testnet deploy + anchor-lifecycle driver.
//!
//! Deploys the rc3 `whistleblower_registry` guest to the public LEZ testnet
//! and exercises the registry lifecycle, capturing every transaction hash so
//! the evidence is independently re-verifiable with `wallet chain-info`.
//!
//! Why a bespoke driver instead of `wallet deploy-program` + `anchor_spike`:
//!   - `wallet deploy-program` is fire-and-forget — it discards the deploy tx
//!     hash, so there's nothing to put in the proof log. Here we build a typed
//!     `ProgramDeploymentTransaction`, submit it, capture the hash, and poll.
//!   - the `LezRegistryClient` adapter swallows anchor tx hashes (its
//!     `submit_and_wait` returns `()`); this driver uses the SAME encoding the
//!     adapter uses (`borsh::to_vec(&RegistryInstruction)` → `Message::try_new`
//!     with an empty witness set) but keeps the hashes.
//!
//! The program id is taken from the SAME `.bin` we deploy (not the embedded
//! `WHISTLEBLOWER_REGISTRY_ELF`) because a docker `cargo risczero build` and an
//! in-process `embed_methods` build can produce different ImageIDs.
//!
//! Run:
//!   export NSSA_WALLET_HOME_DIR=~/Projects/logos-basecamp/.nssa-testnet-wallet
//!   export LP0017_PROGRAM_BIN=.../docker/whistleblower_registry.bin
//!   export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
//!   cargo run --release -p anchor-spike --bin testnet_lifecycle

use anyhow::{Context, Result};
use borsh::BorshDeserialize;
use common::transaction::NSSATransaction;
use nssa::program::Program;
use nssa::program_deployment_transaction::Message as DeployMessage;
use nssa::public_transaction::{Message, WitnessSet};
use nssa::{AccountId, ProgramDeploymentTransaction, PublicTransaction};
use nssa_core::program::{PdaSeed, ProgramId};
use sequencer_service_rpc::RpcClient;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wallet::WalletCore;
use whistleblower_core::{
    cid_hash as compute_cid_hash, AnchorEntry, CanonicalCid, MetadataHash, RegistryInstruction,
};

fn program_id_hex(id: &ProgramId) -> String {
    let mut out = String::with_capacity(64);
    for word in id {
        for byte in word.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn run_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

/// Submit a registry instruction against the supplied entry PDAs (unsigned
/// public tx — the registry program takes no signer), capture the hash, and
/// poll until inclusion. Returns (hash_hex, confirmed).
async fn submit_and_capture(
    wallet_core: &WalletCore,
    program: &Program,
    accounts: Vec<AccountId>,
    instruction: &RegistryInstruction,
    label: &str,
    attempts: usize,
) -> Result<(String, bool)> {
    let payload = borsh::to_vec(instruction).context("borsh-encode instruction")?;
    let message = Message::try_new(program.id(), accounts, vec![], payload)
        .map_err(|e| anyhow::anyhow!("{label}: build message: {e:?}"))?;
    let witness_set = WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let hash = wallet_core
        .sequencer_client
        .send_transaction(NSSATransaction::Public(tx))
        .await
        .map_err(|e| anyhow::anyhow!("{label}: submit: {e:?}"))?;
    let hash_hex = format!("{hash:?}");

    let poll = Duration::from_millis(750);
    for _ in 0..attempts {
        tokio::time::sleep(poll).await;
        if let Ok(Some(_)) = wallet_core.sequencer_client.get_transaction(hash).await {
            return Ok((hash_hex, true));
        }
    }
    Ok((hash_hex, false))
}

/// Read an entry PDA's account data and decode the `AnchorEntry` (None if the
/// PDA is unclaimed / empty).
async fn read_entry(wallet_core: &WalletCore, pda: AccountId) -> Result<Option<AnchorEntry>> {
    let account = wallet_core
        .get_account_public(pda)
        .await
        .map_err(|e| anyhow::anyhow!("fetch entry PDA: {e:?}"))?;
    let bytes: Vec<u8> = account.data.clone().into();
    if bytes.is_empty() {
        return Ok(None);
    }
    AnchorEntry::try_from_slice(&bytes)
        .map(Some)
        .map_err(|e| anyhow::anyhow!("decode AnchorEntry: {e}"))
}

fn entry_pda(program: &Program, cid: &CanonicalCid) -> AccountId {
    let cid_hash = compute_cid_hash(cid);
    AccountId::for_public_pda(&program.id(), &PdaSeed::new(cid_hash.0))
}

#[tokio::main]
async fn main() -> Result<()> {
    let wallet_core = Arc::new(
        WalletCore::from_env()
            .context("WalletCore::from_env failed — set NSSA_WALLET_HOME_DIR (testnet wallet)")?,
    );

    let elf_path = std::env::var("LP0017_PROGRAM_BIN")
        .context("set LP0017_PROGRAM_BIN to the rc3 whistleblower_registry.bin")?;
    let elf = fs::read(&elf_path).with_context(|| format!("read program ELF {elf_path}"))?;
    let program =
        Program::new(elf.clone()).map_err(|e| anyhow::anyhow!("parse program ELF: {e:?}"))?;
    let program_id = program.id();

    let suffix = run_suffix();
    let cid_a = CanonicalCid::new(format!("bafy-lp0017-testnet-{suffix}-alpha")).unwrap();
    let cid_b = CanonicalCid::new(format!("bafy-lp0017-testnet-{suffix}-bravo")).unwrap();
    let mh_a = MetadataHash([0x11; 32]);
    let mh_b = MetadataHash([0x22; 32]);

    println!("== LP-0017 whistleblower-registry public-testnet lifecycle ==");
    println!("program_id (hex) = {}", program_id_hex(&program_id));
    println!("entry_pda(cid_a) = {}", entry_pda(&program, &cid_a));
    println!("entry_pda(cid_b) = {}", entry_pda(&program, &cid_b));
    println!();

    // [0] Deploy the program (typed tx so we can capture + confirm the hash).
    let deploy_tx = ProgramDeploymentTransaction::new(DeployMessage::new(elf.clone()));
    let deploy_hash = wallet_core
        .sequencer_client
        .send_transaction(NSSATransaction::ProgramDeployment(deploy_tx))
        .await
        .map_err(|e| anyhow::anyhow!("deploy submit: {e:?}"))?;
    let deploy_hash_hex = format!("{deploy_hash:?}");
    let mut deploy_confirmed = false;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(750)).await;
        if let Ok(Some(_)) = wallet_core
            .sequencer_client
            .get_transaction(deploy_hash)
            .await
        {
            deploy_confirmed = true;
            break;
        }
    }
    println!(
        "[0] deploy_program          {} tx={deploy_hash_hex}",
        if deploy_confirmed {
            "confirmed"
        } else {
            "submitted (not yet confirmed)"
        }
    );

    // [1] anchor_one(cid_a) — fresh PDA.
    let (h1, c1) = submit_and_capture(
        &wallet_core,
        &program,
        vec![entry_pda(&program, &cid_a)],
        &RegistryInstruction::AnchorOne {
            cid: cid_a.clone(),
            metadata_hash: mh_a,
            anchor_timestamp: now_ms(),
        },
        "anchor_one(cid_a)",
        60,
    )
    .await?;
    println!(
        "[1] anchor_one(cid_a)       {} tx={h1}",
        if c1 { "confirmed" } else { "NOT confirmed" }
    );

    // [2] anchor_one(cid_a) again — duplicate, expect idempotent no-op success.
    let (h2, c2) = submit_and_capture(
        &wallet_core,
        &program,
        vec![entry_pda(&program, &cid_a)],
        &RegistryInstruction::AnchorOne {
            cid: cid_a.clone(),
            metadata_hash: mh_a,
            anchor_timestamp: now_ms(),
        },
        "anchor_one(cid_a) dup",
        60,
    )
    .await?;
    println!(
        "[2] anchor_one(cid_a) dup   {} tx={h2}",
        if c2 {
            "confirmed (idempotent)"
        } else {
            "NOT confirmed"
        }
    );

    // [3] anchor_batch([cid_a, cid_b]) — mixed existing + new in one tx.
    let (h3, c3) = submit_and_capture(
        &wallet_core,
        &program,
        vec![entry_pda(&program, &cid_a), entry_pda(&program, &cid_b)],
        &RegistryInstruction::AnchorBatch {
            entries: vec![(cid_a.clone(), mh_a), (cid_b.clone(), mh_b)],
            anchor_timestamp: now_ms(),
        },
        "anchor_batch([cid_a,cid_b])",
        60,
    )
    .await?;
    println!(
        "[3] anchor_batch(a,b)       {} tx={h3}",
        if c3 { "confirmed" } else { "NOT confirmed" }
    );

    // [4] Final state readback — pure chain reads, no tx.
    let entry_a = read_entry(&wallet_core, entry_pda(&program, &cid_a)).await?;
    let entry_b = read_entry(&wallet_core, entry_pda(&program, &cid_b)).await?;
    println!("[4] entry(cid_a) = {entry_a:?}");
    println!("    entry(cid_b) = {entry_b:?}");

    println!();
    println!("== hashes (verify each with `wallet chain-info transaction --hash <h>`) ==");
    println!("deploy_program  {deploy_hash_hex}");
    println!("anchor_one      {h1}");
    println!("anchor_one_dup  {h2}");
    println!("anchor_batch    {h3}");

    Ok(())
}
