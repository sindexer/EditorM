# ADR-001 - Rust core workspace boundaries

Status: Accepted  
Date: 2026-08-07

## Context

Phase 0A needs a small Rust foundation that preserves the long-term separation between
persistent document semantics, computed scene state, rendering, browser runtime, and UI.
Creating the full future crate tree now would add empty boundaries without executable proof.

## Decision

Use a Cargo workspace with exactly three crates:

- `visual_authoring_core_math` owns pure `f64` vector, rectangle, and affine math;
- `visual_authoring_document` owns persistent identity, node semantics, hierarchy,
  invariant-safe mutations, transform resolution, and geometry bounds;
- `visual_authoring_serialization` owns the versioned file schema and validated
  persistence boundary.

Dependency direction is strictly:

```text
core_math <- document <- serialization
     ^_________________________|
```

`serialization` may also use math types while mapping its explicit schema. No browser,
renderer, UI, command, history, scene, or spatial crate exists at this gate.

## Alternatives considered

- One core crate: fewer manifests, but it would blur document and persistence ownership.
- The complete future crate topology: visually comprehensive, but empty crates would not
  prove architecture and would invite premature cross-layer assumptions.
- A TypeScript document core: prohibited by the authoritative architecture.

## Why this decision

The chosen graph is the smallest graph that makes dependency direction enforceable by
Cargo. Math and document tests run without a browser or GPU, and serialization cannot
become the in-memory source of truth.

## Tradeoffs

Some domain-to-schema mapping code is intentionally repetitive. That cost makes the
persistence boundary visible and prevents private caches from being serialized later.

## Boundary impact

- `document` imports only `core_math` plus generic infrastructure crates.
- `serialization` receives and returns semantic `Document` values through validated
  snapshots.
- No runtime or presentation data crosses these boundaries.

Critical third-party infrastructure is limited to `uuid` (MIT/Apache-2.0), `serde` and
`serde_json` (MIT/Apache-2.0), and `thiserror` (MIT/Apache-2.0). `proptest`
(MIT/Apache-2.0) is test-only. All are replaceable without changing document semantics.

## Migration / reversibility

Future crates can be added in the topology defined by the architecture spec. Splitting a
current crate further remains possible because the public ownership lines are explicit.

## Future impact

Commands can depend on `document`; computed scene can derive from it; render crates can
depend on scene/render representations without becoming dependencies of document or math.
