# GitHub Review Workflow

## Branch and commit discipline

Create a focused branch from the reviewed `main` SHA. Separate approved product baselines from repository-governance changes. Subsequent fixes stay in the same PR as additional commits; do not create replacement ZIPs or duplicate PRs for review feedback.

## Pull requests

Open PRs as drafts. The description must identify base/head branches and SHAs, changed and excluded files, checks actually run, checks not run and why, LFS status, security/forbidden-artifact results, known limitations, and the active phase boundary.

Move a PR to Ready for review only after required CI passes. Do not merge, tag, or begin the next product phase without explicit authorization.

## Automated checks

`.github/workflows/pr-fast.yml` is the required every-PR suite. It verifies the approved baseline, LFS and repository hygiene, Rust quality, pinned WASM rebuild and ABI/protocol initialization, React/Web tests, and production builds.

`.github/workflows/phase-gate.yml` is manually dispatched for runner-compatible milestone verification. It does not claim or emulate actual hardware WebGPU proof. Fresh local hardware evidence remains mandatory when GPU-facing paths change.

## Review evidence

Ordinary PRs use diffs, commits, CI logs, and review threads. A checksummed review ZIP is generated once for a formal Phase Gate or release only. Existing verification files are historical evidence and must not be renamed, duplicated, or rewritten as new executions.

## Failure handling

Analyze CI failures before changing product tests. If a bootstrap failure would require modifying the approved product baseline, document the reason in the PR and stop for authorization. Never weaken a product assertion solely to make CI pass.
