# Phase 1B Multiple Selection, Alignment, Distribution, and Snapping Authorization

## Scope

Work is limited to the remaining Phase 1 capabilities in `docs/ROADMAP.md` that Phase 1A intentionally excluded: multiple selection, alignment, distribution, and snapping, each fully undoable. The user requested continued product development on 2026-08-22 after Phase 1A merged to `main` as commit `8f5a551`.

Gate 1B passed on 2026-08-23 for clean `main` commit `330476b57ba4f16963f5160e24ceb82a21d66438`. This document does not authorize Phase 2 or later capabilities, and it does not pull Pen/Bezier, text, gradients, images, shadows, components, motion, AI, or export into Phase 1.

## Direct baseline

- Branch base: `main` merge commit `8f5a551` (Phase 1A).
- Approved Phase 0E-R3 payload: `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`.
- Approved tag: `phase-0e-r3-approved`.

All Phase 0 documents, ADR-001 through ADR-041, `CHECKSUMS.sha256`, and every `docs/verification/PHASE_0*` artifact remain byte-identical.

## Authorized work

- Engine-owned multiple selection with atomic validation, including replace-many and extend modes.
- Rubber-band (marquee) selection resolved through the spatial index, returning top-level nodes of the active root.
- Multi-node translation driven by one world delta per drag frame, anchored to geometry captured when the transaction opens.
- Edge and center snapping with alignment guides, computed in the runtime from bounded spatial band queries.
- Alignment (left, horizontal center, right, top, vertical center, bottom) and distribution (horizontal, vertical) planned in the runtime and applied as one transaction, therefore one undo step.
- Typed, recoverable failures with preserved failure atomicity for every new request.
- Focused regression coverage in Rust, in the EngineHost protocol, and in the editor.

## Explicit exclusions

- Multi-selection resize and rotate. Transform handles stay on a single selection.
- Pixel-grid snapping, spacing/distance measurements between objects, and user-placed guides or rulers.
- Pen/Bezier, path editing, text, gradients, image fills, shadows, auto layout, components, motion, AI integration, multiple projects, and export.
- Transform-aware Frame clipping, which remains a Phase 1A exclusion.

## Evidence requirements

Every new behaviour requires executed evidence, recorded with its command, timestamps, environment, and exit status. Stored JSON must never be presented as a fresh execution.

Gate 1B additionally requires actual Chrome, Worker, WASM, and WebGPU evidence on real hardware, including pixel readback for the new overlay. Run `phase1b-20260823T062230Z-330476b57ba4-daa2b79a` produced that evidence on Windows 10 with Chrome 151.0.7922.174 and an NVIDIA GeForce GTX 970. The authoritative result is `docs/verification/PHASE_1B_GATE_STATUS.json`: 22 PASS, 0 FAIL, 0 UNVERIFIED, conclusion PASSED.
