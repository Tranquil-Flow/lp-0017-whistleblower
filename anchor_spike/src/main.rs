//! Task 1.0B — Registry idempotency spike against a real LEZ sequencer.
//!
//! Drives the deployed `whistleblower-registry` program through the actual
//! sequencer, exercising the four behaviours called out in `REGISTRY_SPIKE.md`:
//!
//! 1. First `anchor_one(cid_a)` creates the entry.
//! 2. Second `anchor_one(cid_a)` is a no-op success.
//! 3. `anchor_batch` with mixed existing/new CIDs is a partial-success no-error.
//! 4. `anchor_batch` with 10 fresh CIDs lands in one transaction.
//!
//! Run order:
//!     lgs localnet start
//!     lgs deploy --program-path target/.../whistleblower_registry.bin
//!     export NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet
//!     cargo run --release -p anchor-spike

use anyhow::{anyhow, bail, Context, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use common::transaction::NSSATransaction;
use nssa::{
    public_transaction::{Message, WitnessSet},
    AccountId, PublicTransaction,
};
use nssa::program::Program;
use nssa_core::program::{PdaSeed, ProgramId};
use sequencer_service_rpc::RpcClient;
use wallet::WalletCore;
use whistleblower_core::{
    AnchorEntry, CanonicalCid, MetadataHash, RegistryInstruction, REGISTRY_PDA_SEED_BYTES,
};
use whistleblower_methods::WHISTLEBLOWER_REGISTRY_ELF;

/// Same on-chain layout as the guest. Defining it here avoids sharing a
/// guest-side module with the host runner.
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
struct RegistryStateOnChain {
    entries: Vec<AnchorEntry>,
}

/// Fixed PDA seed for the single registry-root account. Must match the
/// guest's `REGISTRY_PDA_SEED_BYTES` so both compute the same PDA.
const REGISTRY_SEED: PdaSeed = PdaSeed::new(REGISTRY_PDA_SEED_BYTES);

/// Per-run unique suffix so the CIDs we anchor in this run don't collide with
/// CIDs already on the PDA from previous runs. The PDA itself is shared
/// across runs (single registry root); we just ensure each run's "new" CIDs
/// are actually new.
fn run_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

fn program_id_hex(id: &ProgramId) -> String {
    let mut out = String::with_capacity(64);
    for word in id {
        for byte in word.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

fn make_cid(suffix: &str) -> CanonicalCid {
    CanonicalCid::new(format!("bafy-spike-{suffix}")).expect("non-empty test cid")
}

fn make_metadata_hash(seed: u8) -> MetadataHash {
    MetadataHash([seed; 32])
}

async fn submit_and_wait(
    wallet_core: &WalletCore,
    program: &Program,
    pda: AccountId,
    instruction: &RegistryInstruction,
    label: &str,
) -> Result<()> {
    use tokio::time::{sleep, Duration};

    let payload = borsh::to_vec(instruction).context("encode RegistryInstruction")?;
    let message = Message::try_new(program.id(), vec![pda], vec![], payload)
        .map_err(|e| anyhow!("build {label} message: {e:?}"))?;
    let witness_set = WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let hash = wallet_core
        .sequencer_client
        .send_transaction(NSSATransaction::Public(tx))
        .await
        .with_context(|| format!("submit {label} transaction"))?;
    println!("  ✓ {label} submitted (tx hash: {hash:?})");

    // Poll get_transaction until it lands. Localnet block interval is ~15s.
    for attempt in 0..40 {
        sleep(Duration::from_millis(750)).await;
        let res = wallet_core.sequencer_client.get_transaction(hash).await;
        if let Ok(Some(_)) = res {
            return Ok(());
        }
        if attempt % 8 == 7 {
            eprintln!(
                "  ... waiting for {label} to land ({}s elapsed)",
                ((attempt + 1) * 750) / 1000
            );
        }
    }
    bail!("{label}: tx not confirmed within 30s")
}

async fn read_state(
    wallet_core: &WalletCore,
    pda: AccountId,
) -> Result<RegistryStateOnChain> {
    let account = wallet_core
        .get_account_public(pda)
        .await
        .context("fetch registry-root account")?;
    let bytes: Vec<u8> = account.data.clone().into();
    if bytes.is_empty() {
        return Ok(RegistryStateOnChain::default());
    }
    BorshDeserialize::try_from_slice(&bytes).context("decode RegistryStateOnChain")
}

#[tokio::main]
async fn main() -> Result<()> {
    let wallet_core = WalletCore::from_env()
        .context("WalletCore::from_env failed — set NSSA_WALLET_HOME_DIR")?;

    let program = Program::new(WHISTLEBLOWER_REGISTRY_ELF.to_vec())
        .context("parse WHISTLEBLOWER_REGISTRY_ELF")?;
    let registry_pda: AccountId = (&program.id(), &REGISTRY_SEED).into();

    println!("program_id   = {}", program_id_hex(&program.id()));
    println!("registry_pda = {registry_pda}");
    println!();

    let suffix = run_suffix();
    let cid_a = make_cid(&format!("{suffix}-alpha"));
    let cid_b = make_cid(&format!("{suffix}-bravo"));
    let mh_a = make_metadata_hash(0x01);
    let mh_b = make_metadata_hash(0x02);

    let baseline = read_state(&wallet_core, registry_pda.clone()).await?.entries.len();
    println!("baseline entries on registry PDA: {baseline}");
    println!();

    println!("[1/4] anchor_one(cid_a) — expect baseline + 1");
    submit_and_wait(
        &wallet_core,
        &program,
        registry_pda.clone(),
        &RegistryInstruction::AnchorOne {
            cid: cid_a.clone(),
            metadata_hash: mh_a,
            anchor_timestamp: 1_000_000,
        },
        "anchor_one(cid_a)",
    )
    .await?;
    let s = read_state(&wallet_core, registry_pda.clone()).await?;
    if s.entries.len() != baseline + 1 {
        bail!("test 1: expected baseline+1 = {}, got {}", baseline + 1, s.entries.len());
    }
    println!("  state has {} entries (baseline+1)\n", s.entries.len());

    println!("[2/4] anchor_one(cid_a) again — expect entries unchanged at baseline+1");
    submit_and_wait(
        &wallet_core,
        &program,
        registry_pda.clone(),
        &RegistryInstruction::AnchorOne {
            cid: cid_a.clone(),
            metadata_hash: mh_a,
            anchor_timestamp: 2_000_000,
        },
        "anchor_one(cid_a) duplicate",
    )
    .await?;
    let s = read_state(&wallet_core, registry_pda.clone()).await?;
    if s.entries.len() != baseline + 1 {
        bail!("test 2: expected baseline+1 = {}, got {}", baseline + 1, s.entries.len());
    }
    let cid_a_entry = s.entries.iter().find(|e| e.cid == cid_a)
        .ok_or_else(|| anyhow!("test 2: cid_a entry missing from state"))?;
    if cid_a_entry.anchor_timestamp != 1_000_000 {
        bail!(
            "test 2: original anchor_timestamp should be preserved (got {})",
            cid_a_entry.anchor_timestamp
        );
    }
    println!("  cid_a entry preserved with original timestamp 1_000_000\n");

    println!("[3/4] anchor_batch([cid_a, cid_b]) — expect 1 new, 1 skipped, total=baseline+2");
    submit_and_wait(
        &wallet_core,
        &program,
        registry_pda.clone(),
        &RegistryInstruction::AnchorBatch {
            entries: vec![(cid_a.clone(), mh_a), (cid_b.clone(), mh_b)],
            anchor_timestamp: 3_000_000,
        },
        "anchor_batch([cid_a, cid_b])",
    )
    .await?;
    let s = read_state(&wallet_core, registry_pda.clone()).await?;
    if s.entries.len() != baseline + 2 {
        bail!("test 3: expected baseline+2 = {}, got {}", baseline + 2, s.entries.len());
    }
    println!("  state has {} entries (baseline+2)\n", s.entries.len());

    println!("[4/4] anchor_batch(10 fresh CIDs) — expect entries to jump by 10");
    let fresh: Vec<(CanonicalCid, MetadataHash)> = (10..20)
        .map(|i| (make_cid(&format!("{suffix}-{i}")), make_metadata_hash(i as u8)))
        .collect();
    submit_and_wait(
        &wallet_core,
        &program,
        registry_pda.clone(),
        &RegistryInstruction::AnchorBatch {
            entries: fresh,
            anchor_timestamp: 4_000_000,
        },
        "anchor_batch(10 new)",
    )
    .await?;
    let s = read_state(&wallet_core, registry_pda.clone()).await?;
    if s.entries.len() != baseline + 12 {
        bail!("test 4: expected baseline+12 = {}, got {}", baseline + 12, s.entries.len());
    }
    println!("  state has {} entries (baseline+12)\n", s.entries.len());

    println!("✅ Task 1.0B spike PASSED — duplicate-safe semantics work end-to-end");
    println!("   Registry PDA: {registry_pda}");
    println!("   Baseline entries (from prior runs): {baseline}");
    println!("   Final entry count: {}", s.entries.len());
    Ok(())
}
