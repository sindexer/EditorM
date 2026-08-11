# ADR-020: Checked Revision Exhaustion

- Status: Accepted
- Date: 2026-08-09

## Context

Revision advancement mixed `saturating_add(1)` with a separate unchecked `before + 1`.
At `u64::MAX` this could panic in debug builds, wrap in other builds, or desynchronize
Document, Scene, RenderModel, history, transaction preview, and selection.

## Decision

Every revision-advancing public editor path preflights the next revision with `checked_add`.
`DocumentChangeSet::changed` receives explicit before and after revisions and never computes
its own increment. Exhaustion returns `EditorError::RevisionExhausted { revision }` before
mutation. Dispatch, transaction update/rollback, undo, redo, and replacement use the same
policy.

## Consequences

Revision exhaustion is recoverable and atomic. Test-only revision injection verifies the real
`u64::MAX` boundary without exposing a production setter.