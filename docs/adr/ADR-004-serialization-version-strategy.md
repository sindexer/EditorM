# ADR-004 - Serialization and version strategy

Status: Accepted  
Date: 2026-08-07

## Context

The file format must be versioned, preserve stable IDs and sibling order, reject invalid
hierarchies, and stay distinct from Rust's incidental private representation and all future
runtime caches.

## Decision

Use an explicit JSON envelope:

```json
{
  "format": "visual-authoring-document",
  "version": 1,
  "document": {}
}
```

Private `Stored*V1` types define the schema. They map to a hierarchy-complete semantic
`DocumentSnapshot`, and every load calls `Document::from_snapshot` before returning a
document. Unknown formats and versions fail with typed errors. Version 1 rejects dangling,
inconsistent, duplicate-ID, and cyclic data; it never silently normalizes it.

Nodes are emitted in stable `NodeId` order and children remain in semantic sibling order,
making save/load/save textually stable for unchanged documents.

## Alternatives considered

- Derive serialization directly on `Document`: concise but would couple files to private
  maps/caches and make runtime-state leakage likely.
- Store only parent references: smaller but makes order reconstruction and corruption
  diagnostics less explicit.
- Normalize corrupt input: potentially user-friendly later, but unsafe without a reviewed,
  deterministic recovery policy.

## Why this decision

Explicit schema DTOs create a migration boundary from version 1 and force all input through
the same invariant checker used by the document foundation.

## Tradeoffs

Parent and child references are redundant and require consistency validation. Mapping code
must be updated deliberately when persistent node semantics evolve.

## Boundary impact

`serde`/`serde_json` own generic JSON encoding only. The serialization crate owns schema
shape, while the document crate owns semantic validity. No renderer, UI, editor-session,
or computed-scene state crosses this boundary. Both dependencies are MIT/Apache-2.0 and
replaceable behind the public load/save functions.

## Migration / reversibility

`from_json` dispatches on the envelope version. Future versions can deserialize to their own
DTOs, migrate to the current semantic snapshot, then invoke document validation.

## Future impact

Interop formats, archive containers, or binary encodings can coexist as separate adapters
without making any external format the document's source of truth.
