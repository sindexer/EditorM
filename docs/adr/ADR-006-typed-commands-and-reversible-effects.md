# ADR-006 - Typed commands and local reversible effects

Status: Accepted  
Date: 2026-08-08

## Context

Commands must be deterministic typed data, validate user-editable input, fail atomically, and
produce undo information without cloning the complete Document.

## Decision

`Command` is a public enum covering explicit-ID registration/creation, attach/detach,
subtree deletion, local/world-preserving reparent, and all current persistent properties.
Execution returns a public `CommandOutcome` and a crate-private `ReversibleEffect`.

Effects store command-local before/after values. Delete stores only the deleted subtree
records and original parent/index. Reparent stores before/after placement and exact local
transforms. Create stores the explicit NodeSpec and placement. Effect constructors and apply
methods are crate-private, so callers cannot forge undo records.

Validation runs before mutation and returns `CommandError`, wrapping typed `DocumentError`
where appropriate. Lock checks, target/parent existence, root restrictions, container and
index rules, cycles, geometry/appearance validity, and finite transforms are enforced without
silent clamp. Existing Phase 0A derived-numeric error behavior remains intact.

## Alternatives considered

- Closures or UI callbacks: rejected because they are not serializable, inspectable, or
  deterministic future AI inputs.
- Full Document snapshots per command: rejected as the semantic history model.
- Public inverse commands: rejected because callers could forge state-dependent undo data.

## Dependency and public API

The implementation is `crates/document/src/command.rs` and depends only on document semantics
and `core_math`. Public consumers construct `Command`; only `Editor` can execute it.

## Invariants and atomicity

Every fallible validation or derived calculation completes before mutation. Subtree restore
validates its opaque local record before inserting anything. History replay uses the same
crate-private raw primitives and compensates already-applied effects if an internal replay
step unexpectedly fails.

## Tradeoffs and future integration

Effects may contain a large deleted subtree, which is proportional to the operation being
undone, but never an unrelated full-document copy. Typed commands can become future AI,
UI, plugin, or worker protocol payloads after an explicit serialization/protocol ADR.

## Known limitations

Commands themselves are not yet persisted or sent across processes. Multi-node semantic
operations beyond the Phase 0A document model are deferred.