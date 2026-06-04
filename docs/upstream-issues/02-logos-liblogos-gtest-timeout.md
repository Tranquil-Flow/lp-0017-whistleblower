# `nix build logos-liblogos` fails: gtest_discover_tests 5s timeout

Target repo: `logos-co/logos-liblogos`
Suggested labels: `bug`, `build`, `nix`, `macos`
Severity: moderate (blocks the storage-module README's `logoscore` runtime install path)

---

## Summary

`nix build github:logos-co/logos-liblogos` (and `.#logos-liblogos`,
`.#logos-liblogos-bin`) links the test binary successfully, then fails in
CMake's `gtest_discover_tests`, which runs the binary with a 5-second timeout to
enumerate tests.

## Symptom

```
[27/27] Linking CXX executable bin/logos_core_tests
CMake Error at .../GoogleTestAddTests.cmake:132 (message):
  Error running test executable.
  Path: '.../build/bin/logos_core_tests'
  Result: Process terminated due to timeout
```

The binary either SIGSEGVs at startup or is too slow in the Nix sandbox. Same
shape as the closed-and-fixed `logos-basecamp#77`.

## Workaround

Build the `portable` output, which skips test discovery and still produces the
same `bin/logos_host` + `bin/logos_host_qt`:

```bash
nix build github:logos-co/logos-liblogos#portable
```

## Suggested fix

Bump the `gtest_discover_tests` `TIMEOUT` to 30+s in the CMake config, or
condition test discovery on a build-time env var so package builds skip it by
default.
