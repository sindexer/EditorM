# ADR-008 - Linear undo redo history and branch invalidation

Status: Accepted  
Date: 2026-08-08

## Context

Gate 0B needs reversible command and transaction history, exact hierarchy/order restoration,
and redo invalidation after divergent editing without snapshot-per-event history.

## Decision

`Editor` owns private undo and redo stacks of `HistoryEntry`. An entry is an ordered vector of
opaque local `ReversibleEffect` values. A normal changed command pushes one entry. A committed
transaction pushes one entry containing all non-coalesced effects.

Undo applies effects backward in reverse order and moves the same entry to redo. Redo applies
forward in original order and moves it back to undo. Neither action creates another entry.
A changed dispatch or non-empty transaction commit clears the redo stack. No-op commands and
empty commits do not alter history. Successful document replacement clears both stacks.

Read-only `can_undo`, `can_redo`, and `history_state` expose only depths, never mutable entries.

## Alternatives considered

- Whole-document deep copy for every entry: rejected.
- Replay commands from the beginning: rejected because it is inefficient and makes external
  state/version assumptions part of undo.
- Public history entry construction: rejected because forged effects could break invariants.
- Branching history graph: deferred; Gate 0B requires linear branch invalidation.

## Invariants and failure handling

Only command execution creates entries. Stable IDs, subtree records, exact placement, and
before/after property values preserve semantic snapshots. Unexpected replay failure triggers
best-effort compensation of already-applied local effects and keeps the entry on its source
stack.

## Dependency and future integration

History is private to `Editor` in the document crate and has no serialization or renderer
dependency. Future UI and AI surfaces can query availability and invoke undo/redo without
accessing effect internals.

## Known limitations

There are no persistence, checkpoints, memory budgets, branch UI, entry labels, or history
squashing beyond transaction coalescing.