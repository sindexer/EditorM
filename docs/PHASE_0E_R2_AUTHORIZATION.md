# Phase 0E-R2 Structural Correction Authorization

- Date: 2026-08-11
- Direct baseline ZIP: `visual_authoring_engine_phase0e_r1_review_2026-08-10.zip`
- Direct baseline SHA-256: `07d6ba27f27c32b89474b098e0e1f3045bbd2d4511eb7951d1e7d03d3bafe3db`
- Direct baseline size: 47,764,327 bytes
- Gate state: Gate 0E-R1 held; Phase 1 is not authorized

## Authorized work

- Replace dense sibling arrays in Document and Scene with bounded rank/select sequences.
- Replace large React projection arrays with a bounded rank/select sequence and viewport reads.
- Correct Group/Ungroup, Undo/Redo, persistence, and deterministic restoration after sibling edits.
- Remove `__phase0e_r1_group_positions` from user metadata and use a versioned internal schema.
- Add actual Document, Scene, and UI work counters, 10K/100K release and browser proofs, order-accuracy regressions, documentation, and review packaging.

## Prohibited work

Phase 1 and later product features, new tools, React product expansion, and design expansion are not authorized. Existing Phase 0D, Phase 0D-R1-P1, Phase 0E, and Phase 0E-R1 evidence must remain byte-identical. New execution evidence uses only Phase 0E-R2 paths.

This document records the supplied authorization. It does not claim Gate approval.
