# Phase 0E-R3 Complexity and Evidence Integrity Authorization

## Scope

Work is limited to the Phase 0E-R3 complexity, instrumentation, restoration, atomicity, and evidence corrections requested by the user on 2026-08-11. Gate 0E-R2 remained held when this work began. This document does not authorize or begin Phase 1.

## Direct baseline

- Archive: `visual_authoring_engine_phase0e_r2_review_2026-08-11.zip`
- Bytes: 48,176,877
- SHA-256: `2292fb079d4a5b6da4bedcd3f026277a6b2c6ae372193616112e1b2766ff2bb1`

Prior Phase 0D, 0E, 0E-R1, and 0E-R2 documents and evidence must remain byte-identical.

## Authorized corrections

- Remove k-dependent quadratic Group and Ungroup planning.
- Replace ID-derived randomized priority order with a worst-case logarithmic ranked sequence.
- Add mandatory tracked constructors and complete sequence work counters.
- Introduce Group restoration schema version 2 with deterministic run/anchor behavior and version-1 migration.
- Preserve every runtime plane and diagnostic counter after failed public requests.
- Produce actual `engine_host_bg.wasm` evidence, not native EngineHost evidence relabeled as WASM.
- Execute the full N/k/pattern matrix for native runtime, native EngineHost, actual WASM, React ProjectionStore, and actual Chrome Worker/WASM/UI.
- Preserve all pre-fix failures and do not reuse stored JSON as a fresh execution result.

## Required matrix and thresholds

- N: 10,000 and 100,000.
- k: 3, 30, 100, 300, 1,000, 3,000, and 10,000.
- Patterns: contiguous front, contiguous middle, uniform, fixed-seed random, and multiple runs.
- Operations: Group, Ungroup, Undo, and Redo.
- At least 10 warm-ups and 30 measured iterations, with raw samples, median, p95, exact baseline restoration, and no no-op measurements.
- Work per k at k=3,000 versus k=30: at most 2.0.
- Tenfold-k work ratio: at most 15.0.
- Same-k 100K/10K work ratio: at most 1.35.
- Actual WASM k=1,000 Group median: at most 25 ms.
- Actual WASM k=3,000/k=300 Group median ratio: at most 15.
- Full scans/copies, dense rewrites, fallback/full rebuilds, unchanged-visual GPU work, console errors, and GPU validation errors: zero.

## Required evidence and stop boundary

The R3 package must contain authorization, change manifest, metrics, checklist, review packet, ADRs, raw native/EngineHost/actual-WASM/React/Chrome evidence, atomicity and adversarial balance outputs, static audit, verification log, checksums, and a sidecar hash. Packaging must exclude generated dependencies, caches, temporary files, credentials, and nested ZIPs.

Stop after creating and verifying the Phase 0E-R3 review ZIP and sidecar. Do not start Phase 1 and do not claim external Gate approval.
