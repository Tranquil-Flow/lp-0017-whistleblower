//! Deterministic compute-unit (CU) measurement for the whistleblower-registry
//! LEZ program.
//!
//! LEZ does not persist a per-transaction CU value (the sequencer consumes the
//! RISC0 `SessionInfo.journal` and discards the cycle count; there is no
//! `getTransactionReceipt` RPC — see `BUGS_FILED.md` #7). But the RISC0 zkVM is
//! deterministic: the cycles a public transaction consumes depend only on
//! `(program ELF, input)`. Public anchors run on the sequencer's *executor*
//! (no host-side proof — see `BENCHMARKS.md`), so the executor cycle count of
//! the **deployed** ELF is exactly the CU the testnet charges.
//!
//! This binary reconstructs the same `ExecutorEnv` the sequencer builds for a
//! public program (`nssa::Program::execute` → `write_inputs`: program_id,
//! caller, pre_states, instruction_data) and runs the deployed rc3 ELF through
//! `default_executor().execute()` with the same 32 M-cycle public-execution
//! session limit. No proving, no network, no wallet — fully reproducible.
//!
//! Usage:
//!   cargo run --release -p anchor-spike --bin measure_cu_cycles -- \
//!     [path/to/whistleblower_registry.bin]
//! (defaults to the docker-reproducible build output path).

use anyhow::{Context, Result};
use nssa::program::Program;
use nssa_core::account::{Account, AccountId, AccountWithMetadata};
use nssa_core::program::{PdaSeed, ProgramId};
use risc0_zkvm::serde::to_vec as risc0_to_vec;
use risc0_zkvm::{default_executor, ExecutorEnv};
use whistleblower_core::{cid_hash, CanonicalCid, MetadataHash, RegistryInstruction};

/// Same cap the sequencer applies to a public execution
/// (`nssa::program::MAX_NUM_CYCLES_PUBLIC_EXECUTION`).
const PUBLIC_EXECUTION_CYCLE_LIMIT: u64 = 1024 * 1024 * 32; // 32 M

/// Build the fresh (empty, default-owned) pre-state account at the PDA the guest
/// expects for `cid` — i.e. `for_public_pda(program_id, cid_hash(cid))`. Empty
/// data ⇒ the guest treats it as a fresh anchor and claims it.
fn fresh_pda_pre_state(program_id: &ProgramId, cid: &CanonicalCid) -> AccountWithMetadata {
    let ch = cid_hash(cid);
    let pda = AccountId::for_public_pda(program_id, &PdaSeed::new(ch.0));
    AccountWithMetadata::new(Account::default(), false, pda)
}

/// Run one instruction through the executor exactly as the sequencer would and
/// return the total user-cycle count (sum over segments).
fn measure(
    elf: &[u8],
    program_id: ProgramId,
    instruction: &RegistryInstruction,
    pre_states: &[AccountWithMetadata],
) -> Result<u64> {
    // The guest reads the instruction as `Vec<u8>` (borsh-encoded
    // RegistryInstruction), serialized into the RISC0 input stream as `Vec<u32>`
    // — identical to `nssa::Program::serialize_instruction`.
    let instruction_bytes: Vec<u8> =
        borsh::to_vec(instruction).context("borsh-encode RegistryInstruction")?;
    let instruction_data: Vec<u32> =
        risc0_to_vec(&instruction_bytes).context("risc0-serialize instruction bytes")?;

    let mut builder = ExecutorEnv::builder();
    builder.session_limit(Some(PUBLIC_EXECUTION_CYCLE_LIMIT));
    // Write order must match `nssa::Program::write_inputs` / the guest's
    // `read_nssa_inputs`: program_id, Option<caller>, pre_states, instruction.
    builder
        .write(&program_id)
        .context("write program_id")?
        .write(&None::<ProgramId>)
        .context("write caller_program_id")?
        .write(&pre_states.to_vec())
        .context("write pre_states")?
        .write(&instruction_data)
        .context("write instruction_data")?;
    let env = builder.build().context("build ExecutorEnv")?;

    let session = default_executor()
        .execute(env, elf)
        .context("executor.execute (guest panicked or hit cycle limit)")?;

    let cycles: u64 = session.segments.iter().map(|s| u64::from(s.cycles)).sum();
    Ok(cycles)
}

fn main() -> Result<()> {
    let elf_path = std::env::args().nth(1).unwrap_or_else(|| {
        "target/riscv32im-risc0-zkvm-elf/docker/whistleblower_registry.bin".to_string()
    });
    let elf = std::fs::read(&elf_path).with_context(|| {
        format!("read guest ELF at {elf_path} (run `cargo risczero build` first)")
    })?;

    let program = Program::new(elf.clone())
        .map_err(|e| anyhow::anyhow!("decode program / compute image id: {e:?}"))?;
    let program_id = program.id();
    let image_id_hex: String = program_id
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect();

    println!("# whistleblower-registry — deterministic CU (RISC0 user cycles)");
    println!("ELF       : {elf_path} ({} bytes)", elf.len());
    println!("ImageID   : {image_id_hex}");
    println!("CycleLimit: {PUBLIC_EXECUTION_CYCLE_LIMIT} (32 M, sequencer public-exec cap)");
    println!();
    println!("| operation | accounts | user cycles | per-CID |");
    println!("|---|---:|---:|---:|");

    // N = 1 (AnchorOne)
    let cid = CanonicalCid::new("bafy-lp0017-cu-bench-single")
        .map_err(|e| anyhow::anyhow!("cid: {e:?}"))?;
    let pre = vec![fresh_pda_pre_state(&program_id, &cid)];
    let anchor_one = RegistryInstruction::AnchorOne {
        cid,
        metadata_hash: MetadataHash([0x11; 32]),
        anchor_timestamp: 1_780_000_000_000,
    };
    let c1 = measure(&elf, program_id, &anchor_one, &pre)?;
    println!("| `anchor_one` (1 CID) | 1 | {c1} | {c1} |");

    // N = 10, 50 (AnchorBatch with N fresh CIDs)
    for n in [10usize, 50] {
        let mut entries = Vec::with_capacity(n);
        let mut pre_states = Vec::with_capacity(n);
        for i in 0..n {
            let cid = CanonicalCid::new(format!("bafy-lp0017-cu-bench-batch-{i:04}"))
                .map_err(|e| anyhow::anyhow!("cid: {e:?}"))?;
            pre_states.push(fresh_pda_pre_state(&program_id, &cid));
            entries.push((cid, MetadataHash([0x22; 32])));
        }
        let batch = RegistryInstruction::AnchorBatch {
            entries,
            anchor_timestamp: 1_780_000_000_000,
        };
        let c = measure(&elf, program_id, &batch, &pre_states)?;
        let per = c as f64 / n as f64;
        println!("| `anchor_batch` ({n} CIDs) | {n} | {c} | {per:.0} |");
    }

    Ok(())
}
