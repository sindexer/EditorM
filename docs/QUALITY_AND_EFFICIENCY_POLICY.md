# Quality and Efficiency Policy

## Non-negotiable correctness

Immediately address data loss, state-machine violations, structural performance regressions, renderer-integrity failures, secret exposure, and tests that falsely report success. Public operations must remain typed, recoverable, and failure-atomic. Cost reduction may remove repeated work, never quality criteria.

## Verification tiers

### Scope-aware pull-request checks

Every PR runs repository integrity, immutable Phase 0 history protection, Git LFS, forbidden-artifact, large-blob, and tracked-secret checks. Secret detection reports file paths and counts only; token, key, line, and substring values must never appear in logs.

The fail-closed classifier selects Rust, WASM, Editor, Preview, and hardware-evidence jobs from the actual base/head diff. Documentation-only changes skip unrelated builds with an explicit reason. Workflow, classifier, CI-script, and unknown product paths select the complete software suite. `PR Decision` fails on a missing classification, a failed or cancelled selected job, or an unexpected job result.

### Targeted subsystem checks

Run focused structural matrices, serialization round trips, Worker sequencing/restart tests, DOM input proof, renderer/WGSL validation, accessibility, and visual regression checks when the affected subsystem changes. Renderer, WGSL, render-binary-schema, and GPU-application changes require a newly added fresh local hardware proof.

### Full Phase Gate or release checks

Run the complete Rust, WASM, actual-WASM matrix, Editor, Preview, and runner-compatible milestone suite before a Phase Gate or release candidate. Validate a fresh tracked local WebGPU proof by path and SHA-256. Hosted runners cannot substitute for the local hardware execution.

## Cache and supply-chain policy

- npm caches are keyed by OS, Node version, and the relevant `package-lock.json` hash; `node_modules` is never cached.
- Cargo registry, Git database, target output, and the exact `wasm-bindgen-cli 0.2.126` are keyed by OS, Rust 1.89.0, target, and `Cargo.lock` hash.
- Caches reduce execution time; they never change the meaning of a Clippy, test, or WASM build failure.
- Clippy, workspace tests, and the pinned WASM build run once. Their first non-zero exit is the job result; CI does not delete `target` or retry automatically.
- If cache corruption is suspected, preserve the failed run and logs, then use a separate Actions rerun. Record both attempts and never present the rerun as if the first execution passed.
- Cache hit state and major job duration are written to the Actions summary.
- Third-party actions use immutable full commit SHAs with the corresponding supported release noted inline.
- Concurrency cancels obsolete runs for the same PR. Jobs have explicit timeouts.

## Evidence integrity and cost control

The future `phase-0e-r3-approved` tag points to the final Bootstrap merge commit, which must contain fixed payload commit `39085167a1b9d2ce1ba78060b3fee4d9327aaf27` as an ancestor. The 297-file byte audit always checks only that fixed payload commit, never the tag target's current tree.

The approved R3 297-file byte audit is separate from current product verification. Preserve prior-phase evidence byte-for-byte and never treat stored JSON as a new execution. Do not regenerate unchanged evidence, repeat full matrices for documentation-only changes, hide failures, introduce mocks, lower thresholds, or reduce tests solely to save time.
