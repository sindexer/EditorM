# ADR-011 - Scene-aware runtime ownership and synchronization

Status: Accepted  
Date: 2026-08-08

## Context

A standalone Scene that callers manually synchronize would permit successful Document edits
with stale derived state. Keeping two equally named app-facing editors would also make the
wrong path easy to select.

## Decision

`visual_authoring_runtime::EngineRuntime` is the normal application owner. It owns the
explicitly named lower-level `HeadlessEditorCore`, `ComputedScene`, its spatial index, Camera,
revisions, and metrics. All persistent mutation entry points execute the command core and
synchronize the change set before returning success. The core remains public only as a
headless/persistence test boundary and does not claim scene synchronization.

Scene derivation is total for valid Document data by recording node-local invalid-derived
states. A revision mismatch or unexpected spatial failure triggers a fresh Scene rebuild;
the returned sync status names the cause and metrics increment `fallback_rebuild_count`.
Normal incremental work and explicit full replacement do not count as fallback.

## Invariants and limitations

No mutable core or Document reference is exposed by `EngineRuntime`. If even fallback rebuild
fails, the runtime returns an error rather than successful stale state. Process-level crash
recovery and cross-thread hosting belong to later authorized phases.

