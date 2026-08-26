# ADR-049: Stable Path Editing and Extended Stroke Contract

- Status: Proposed for Phase 2B contract review
- Date: 2026-08-26
- Scope: Phase 2B only

## Context

Phase 2A stores ordered anchors with stable identities and node-local absolute handles, but its
public editing surface replaces complete path geometry. Direct editing needs smaller semantic
commands that can reject stale UI intent and preserve undo/redo meaning. The existing stroke has
only color and centered width, so cap, join, miter, and dash behavior also lack persistent and
renderer contracts.

## Decision

### Stable semantic targets

Anchor commands address `PathAnchorId`. Segment commands address the ordered pair `(start, end)`.
Rust resolves adjacency against the current document immediately before mutation. Numeric indices
may be returned as ephemeral projection hints but are never accepted as authoritative persistent
targets.

### Typed command families

Phase 2B introduces narrowly scoped commands for:

- setting one or more anchor positions or handles;
- inserting an anchor by splitting an addressed segment at `t` in `(0, 1)`;
- deleting a validated set of anchors;
- setting open or closed state;
- converting an addressed segment to straight or cubic;
- setting the complete extended-stroke value.

Every command validates its complete request before mutation. Multi-anchor edits are one command
and one history entry. Drag previews use the existing transaction boundary and coalesce into that
single history effect.

### Curve-preserving insertion

Insertion uses De Casteljau subdivision in Rust. The new anchor and controls replace one segment
with two geometrically equivalent segments. The caller supplies the new stable anchor identity;
duplicates are rejected. Straight segments use the same algorithm with collapsed controls.

### Deterministic straight/cubic conversion

Straight conversion removes the outgoing handle of the start anchor and incoming handle of the end
anchor. Cubic conversion preserves existing applicable controls; missing controls are placed at
one third and two thirds of the straight chord. Zero-length segments receive collapsed controls at
their anchors. No camera or zoom value participates in persistent geometry.

### Stroke schema

`Stroke` gains closed enums for cap and join, a finite miter limit of at least `1.0`, and a dash
pattern limited to 64 entries. The stored pattern retains author order exactly. An empty pattern is
solid. A non-empty pattern must contain finite non-negative lengths and at least one positive
value. The renderer may build a derived repeated-even sequence without rewriting stored data.

The document format advances by one version. Older versions migrate to butt caps, miter joins, a
miter limit of `4.0`, and a solid empty dash pattern. Save/load/save of the new version is stable.

### Layer ownership

- React owns transient hover, marquee, anchor/handle selection, drag orchestration, and affordance
  presentation.
- Rust/WASM owns adjacency, curve subdivision, geometry validation, persistent stroke data,
  commands, undo/redo, serialization, bounds, and hit testing.
- The render model/WebGPU renderer owns derived stroke tessellation and batching.

## Consequences

- Stale browser intent fails explicitly instead of editing whichever anchor later occupies an
  index.
- Path editing remains compatible with future AI command callers because UI and automation share
  the same semantic command surface.
- Geometry-preserving insertion and deterministic conversion can be tested below the UI.
- Stroke tessellation becomes more complex, but only one affected path may be rebuilt per edit.

## Rejected alternatives

- Persisting selected anchor indices: rejected because insertion and deletion make them stale.
- Sending fully replaced geometry for every edit: rejected because it hides semantic intent and
  weakens stale-input validation.
- Computing subdivision or cubic handles in React: rejected because it duplicates document truth.
- Adding dash offset or variable-width strokes now: rejected as unauthorized scope expansion.
- Rebuilding all path vertices after one edit: rejected by Phase 2 bounded-work requirements.
