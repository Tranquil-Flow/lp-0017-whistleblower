# `whistleblower-logos-adapter` — bring-up plan

This crate is a **scaffold** for the real Logos Storage + Delivery adapters.
It is not yet wired into the workspace `members` list (so the rest of the
project compiles cleanly without it). The next session that takes this
forward should follow the steps below.

## Why it isn't wired in yet

The Logos Core C++ modules (`logos-storage-module`, `logos-delivery-module`)
need to be built via `nix` and produce `.dylib` files plus a `logoscore`
runtime binary. On the local Mac that build cascade hit a disk-full
incident mid-fetch (system was already at ~95% capacity). The plan is to
do the heavy nix builds on the `m4pro` Tailscale host (`100.84.252.4`) where
there's 158GB+ of headroom, then SCP the artifacts back here.

## Bring-up sequence (next session)

### On `m4pro` (one-time setup, ~10 min + ~1-2 hr build cascade)

```bash
ssh m4pro

# 1. Install Determinate Nix (needs sudo — interactive)
curl -L https://install.determinate.systems/nix | sh -s -- install

# 2. Source nix into the shell
source /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

# 3. Build the runtime binary (this is what the adapters subprocess into)
nix build --extra-experimental-features 'nix-command flakes' \
  github:logos-co/logos-liblogos --out-link ~/logos-runtime

# 4. Build the storage module (dylib + headers)
mkdir -p ~/logos-modules && cd ~/logos-modules
git clone --depth 1 https://github.com/logos-co/logos-storage-module
cd logos-storage-module && nix build --print-build-logs

# 5. Build the delivery module (dylib + headers)
cd ~/logos-modules
git clone --depth 1 https://github.com/logos-co/logos-delivery-module
cd logos-delivery-module && nix build --print-build-logs

# Confirm artifacts:
ls ~/logos-runtime/bin/  # expect logoscore
ls ~/logos-modules/logos-storage-module/result/lib/
ls ~/logos-modules/logos-delivery-module/result/lib/
```

### Back on the local Mac

```bash
# Pull the runtime binary + module dylibs back.
mkdir -p logos-runtime/bin logos-runtime/modules

scp m4pro:logos-runtime/bin/logoscore logos-runtime/bin/
scp 'm4pro:logos-modules/logos-storage-module/result/lib/*' logos-runtime/modules/
scp 'm4pro:logos-modules/logos-delivery-module/result/lib/*' logos-runtime/modules/

# Storage module config (use the upstream sample as a starting point):
curl -L https://raw.githubusercontent.com/logos-co/node-configs/refs/heads/master/storage_config.json \
  -o logos-runtime/storage_config.json

# Smoke test logoscore can load the modules:
./logos-runtime/bin/logoscore -m ./logos-runtime/modules \
  --load-modules storage_module \
  -c "storage_module.init(@logos-runtime/storage_config.json)" \
  -c "storage_module.start()"
```

If logoscore prints version + connect events, the runtime is good.

### Wire the adapter

1. Replace `src/lib.rs` stub with real `LogoscoreStorageAdapter` impl that
   spawns `logoscore` as a subprocess (one of two strategies):

   - **Per-call** (simpler): each `upload_file()` spawns logoscore with a
     fresh `-c "storage_module.importFiles(...)"`, parses stderr for
     `storageUploadDone` event, returns the CID. ~5s overhead per call.
   - **Long-lived** (production-shaped): one logoscore subprocess per
     adapter instance, command stream over stdin, event stream from
     stderr — multiplexed. Lower per-call latency but more plumbing.

2. Add the crate to the workspace `members`:

   ```toml
   members = [
       # ...existing...
       "adapters/logos",
   ]
   ```

3. Wire `LogoscoreStorageAdapter` into `whistleblower-batch` behind a
   feature flag or runtime config (replacing the current `MockDeliveryClient`
   placeholder).

4. Drop the `--mock-delivery` flag requirement in `batch/src/main.rs`.

5. Add live integration tests in `adapters/logos/tests/live_logos.rs`
   gated `#[ignore]` (require running logoscore + connected Logos network).

6. Re-record the demo video showing real upload → broadcast → batch anchor
   → registry query.

## Open questions for the implementation

- **Event format on stderr**: the storage module README shows
  `Debug: [LOGOS_HOST "storage_module"]: "storageUploadDone" cid="..."`
  but the exact format may have evolved. Lock it in by capturing stderr
  from a real run and writing a `nom`-or-`regex` parser.
- **Error event taxonomy**: which events map to `AdapterErrorKind::Retryable`
  vs `NonRetryable`? Probably: network/peer errors → Retryable;
  bad-config / file-not-found → NonRetryable.
- **Backpressure**: a long-lived logoscore process can be flooded by the
  batch CLI under high subscription rate. Bound the command queue.
- **Crash recovery**: if logoscore dies mid-upload, surface a
  retryable error and (on retry) check whether the file already uploaded
  via `storage_module.exists(cid)`.
- **Devnet vs localnet sequencer URL**: the storage_config.json points at
  peers, not at the LEZ sequencer — keep these orthogonal. The registry
  side uses NSSA_SEQUENCER_URL via the `wallet`/`spel` toolchain.

## Cleanup if you abandon the path

The crate is self-contained — `rm -rf adapters/logos/` is safe. It's not
in the workspace, so no other crate depends on it.
