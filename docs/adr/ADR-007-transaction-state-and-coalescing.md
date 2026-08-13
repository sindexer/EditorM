# ADR-007 - Transaction state machine and adjacent coalescing

Status: Accepted  
Date: 2026-08-08

## Context

Interactive previews require begin/update/commit/rollback, one history step per interaction,
and exact cancellation without storing a full starting Document snapshot.

## Decision

`Editor` permits one optional active transaction. `begin_transaction` rejects nesting.
`update_transaction` executes a command immediately into preview document state and appends
its local reversible effect. Adjacent effects for the same transform or property and target
coalesce by retaining the first `before` and latest `after` value.

`commit_transaction` removes semantic no-ops and creates at most one history entry. Empty
commit creates none. `rollback_transaction` applies the accumulated effects backward in
reverse order. Undo, redo, normal dispatch, document replacement, and consuming the document
are rejected while a transaction is active.

A failed update makes no document or transaction-record change. Selection is independent
session state; it is sanitized after successful preview mutations and is not restored by
rollback.

## Alternatives considered

- Nested transactions: rejected because no required nesting semantics exist at Gate 0B.
- Snapshot at begin: rejected as the transaction/history foundation.
- Record every pointer update separately: rejected due history and memory growth.
- Clear redo on every preview update: rejected because rollback must preserve the prior
  history branch; redo is cleared only by a non-empty commit.

## Dependency, API, and invariants

The state machine lives in `crates/document/src/editor.rs`. Public methods are explicit and
return typed `EditorError` states. A transaction has only local effects and an update count;
it does not own a Document clone.

## Tradeoffs and future integration

Only adjacent same-target property effects coalesce. This handles drag/scrub workloads while
avoiding unsafe reordering across other commands. Future tool state machines can map pointer
down/move/up and Escape directly to begin/update/commit/rollback.

## Known limitations

There is no nested or named transaction, merge window, checkpoint, or collaboration meaning.
Those require later authorization and semantics.