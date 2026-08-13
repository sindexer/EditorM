# GitHub Review Workflow

## Branch, commit, and review discipline

Create a focused branch from the reviewed base SHA. Keep approved product baselines distinguishable from repository-governance changes. Review corrections stay in the same PR as follow-up commits; do not create duplicate PRs, replacement ZIPs, or per-PR review packages.

Open PRs as drafts and move them to Ready for review only after `PR Decision` and all selected jobs pass. Do not merge, tag, or start a new product phase without explicit authorization.

## Approved baseline audit versus current product verification

`.github/workflows/baseline-audit.yml` separates two roles. `governance` checks out the PR head, workflow-dispatch governance commit, or future tag target so the workflow and verification scripts exist. `approved_baseline` always checks out fixed payload commit `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`, verifies all 297 package checksums, and validates the approved Git LFS object.

The future `phase-0e-r3-approved` tag points to PR #1's final Bootstrap merge commit, not directly to the payload commit. The workflow requires the tag name exactly, verifies that the fixed payload is an ancestor of the tag/governance target, and never claims the tag target's current product tree is byte-identical to the 297-file payload. Workflow dispatch may name only a lineage target; it cannot replace the fixed audited payload. Historical Phase 0 evidence is stored evidence, not a newly executed product test.

Future product branches are not required to remain byte-identical to R3. `.github/workflows/pr-fast.yml` instead verifies that the approved commit remains an ancestor and that files which existed in the approved history have not been changed, deleted, renamed, or overwritten. Protected history includes `CHECKSUMS.sha256`, Phase 0 authorization/review/verification files, ADR-001 through ADR-041, and the authoritative package.

## Scope-aware general PR checks

Repository integrity, Git LFS, forbidden-artifact, large-blob, and tracked-secret checks always run. `tools/ci-change-scope.ps1` then selects only affected software jobs:

| Scope | Required jobs |
| --- | --- |
| Documentation only | Integrity and classification |
| Rust/Cargo | Rust; WASM when core, Cargo, bridge, protocol, or schema can affect it |
| WASM/core/protocol/schema | WASM and relevant Rust checks |
| `web/editor` | Editor tests, TypeScript check, and build |
| `web/phase0d-preview` | Preview tests and build |
| Renderer/WGSL/GPU application | All relevant software jobs plus a newly added fresh local hardware proof |
| Workflow, CI, classifier, or unknown path | Full software suite, fail-closed |

`PR Decision` always runs. A selected job must succeed, an intentionally unselected job must be `skipped`, and a missing classification or result fails the PR. Its summary records why each job ran or was omitted.

## Failure semantics and cache recovery

Caches reduce time only. Clippy, workspace tests, and the pinned WASM build each run once, and the first non-zero exit fails the job without automatic `target` deletion or retry. Suspected cache corruption is investigated by preserving the failed run and logs, then starting a separate Actions rerun. If a rerun passes, both attempts remain recorded and the initial failure is not rewritten.

## Phase Gate and hardware boundary

`.github/workflows/phase-gate.yml` is manually dispatched with three separate inputs: an exact confirmation string, a tracked `docs/verification` JSON path, and its SHA-256. Inputs are passed through step environment variables and read as data; they are never inserted into or evaluated as PowerShell code.

The evidence validator requires valid JSON, a matching hash, actual WebGPU and actual hardware flags, no mock or Canvas2D fallback, pixel readback, DOM input proof, all checks passing, adapter and driver details, and command/timestamps within the freshness window. GitHub-hosted runners validate the tracked file only; they do not execute or claim actual hardware WebGPU proof.

## Evidence and packaging

Ordinary PRs use commits, diffs, CI logs, summaries, and review threads. A checksummed review ZIP is created only for a formal Phase Gate or release. Existing verification files are immutable history; add new evidence under a new path rather than rewriting an old run.
