# ADR-009 - Selection as non-persistent editor session state

Status: Accepted  
Date: 2026-08-08

## Context

Selection must not become persistent document truth or depend on renderer, camera, hit testing,
or UI nodes.

## Decision

`Editor` owns `Selection`, an ordered vector of existing NodeIds plus an optional primary ID.
Public editor methods implement replace, add, remove, toggle, clear, contains/query, and
sanitization. Adding or toggling a missing ID returns `SelectionError`; locked nodes remain
selectable because lock controls editing, not inspection.

The latest added/selected ID is primary. Removing the primary promotes the last remaining ID.
Document mutations, undo/redo, and replacement sanitize missing IDs. Undoing a deletion does
not automatically restore selection because selection remains independent session state.
Replacement preserves only selected IDs that also exist in the new document.

Selection changes never produce commands or history entries. `Selection`, history, and active
transaction have no fields in version 1 serialization.

## Alternatives considered

- Serialize selection into Document: rejected because it would make session state persistent.
- Represent overlays/handles as nodes: rejected because they are editor presentation state.
- Automatically restore selection through document history: rejected because it couples two
  independent lifecycles.
- Reject selection of locked nodes: rejected; lock is an edit policy, not visibility/access.

## Dependency, API, and invariants

Selection is pure Rust in `crates/document/src/editor.rs`. It depends only on NodeId existence
queries. It has no scene, spatial, camera, renderer, browser, or serialization dependency.

## Tradeoffs and future integration

Sanitization is currently proportional to selection size, not document size. Future hit testing
can propose IDs to the same API without moving selection ownership. Anchor/range semantics
beyond the primary ID are deferred.

## Known limitations

Marquee, nested target resolution, hover, handles, multi-selection transform, and selection
bounds belong to later authorized phases.