# ADR-010 - Revisioned semantic document change sets

Status: Accepted  
Date: 2026-08-08

## Context

`CommandOutcome::affected()` could not describe placement direction, restored/deleted IDs,
or whether a property affects scene derivation. Scene code must not inspect private history
effects or compare whole Document snapshots on the normal update path.

## Decision

The document crate exposes immutable `DocumentChangeSet` values containing before/after
revisions and typed semantic changes: insert, remove, placement, transform, geometry,
visibility, appearance, irrelevant persistent property, and full reset. Placement changes
carry old/new parent/index, subtree IDs, and whether the local transform changed.

Every dispatch and transaction preview returns a report through `CommandOutcome`. Rollback,
undo, redo, and replacement return reports directly. Transaction commit emits no second
report because preview updates already changed Document and Scene. `ReversibleEffect` remains
private and is only the internal source used to derive forward/backward descriptions.

## Invariants and tradeoffs

Reports grant no mutation capability and contain no mutable nodes, snapshots, or history
records. A report is meaningful only with its revision pair; the Scene rejects a mismatched
base revision. Reports may repeat a node across a multi-effect rollback, reflecting actual
ordered work rather than pretending effects were commutative.

