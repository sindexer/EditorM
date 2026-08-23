# Phase 2A Path Schema Verification — 2026-08-23

- Record type: local software execution evidence
- Executed at UTC: 2026-08-23T06:57:55.8507247Z
- Environment: Windows 10.0.19045, PowerShell, Cargo 1.89.0
- Starting commit: `210568f72c2701c4fb91bd2314353ece15f16add`
- Scope: Phase 2A persistent path schema, document validation/commands, serialization v3 compatibility, and explicit pre-render omission

## Successful executions

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
```

- Exit status: 0
- Result: all workspace unit, integration, bounded-structure, runtime, WASM bridge, and doc tests passed.
- Relevant new coverage: stable path-anchor identity and bounds, invalid-command failure atomicity, serialization v3 round trip, v1/v2 load compatibility, malformed duplicate-anchor rejection, and explicit render-model omission.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
```

- Exit status: 0
- Result: all workspace targets passed with warnings denied after MSRV-compatible cleanup.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test -p visual_authoring_document -p visual_authoring_serialization -p visual_authoring_render_model
```

- Exit status: 0
- Result: 52 document unit tests, 3 document integration tests, 16 serialization tests, 1 serialization integration test, 9 render-model tests, and 1 document doc-test passed.

## Preserved first-failure record

The first combined full-test/Clippy execution completed the full test suite successfully, then Clippy exited 1 because two uses of `Option::is_none_or` require Rust 1.82 while the workspace MSRV is Rust 1.81. A separate Clippy execution then reported two manual range-pattern warnings and exited 1. The implementation was changed to MSRV-compatible `map_or` checks and `1..=3` version ranges; the successful executions above are fresh commands after those changes. No failed build output or cache was deleted.

## Evidence boundary

This is software-only evidence. It does not claim Pen-tool UI, path editing, GPU path tessellation/rendering, or hardware validation. Those remain later Phase 2A checkpoints under `docs/PHASE_2_AUTHORIZATION.md`.
