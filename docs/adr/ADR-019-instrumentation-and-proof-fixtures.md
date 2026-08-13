# ADR-019 - Runtime instrumentation and structural proof fixtures

Status: Accepted  
Date: 2026-08-08

## Decision

Actual traversal/update/query points increment `SceneUpdateStats`. Per-operation and cumulative
metrics distinguish dirty/visited nodes, world/bounds recomputation, ancestor aggregate work,
spatial insert/remove/update, candidates, exact tests, full/fallback rebuilds, and synchronized
document/scene revisions. Current total/attached/visible/invalid/indexed counts are maintained
incrementally rather than rescanning after each edit.

Deterministic validated fixtures provide 1,000, 10,000, and 100,000 flat rectangles plus a
10,000-level nested hierarchy. Fixture construction uses the explicit validated load boundary;
normal edits still use commands. IDs derive from fixed UUID integers and placement is fixed.

`phase0c_proof` runs in release mode through `tools/phase0c-proof.ps1`. The executable writes
`docs/PHASE_0C_METRICS.json`; the PowerShell runner captures its raw stdout/stderr and exit code
in `docs/verification/PHASE_0C_VERIFICATION.txt`. Acceptance is based on structural counters,
not environment-specific wall-clock thresholds. Timings are recorded as informational values.

## Limitations

Peak memory is not measured because no portable dependency-free Windows measurement was
introduced. Renderer/GPU/worker metrics are outside Phase 0C and are not claimed.
