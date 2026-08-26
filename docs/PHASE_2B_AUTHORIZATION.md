# Phase 2B Authorization — Path Editing and Extended Stroke

## Status

- Authorized parent scope: `docs/PHASE_2_AUTHORIZATION.md`
- Entry condition: Phase 2A merged in PR #23 at `390bbd757b5ec8d3832d7f138598151d6c95d43c`
- Current checkpoint: contract and architecture review
- Phase 2C and later work is not part of this checkpoint.

## Product outcome

Phase 2B makes an existing path directly editable without replacing it or changing its stable node
identity. A person can select anchors and control handles, move them, insert or delete anchors,
open or close a path, convert a segment between straight and cubic, and configure the authorized
extended stroke properties. Every edit crosses the typed Worker/WASM command boundary and remains
one deterministic, undoable document operation.

## Authorized persistent schema

### Path editing

- Anchor identity remains `PathAnchorId`; UI row indices and array offsets are never persistent
  identities.
- A segment is addressed by its stable start and end anchor identities. Commands reject stale,
  non-adjacent, duplicate, or missing identities before mutation.
- Anchor position, incoming handle, and outgoing handle remain node-local finite coordinates.
- Straight conversion clears the two controls that define the addressed segment. Cubic conversion
  creates deterministic controls in Rust only when the segment does not already have them.
- Insertion splits the addressed segment at one finite parameter strictly inside `(0, 1)` and uses
  De Casteljau subdivision so the rendered curve is unchanged within numeric tolerance.
- Deletion preserves stable identities of surviving anchors and rejects results below two anchors
  for an open path or three anchors for a closed path.

### Extended stroke

The only newly authorized stroke fields are:

- cap: `butt`, `round`, or `square`;
- join: `miter`, `round`, or `bevel`;
- miter limit: finite and at least `1.0`;
- dash pattern: at most 64 finite non-negative lengths with at least one positive length.

Dash offset, variable width, stroke alignment, arrowheads, markers, pressure profiles, and custom
caps are excluded. Adding one requires a reviewed authorization update.

## Required implementation order

1. Publish ADR-049 and the versioned Rust document/serialization contract.
2. Add typed, failure-atomic document commands and exact undo/redo tests.
3. Carry the commands through WASM/Worker with stable-identity validation.
4. Implement geometry-aware bounds, hit testing, culling, tessellation, and WebGPU rendering for
   the authorized caps, joins, miter limit, and dash pattern without fallback rebuilds.
5. Add React orchestration for anchor/handle selection and editing. React stores only transient
   interaction state and never duplicates persistent path geometry rules.
6. Reuse the existing Phase 2A direct-WASM and browser hardware harnesses for serialization,
   interaction, pixel, negative-path, and performance evidence.

## Definition of done

- Unit: Rust document, geometry, serialization, render-model, and React interaction tests pass.
- Integration: Worker/WASM commands, undo/redo, rollback, save/load/save, and restart preserve exact
  stable identities and stroke values.
- Browser E2E: real DOM input edits anchors and handles and exercises insert/delete/open/close and
  straight/cubic conversion.
- Rendering: browser pixel evidence covers every authorized cap and join plus miter fallback and a
  dash pattern on actual hardware WebGPU.
- Regression: Phase 2A Pen creation and all Phase 0/1 behavior remain passing.
- Performance: one-anchor or one-segment edits tessellate only the affected path; interaction paths
  report zero full scans, dense rewrites, and fallback rebuilds.
- Visual QA: interaction affordance clarity and stroke quality require user review after automated
  browser evidence passes.

## Explicit exclusions

- Phase 2C gradients and composite fills;
- Phase 2D text;
- Phase 2E boolean operations;
- Phase 3 pivot/origin and layout/components;
- Phase 5 slides, layers integrated with timeline rows, and animation;
- Phase 7 functional AI commands.

## Evidence boundary

Executed evidence belongs under `docs/verification/` and must record command, timestamps,
environment, source commit, exit status, and raw samples where applicable. Stored Phase 2A JSON is
historical input, not a new Phase 2B execution.
