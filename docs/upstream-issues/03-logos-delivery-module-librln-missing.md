# delivery_module Nix output is missing librln.dylib (broken @loader_path link on macOS arm64)

Target repo: `logos-co/logos-delivery-module`
Suggested labels: `bug`, `packaging`, `macos`
Severity: high (blocks every Basecamp app using Logos Delivery on macOS arm64)

---

## Summary

The `lgx`/`default` Nix outputs of `logos-delivery-module` install
`liblogosdelivery.dylib` without the `librln.dylib` (Zerokit/RLN) it links
against, and the recorded load command points at a Nix build-sandbox absolute
path. On macOS arm64 the module fails to `dlopen` on every Basecamp launch.

## Symptom

```
Library not loaded: /nix/var/nix/builds/nix-872-90086794/source/target/release/deps/librln.dylib
  Referenced from: <…> liblogosdelivery.dylib
  Reason: tried: '<many nix-store/qt paths>' (no such file), … 'librln.dylib' (no such file)
LogosAPIConsumer: Failed to acquire plugin/replica for object: "delivery_module"
```

The deployed module dir contains `delivery_module_plugin.dylib`,
`liblogosdelivery.dylib`, `libpq*.dylib`, `manifest.json`, `variant` — but
**no `librln.dylib`**.

## Root cause

`liblogosdelivery.dylib` links `librln.dylib` as a Rust `cdylib` dep, but
(a) the build records the linker's build-time absolute path
(`/nix/var/nix/builds/…/deps/librln.dylib`) as the load command rather than
`@loader_path/librln.dylib`, and (b) the Nix `installPhase` does not copy
`librln.dylib` next to `liblogosdelivery.dylib` in the output.

## Workaround (what we shipped)

A post-install script that locates `librln.dylib` in the local `/nix/store`
(zerokit output), copies it next to `liblogosdelivery.dylib` in each Basecamp
profile's `delivery_module/` dir, sets the copy's install-name to
`@loader_path/librln.dylib`, and rewrites `liblogosdelivery.dylib`'s librln
load command to `@loader_path/librln.dylib`. After this the publish flow runs
end-to-end (storage upload → delivery broadcast).

## Suggested fix

Add a `postFixup` (or equivalent) phase to the flake that:
1. copies `${zerokit}/lib/librln.dylib` into the module's install dir,
2. `install_name_tool -id @loader_path/librln.dylib` on the copy,
3. `install_name_tool -change <build-path> @loader_path/librln.dylib liblogosdelivery.dylib`.

The cleaner Cargo-side fix is `cargo:rustc-link-arg=-Wl,-rpath,@loader_path` in
the FFI crate's build script, so the load command emerges as
`@rpath/librln.dylib` and resolves from the install dir without rewriting.
