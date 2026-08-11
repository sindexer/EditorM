# EditorM Agent Operating Rules

These instructions apply to the entire repository.

## Authorization boundary

- Phase 0E-R3 is the approved baseline. Do not implement Phase 1 or later work without an explicit phase authorization.
- Preserve the product sequence in `docs/ROADMAP.md`; do not pull later requirements into an earlier phase.
- Treat `CHECKSUMS.sha256` as immutable evidence for the approved Phase 0E-R3 package. GitHub-only operational additions are intentionally outside that checksum scope.

## Change discipline

- Use typed, recoverable errors at public boundaries and preserve failure atomicity.
- Keep Document, Scene, Render, Worker, GPU, selection, history, and diagnostics revisions synchronized.
- Do not replace bounded or order-statistic structures with full scans, dense rewrites, or hidden fallback rebuilds.
- Add focused regression tests for behavioral changes. Run the smallest relevant checks while iterating and the required gate suite before a gate or release candidate.
- Never rewrite historical verification evidence as if it were a new execution. New evidence must include the command, timestamps, environment, exit status, and raw samples where applicable.

## GitHub workflow

- Follow `docs/GITHUB_REVIEW_WORKFLOW.md` and `docs/QUALITY_AND_EFFICIENCY_POLICY.md`.
- Keep product changes and repository-governance changes distinguishable in commits and PR descriptions.
- Do not commit `target`, `node_modules`, `dist`, caches, review ZIPs, credentials, or local-only settings.
- Large approved binary originals must use Git LFS. Never replace an LFS-managed source with a normal Git blob.
- GitHub-hosted checks cannot substitute for actual hardware WebGPU proof. Attach fresh local hardware evidence when renderer, WGSL, binary schema, or GPU application paths change.
