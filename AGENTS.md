# EditorM Agent Operating Rules

These instructions apply to the entire repository.

## Authorization boundary

- Phase 0E-R3 remains the approved baseline for Phase 0 evidence. Phase 1A and Phase 1B merged to `main`; Gate 1B passed on source commit `330476b57ba4f16963f5160e24ceb82a21d66438` with the tracked evidence recorded by `docs/PHASE_1B_AUTHORIZATION.md` and `docs/verification/`.
- Phase 2 is authorized by the merged `docs/PHASE_2_AUTHORIZATION.md` and must proceed strictly in its Phase 2A through Phase 2E order. The Phase 2A path schema checkpoint merged to `main` in PR #19. Do not begin Phase 2B until the authorized Phase 2A Pen/Bezier creation, runtime, renderer, browser, and required hardware evidence are complete and reviewed. Phase 3 and later remain unauthorized.
- Preserve the Phase 1 through Phase 8 order in `docs/ROADMAP.md`; do not pull later requirements into an earlier phase.
- Treat `CHECKSUMS.sha256` and the Phase 0 files that existed at approved commit `39085167a1b9d2ce1ba78060b3fee4d9327aaf27` as immutable history. Add a new ADR instead of rewriting ADR-001 through ADR-041.
- Record every phase's scope in a `docs/PHASE_*_AUTHORIZATION.md` and its executed evidence under `docs/verification/`. State plainly which required evidence a task could not produce instead of omitting it.

## Change discipline

- Keep product, test, evidence, and repository-governance changes distinguishable.
- Use typed, recoverable errors at public boundaries and preserve failure atomicity.
- Do not replace bounded or order-statistic structures with full scans, dense rewrites, or hidden fallback rebuilds.
- Never represent stored verification JSON as a new execution. New evidence must record its command, timestamps, environment, exit status, and raw samples where applicable.
- Do not commit `target`, `node_modules`, `dist`, caches, review ZIPs, credentials, local-only settings, internal Qwen addresses, tokens, or account data.

## GitHub workflow

- Follow `docs/GITHUB_REVIEW_WORKFLOW.md` and `docs/QUALITY_AND_EFFICIENCY_POLICY.md`.
- General PRs always run integrity, Git LFS, forbidden-artifact, large-blob, and secret checks. Other jobs are selected by the fail-closed changed-path classifier, and every skip must have a recorded reason in `PR Decision`.
- CI/workflow/classifier changes and unknown product paths run the full software suite. Documentation-only PRs do not repeat unrelated Rust, WASM, or web builds.
- The one-time 297-file R3 byte audit belongs to `baseline-audit.yml` and always targets fixed payload commit `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`. The future `phase-0e-r3-approved` tag points to the final Bootstrap merge commit, which must contain that payload as an ancestor; do not apply R3 byte identity to the tag target or future product worktrees.
- Phase Gates run the complete software suite. Renderer, WGSL, render-binary-schema, or GPU-application changes also require fresh tracked local hardware evidence. A hosted runner may validate that evidence but cannot claim it executed the hardware proof.
- Large approved binary originals must use Git LFS. Never replace an LFS-managed source with a normal Git blob.
- Address review feedback as follow-up commits in the same PR. Do not create a replacement PR or a review ZIP for each PR.
- Reduce repeated cost with path selection and keyed caches; never reduce assertions, test counts, or gate thresholds to save time.
- Preserve first-failure semantics: do not automatically delete `target` or retry failed Clippy, tests, or WASM builds. Preserve the failed run and use a separate Actions rerun when cache corruption is suspected.
