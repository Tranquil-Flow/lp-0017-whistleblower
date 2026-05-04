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

**Status:** TBD. The devnet RPC URL has not been wired into our `scaffold.toml` yet. To deploy:

1. Edit `scaffold.toml` to point `[localnet]` at the devnet endpoint, OR
2. Run `wallet deploy-program <bin>` directly with `NSSA_SEQUENCER_URL=https://devnet.example` (URL TBD — confirm with Logos team via #builder-hub).
3. Capture the resulting program_id here:
   ```
   devnet program_id: <TBD>
   devnet deploy block:  <TBD>
   devnet deploy tx hash: <TBD>
   ```
4. Update `whistleblower-registry-idl.json` metadata if the deployed program ID differs from the local build (it shouldn't — image-hash determinism — but worth verifying).

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
