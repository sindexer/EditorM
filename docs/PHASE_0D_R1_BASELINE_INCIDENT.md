# Phase 0D-R1 Baseline Browser Incident

Date: 2026-08-10 (Asia/Seoul)

## Incident

The standalone baseline command `npm run test:browser` timed out while waiting
for `document.body.dataset.ready === "true"` and Worker heartbeat 1. The browser
test did not start a preview server and, without `PHASE0D_URL`, attempted to use
`http://127.0.0.1:4173/`.

An explicit health request after the timeout returned `NO_SERVER_ON_4173`.
Immediately before the standalone command, `tools/phase0d-proof.ps1` had passed
on its managed port and then correctly stopped its preview server. The original
timeout output remains in the task transcript and has not been represented as a
product test pass.

## Correctly orchestrated rerun

The following command was run without changing product source:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-verify.ps1 -BrowserPort 4181
```

It started and health-checked a new preview server, set the browser URL, launched
a new Chrome 151 process with a new temporary profile, initialized real WebGPU
on an NVIDIA GeForce GTX 970, initialized Worker-owned WASM, observed heartbeat
1 or greater, ran the complete browser automation, and stopped its managed
server. The fresh proof was captured at `2026-08-09T15:04:37.032Z`; the fresh
screenshot write time was `2026-08-09T15:04:37.0292608Z` (UTC).

- Proof kind: `actual-hardware-browser`
- Preview URL: `http://127.0.0.1:4181/`
- Actual hardware: true
- Worker runtime owner: `dedicated-worker`
- Worker heartbeat: 1 or greater
- WebGPU validation errors: 0
- Browser console errors: 0
- Canvas2D/mock fallback: not used
- `all_passed`: true

## Determination

The standalone timeout was caused by a missing test-harness invocation
prerequisite, not a product-code regression: the expected port had no server,
while the existing lifecycle-aware orchestrator produced a fresh real-hardware
pass. Phase 0D-R1 structural correction is therefore allowed to resume. R1 will
also make `npm run test:browser` self-contained so this invocation error cannot
recur.
