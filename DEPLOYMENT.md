# Deployment

## Build the program

```bash
lgs build
```

Produces `target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin` (~417KB).

The program's deterministic ID is computed from the binary's image hash:

```bash
spel inspect target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
```

This prints a stable `ProgramId` in three encodings (decimal `[u32; 8]`, hex string, ImageID hex bytes). The hex string is what `lgs deploy` reports as `program_id`.

## Local sequencer (development)

```bash
# Start a local LEZ sequencer (RPC on :3040, RISC0_DEV_MODE=true).
lgs localnet start

# Deploy the program.
lgs deploy --program-path \
  target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin

# Stop when done.
lgs localnet stop
```

Sequencer logs: `.scaffold/logs/sequencer.log`.
Wallet state: `.scaffold/wallet/`. Pre-seeded accounts:
- `Public/2RHZhw9h534Zr3eq2RGhQete2Hh667foECzXPmSkGni2`
- `Public/CbgR6tj5kWx5oziiFptM7jMvrQeYY3Mzaao6ciuhSr2r`
- `Private/9DGDXnrNo4QhUUb2F8WDuDrPESja3eYDkZG5HkzvAvMC`
- `Private/A6AT9UvsgitUi8w4BH43n6DyX1bK37DtSCfjEWXQQUrQ`

The local sequencer runs `RISC0_DEV_MODE=true` by default — proofs are stubbed. The submission video MUST switch to `RISC0_DEV_MODE=0` for real proof generation; see `DEMO.md`.

## LEZ devnet / testnet

**Status:** Awaiting the devnet RPC URL from the Logos team (#builder-hub).
The deployment commands are identical to localnet — only the
`NSSA_SEQUENCER_URL` env var needs to point at the devnet endpoint.

Once the URL is available, deploy with:

```bash
# Set the devnet endpoint (URL TBD — substitute the real one).
export NSSA_SEQUENCER_URL="https://devnet-sequencer.logos.example"

# Same deploy command as localnet (program_id is image-hash determined,
# so it'll be the same byte-for-byte as the localnet program_id).
lgs deploy --program-path \
  target/riscv-guest/whistleblower-methods/whistleblower-programs/riscv32im-risc0-zkvm-elf/release/whistleblower_registry.bin
```

Then capture the deployment metadata here:

```
devnet program_id:    <copy from `lgs deploy` output>
devnet deploy block:  <from sequencer; query via `wallet chain-info`>
devnet deploy tx hash: <from `lgs deploy` JSON output with --json>
```

And rerun the live integration suite + `anchor_spike` against devnet
to capture the production benchmark numbers (they'll differ from
localnet because `RISC0_DEV_MODE=0` runs real proving):

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  RISC0_DEV_MODE=0 \
  cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
```

Then update `BENCHMARKS.md` with the resulting wall-clock + CU numbers.

If the devnet program ID matches the localnet build (which it should
since program_id is the SHA of the guest binary), the existing
`whistleblower-registry-idl.json` is reusable as-is. If they differ
for any reason, regenerate the IDL or re-publish.

## Verification

After deployment, exercise the registry end-to-end:

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet ./target/release/anchor_spike
```

Expected output: all four spike behaviours pass plus the bonus query, finishing with:

```
✅ Task 1.0B spike PASSED on PDA-per-CID design
   Anchored 12 fresh CIDs total this run.
```

For broader verification, run the live integration suite:

```bash
NSSA_WALLET_HOME_DIR=$PWD/.scaffold/wallet \
  cargo test -p whistleblower-lez-adapter --release -- --ignored --nocapture
```

Expected: 2 tests pass (~30s).

## Rolling back

The PDA-per-CID design has no rollback story — the registry is append-only and entries are owned by the program forever once claimed. If a bug ships, redeploy with a new build (new program_id) and migrate; old entries remain readable but are owned by the old program. The CID itself is a hash of the file contents, so existing publications never need re-uploading.
