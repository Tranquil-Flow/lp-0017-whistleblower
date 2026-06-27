# LP-0017 audio script — current `verify-testnet.sh` terminal evidence

Use this with:

```bash
cd ~/Projects/logos-basecamp/lp-0017-whistleblower/whistleblower
bash scripts/verify-testnet.sh
```

This script matches `scripts/verify-testnet.sh`: recording-safe, read-only public-testnet evidence verification. It re-queries confirmed transaction hashes and decodes the on-chain PDAs. It does not match `scripts/record-final-video.sh`.

## Opening — title screen

This is LP-0017, the Whistleblower document publishing and indexing submission for Logos.

In this terminal evidence recording, I am using the read-only public-testnet verifier. The script points a throwaway wallet home at the Logos public testnet, re-queries the confirmed deployment and anchor transaction hashes, then reads the public registry account PDAs directly from chain and decodes them as Borsh `AnchorEntry` records.

No faucet, signing, local state, localnet, or new transaction is required for this path. It is a re-verification of already confirmed public-testnet evidence.

The program being checked is `1c8a08b62f1cf7b4a92693502bb5522372d937cfe9aa5a60a98a3dac6b5908f7` on `https://testnet.lez.logos.co/`.

## Section 1 — Sequencer reachable; current block id

First, the verifier configures a temporary wallet home to use the public Logos testnet sequencer.

Then it asks the sequencer for the current block id.

This confirms that the recording is querying the live public endpoint, not a local fixture or cached log.

## Section 2 — Per-transaction verdicts queried live from the public sequencer

Now the script re-queries each expected transaction hash from the deployed lifecycle.

The first transaction is the program deployment. It should resolve as `ProgramDeployment`.

The next transactions are public anchor operations: `anchor_one`, `anchor_one_dup`, and `anchor_batch`. They should each resolve as `Public` transactions.

The duplicate anchor transaction is intentional evidence for duplicate handling and idempotent registry behavior. The batch anchor transaction demonstrates the permissionless batch path.

The success condition is that every hash is found live on the public sequencer with the expected transaction kind.

## Section 3 — Entry PDA readback and Borsh AnchorEntry decode

Next, the verifier reads the two public registry account PDAs directly from chain.

For each PDA, it runs `wallet account get --account-id Public/<pda> --raw`, extracts the account data hex, and decodes it as a Borsh `AnchorEntry`.

The decoded fields include the CID string, CID hash prefix, metadata hash, and anchor timestamp.

The first PDA should decode to the alpha CID: `bafy-lp0017-testnet-18b597589606e650-alpha`, with metadata byte `0x11` repeated across the metadata hash.

The second PDA should decode to the bravo CID: `bafy-lp0017-testnet-18b597589606e650-bravo`, with metadata byte `0x22` repeated across the metadata hash.

The success condition is that both entries are present and correct. This proves the registry state is not just transaction-level evidence; the expected account data can also be read and decoded from the public testnet.

## Closing

LP-0017 terminal evidence is now re-verified live: the public sequencer is reachable, the deployed program and anchor transaction hashes resolve with the expected kinds, and both registry PDAs decode to the expected whistleblower document entries.

The full proof context is recorded in `TESTNET_PROOF.md`. This recording is read-only, reproducible, and safe to rerun for evaluator verification.
