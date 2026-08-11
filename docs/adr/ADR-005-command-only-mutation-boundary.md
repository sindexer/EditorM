# ADR-005 - Command-only persistent mutation boundary

Status: Accepted  
Date: 2026-08-08

## Context

Gate 0B requires persistent document edits to be impossible through the normal public API
without a typed command. Phase 0A exposed invariant-safe raw mutators on `Document`; leaving
them public would make commands optional rather than architectural.

## Decision

`Document` remains the persistent semantic type in `visual_authoring_document`. Its raw
register, create, attach, detach, reparent, delete, and property mutation methods are now
`pub(crate)`. The same crate owns private `command` and `editor` modules so command execution
can reach those methods without exposing them across a crate boundary.

The public `Editor` owns a `Document` and exposes only `document(&self) -> &Document`.
Persistent changes enter through `dispatch`, transaction update, undo/redo, or the validated
`replace_document` load boundary. There is no public `&mut Document`, `&mut Node`, raw setter,
or reversible-effect constructor.

`Document::new`, `with_root`, and `from_snapshot` remain controlled construction/load
boundaries. They return independently validated documents and are not in-place edit APIs.

## Alternatives considered

- Keep Phase 0A mutators public and rely on convention: rejected because it is not a
  compile-time boundary.
- Put commands in a new crate: rejected for this gate because that crate would need public
  mutation capabilities from `document` or introduce a circular dependency.
- Expose a public capability token: rejected because normal consumers could retain or forge
  the capability surface.

## Dependency and ownership direction

`core_math -> document(command + editor) -> serialization`. Serialization sees read-only
Document data plus the validated construction boundary. There are no renderer, browser, UI,
or future Phase 0C dependencies.

## Public API and invariants

Public persistent editing is `Command` plus `Editor::{dispatch, begin_transaction,
update_transaction, commit_transaction, rollback_transaction, undo, redo,
replace_document}`. All nodes returned by `Document` are read-only. A compile-fail doctest on
`Editor::document` proves an external caller cannot invoke `Document::register_node`.

## Tradeoffs and future integration

Co-locating editor behavior in the document crate is less physically separated than the
long-term preferred crate topology, but it is the narrowest way to enforce Rust visibility
without an unsafe or forgeable mutation capability. A future split may move modules behind a
sealed internal interface. UI, AI, Worker, and plugin hosts can dispatch the same typed data
without receiving mutable document ownership.

## Known limitations

Rust cannot prevent a caller from constructing a separate validated document at a load
boundary. It does prevent that caller from mutating an existing Document through public raw
methods. Cross-process or WASM protocol enforcement is deferred to its authorized phase.