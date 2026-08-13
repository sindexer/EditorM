# REVIEW PACKET - PHASE 0B

## 1. Revision and authorization

- Date: 2026-08-08 (Asia/Seoul).
- Commit/branch: N/A - the supplied workspace is not a Git repository.
- Revision: Phase 0B Command, Transaction, History, Selection Foundation.
- Authorization: `docs/PHASE_0B_AUTHORIZATION.md` records explicit external approval.
- Gate status before work: Phase 0A approved with 42 passing tests.

## 2. Implemented scope

- Typed deterministic commands for every current persistent mutation.
- Typed validation and atomic failure behavior.
- Rust visibility-enforced command-only mutation boundary.
- Command-local opaque reversible effects; no full-Document history snapshots.
- Transaction begin/update/commit/rollback with adjacent property coalescing.
- Linear undo/redo, depth queries, and divergent redo-branch invalidation.
- Ordered editor-session selection with primary selection and sanitization.
- Controlled document replacement/load policy.
- Phase 0B ADRs, tests, README, and review packaging.

## 3. Explicitly unimplemented

No Phase 0C or later work was started. There is no computed scene, dirty propagation,
spatial index, hit testing, camera/screen-world conversion, renderer, WebGPU/wgpu, WASM,
Worker, React/browser UI, Wanted Design System UI implementation, snapping, vector/path/text
engine, AI command interpreter, PSD/After Effects bridge, or placeholder future interface.

## 4. Final dependency and ownership graph

```text
visual_authoring_core_math
          |
          v
visual_authoring_document
  |-- persistent Document (private fields)
  |-- command module (typed requests + private effects)
  `-- editor module (Document + transaction + history + selection owner)
          |
          v
visual_authoring_serialization
  `-- version 1 validated construction/load boundary
```

`serialization` also imports `core_math` for explicit matrix/size field mapping. `cargo tree
--workspace` confirmed no cyclic local dependency. No new dependency or crate was added for
Phase 0B.

## 5. Public mutation API and bypass evidence

| Public surface | Access | Meaning |
|---|---|---|
| `Editor::document()` | `&Document` only | Read-only persistent truth |
| `Editor::dispatch(Command)` | Mutable Editor | One validated history step |
| transaction methods | Mutable Editor | Preview effects and one commit step |
| `undo` / `redo` | Mutable Editor | Apply opaque internal effects |
| `replace_document(Document)` | Mutable Editor | Validated load/replacement boundary; clears history |
| selection methods | Mutable Editor | Session state only; never document history |
| `Document::{new,with_root,from_snapshot}` | Returns new Document | Controlled construction/load, not in-place editing |
| `Document` queries/snapshot | Shared read | Lookup, transforms, bounds, validation, persistence |

Concrete enforcement:

- `crates/document/src/lib.rs:442-629` declares register/create/set/attach/detach/
  reparent/delete/restore methods `pub(crate)`.
- `crates/document/src/editor.rs:176-245` owns Document and exposes no `document_mut`.
- `crates/document/src/command.rs:155` keeps `ReversibleEffect` crate-private.
- `crates/document/src/editor.rs:207` contains a compile-fail doctest proving an external
  consumer cannot call `Document::register_node`.
- Serialization tests were migrated to `Editor::dispatch`; no crate outside `document`
  invokes a raw mutator.
- There is no public `&mut Document`, `&mut Node`, raw setter, unsafe capability, or global
  mutable state.

## 6. Command inventory and validation

| Command | Persistent effect | Primary validation |
|---|---|---|
| `RegisterNode` | Create detached explicit-ID node | kind, duplicate ID, finite transform, geometry, appearance |
| `CreateNode` | Register and attach atomically | all register checks, parent, container, index, lock |
| `Attach` | Attach detached node | existence, current parent, container, index, cycle, lock |
| `Detach` | Preserve detached-subtree semantics | existence, root, attachment, lock |
| `DeleteSubtree` | Delete complete subtree | existence, root, locked target/ancestor/descendant |
| `Reparent` | Move/reorder with local transform | existence, root, container, index, cycle, lock |
| `ReparentPreservingWorld` | Move/reorder with derived local transform | reparent checks, invertibility, finite derived result |
| `SetLocalTransform` | Replace local affine matrix | existence, root, lock, all components finite |
| `SetName` | Replace name | existence and lock |
| `SetVisible` | Replace visibility | existence and lock |
| `SetLocked` | Replace lock flag | existence; explicit unlock exception policy |
| `SetGeometry` | Replace geometry payload | existence, lock, supported kind, matching type/finite size |
| `SetAppearance` | Replace appearance | existence, lock, finite opacity in 0..=1 |
| `SetMetadata` | Replace metadata map | existence and lock |

Commands contain final NodeIds before execution. They are typed data, not closures or UI
callbacks. Success returns `CommandOutcome`; user-editable validation returns `CommandError`
without panic or silent clamp.

Lock policy: a node is not editable when it or an ancestor is locked. Deleting an ancestor
is also rejected if any descendant is locked. Unlocking a locked node is allowed only when
its ancestors are unlocked. Locked nodes remain selectable.

## 7. Reversible effect and history storage

`crates/document/src/command.rs:155-386` stores local semantic deltas:

| Operation | Stored reversible data |
|---|---|
| insert/create | explicit `NodeSpec` and optional parent/index |
| delete | deleted subtree records plus original parent/index |
| move/reparent/reorder | before/after parent/index and exact local transforms |
| property | target plus before/after value |
| transaction | ordered vector of the above effects |

Effects are opaque outside the crate. Delete undo validates its local record before restoring
exact IDs, node data, hierarchy, and sibling order. History does not contain `Document`,
`DocumentSnapshot`, or a full-document clone.

## 8. Transaction state machine

```text
Idle --begin--> Active
Active --update(command succeeds)--> Active with preview effect
Active --update(command fails)--> Active unchanged
Active --commit(non-empty)--> Idle + one undo entry
Active --commit(empty/no-op)--> Idle + no entry
Active --rollback--> Idle + effects reversed
```

Nested begin is rejected. Normal dispatch, undo, redo, document replacement, and
`into_document` are rejected while active. Adjacent same-target transform/property effects
coalesce to the first before-value and latest after-value. Redo is retained during preview,
cleared only by a non-empty commit, and preserved by rollback.

## 9. Undo redo and branch semantics

`crates/document/src/editor.rs:121-169,331-359` uses private undo/redo stacks of local effect
entries. Undo runs effects backward in reverse order; redo runs forward in original order.
The same entry moves between stacks, so replay creates no new history entry. Any changed
non-transaction command or non-empty committed transaction clears redo. Read-only
`HistoryState` exposes undo/redo depths.

Unexpected internal replay failure compensates already-applied local effects and leaves the
entry on its source stack. Stable IDs, child order, properties, transforms, deleted subtree
content, and world-preserving reparent results are covered by exact snapshot comparisons.

## 10. Selection and serialization exclusion

`Selection` is owned by `Editor` (`crates/document/src/editor.rs:8-91`) and stores ordered
NodeIds plus a primary ID. Replace/add/remove/toggle/clear/query and missing-ID sanitization
are implemented. Deleted IDs are removed after command/redo; undo restores document nodes but
not selection. Replacement preserves only IDs also present in the new document.

Version 1 serialization remains unchanged. `crates/serialization/src/lib.rs:523` asserts
that history, selection, and transaction keys are absent while all three session states
exist. Selection operations never create commands or history entries.

## 11. Document replacement and load policy

- Replacement is rejected during an active transaction.
- Candidate Document invariants are validated before replacement.
- Failure preserves the existing document, history, and selection.
- Success clears undo/redo and sanitizes selection against the replacement.
- JSON load still returns a separately validated Document through `Document::from_snapshot`.
- Invalid JSON/load never reaches `Editor::replace_document`.

Evidence: `crates/document/src/editor.rs:364` and
`crates/serialization/src/lib.rs:553`.

## 12. Verification results

Executed on Windows with Rust/Cargo 1.89.0 via `tools/cargo.ps1` and the isolated
`VAE_TOOL_ROOT` toolchain.

| Command | Actual result |
|---|---|
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check` | Passed |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings` | Passed, 0 warnings |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets` | Passed |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets` | Passed, 65 tests |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace` | Passed, 1 compile-fail doctest |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets` | Passed |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace` | Passed |

Required test-command breakdown:

- `visual_authoring_core_math`: 12 passed.
- `visual_authoring_document`: 42 passed.
- `visual_authoring_serialization`: 11 passed.
- Total: 65 passed, 0 failed, 0 ignored/skipped/measured/filtered.
- Gate 0A tests retained: 42.
- New Phase 0B unit/integration/property tests: 23.
- Additional command-boundary doctest: 1 passed, 0 failed, 0 ignored.

## 13. Major test evidence

- `every_command_family_reports_typed_errors_without_partial_mutation`: typed errors and
  exact document/history atomicity across every command family.
- `delete_undo_restores_complete_subtree_ids_data_and_order`: subtree, IDs, data, parent,
  child order, selection sanitize, undo, and redo.
- `world_preserving_reparent_is_exactly_undoable_and_redoable`: world result and snapshots.
- `same_parent_reorder_undo_and_redo_preserve_exact_child_order`: reorder history semantics.
- `one_hundred_twenty_eight_drag_updates_commit_as_one_history_step`: 128 preview updates,
  one commit entry, one-step undo.
- `transaction_rollback_restores_exact_semantic_state`: rollback and history preservation.
- `failed_transaction_update_keeps_prior_preview_consistent`: failed update isolation.
- `divergent_edit_after_undo_invalidates_redo_branch`: linear branch invalidation.
- `all_command_effects_round_trip_initial_and_final_snapshots`: full undo/redo sequence oracle.
- `randomized_property_command_sequences_undo_and_redo_exactly`: generated sequence oracle.
- `selection_order_toggle_primary_and_missing_id_policy_are_explicit`: selection semantics.
- `locked_parent_blocks_create_and_reparent_commands_atomically`: lock policy.
- `failed_world_preserving_command_with_extreme_finite_values_is_atomic`: numeric safety.
- `editor_session_history_selection_and_transaction_are_not_serialized`: session exclusion.
- `failed_load_preserves_editor_and_successful_replacement_resets_session_policy`: load policy.
- `Editor::document` compile-fail doctest: raw mutation is not public.

## 14. Changed source and documents

Changed from the approved Gate 0A baseline:

- `README.md`
- `crates/document/src/lib.rs`
- `crates/serialization/src/lib.rs`

New:

- `crates/document/src/command.rs`
- `crates/document/src/editor.rs`
- `docs/PHASE_0B_AUTHORIZATION.md`
- `docs/REVIEW_PACKET_0B.md`
- ADR-005 through ADR-009 under `docs/adr/`.

No Cargo dependency or versioned serialization-schema change was required.

## 15. ADRs

- ADR-005: command-only mutation boundary and Rust visibility/ownership.
- ADR-006: typed commands and command-local reversible effects.
- ADR-007: transaction state machine and adjacent coalescing.
- ADR-008: linear undo/redo and divergent branch invalidation.
- ADR-009: selection ownership and serialization exclusion.

## 16. Phase 0A API migration

Phase 0A raw mutation methods changed from `pub` to `pub(crate)`. External setup and tests now
construct typed commands and call `Editor::dispatch`. Read-only Document queries and the
validated snapshot/load boundary remain public. This is an intentional breaking correction:
retaining the raw API would violate Gate 0B even if all application code happened to use
commands by convention.

## 17. Deviations

The preferred long-term topology shows separate commands/history/editor crates, but the
instruction explicitly permits modules inside `document` when Rust crate boundaries require
it. Co-location was selected to keep raw mutators crate-private without a public capability
or cyclic dependency. No functional requirement was omitted.

## 18. Known bugs

None known after format, zero-warning Clippy, debug/release builds, 65 unit/integration tests,
and the compile-fail doctest.

## 19. Known limitations and temporary scope

- History is in-memory, linear, unlabeled, and has no persistence, checkpoints, or memory cap.
- Coalescing is adjacent and same-target for transform/property effects only.
- Deleted subtree history is proportional to the deleted subtree, not the complete document.
- Selection has one primary ID but no marquee/anchor range, bounds, or multi-transform logic.
- Commands are typed Rust data but do not yet have a cross-process/WASM protocol encoding.
- `Appearance`, string metadata, and version 1 JSON remain the intentionally minimal Gate 0A
  boundaries; Phase 0B did not expand their schema.

These are explicit gate limits, not placeholders for Phase 0C systems.

## 20. Architecture risks

- A future crate split must preserve the sealed raw mutation capability.
- A future command protocol must version command payloads without exposing opaque effects.
- Large subtree deletion requires later history memory policy, but must retain exact undo.
- Future tool interactions must keep transaction begin/update/commit/rollback explicit.
- Future scene/renderer work must consume read-only Document state and never absorb history or
  selection ownership.

## 21. Authoritative inputs and packaging policy

All 18 files listed by `visual_authoring_engine_codex_package/SHA256SUMS.txt`, including the
Wanted Design System originals, matched their supplied SHA-256 values after implementation.
The authoritative package was not modified.

Review archive target:
`C:\Users\thdwl\Documents\Codex\visual_authoring_engine_phase0b_review_2026-08-08.zip`.
The archive excludes `target`, `.tools`, prior/nested ZIPs, temporary files, IDE caches, and
unnecessary binary outputs.

## 22. Gate 0B checklist

### Mutation boundary

- [x] Normal persistent mutation cannot call raw Document mutators publicly.
- [x] Application Document access is read-only.
- [x] Typed commands cover every current persistent mutation.
- [x] Reversible effects are opaque and crate-private.
- [x] Compile-fail evidence verifies the Rust visibility boundary.

### Validation and atomicity

- [x] Target/parent, duplicate ID, root, container, cycle, index, kind/geometry,
  appearance, finite transform, lock, and unsupported-operation errors are typed.
- [x] Failed commands and transaction updates preserve exact state.
- [x] Extreme finite derived failures remain recoverable and atomic.

### Transaction and history

- [x] Begin/update/commit/rollback state machine is implemented.
- [x] 128 drag-like updates create one undo step.
- [x] Empty commit creates no entry; nesting and active conflicts are rejected.
- [x] Undo/redo preserve IDs, hierarchy/order, properties, transforms, and subtree data.
- [x] Divergent edit invalidates redo.
- [x] History is local-effect based, not full-Document deep-copy based.

### Selection and serialization

- [x] Selection is ordered independent session state with a primary ID.
- [x] Delete/redo/replacement sanitize missing IDs.
- [x] Undo does not implicitly restore selection.
- [x] Selection/history/transaction are absent from version 1 JSON.
- [x] Load failure preserves the existing Editor; replacement clears history.

### Scope and verification

- [x] Existing 42 tests and 23 new tests pass.
- [x] Format, Clippy, build, test, doctest, and release build pass.
- [x] ADR-005 through ADR-009 and authorization record exist.
- [x] Phase 0C and later systems were not started.

## 23. Stop confirmation

Phase 0B implementation is complete and stops at Gate 0B. Gate 0B external review is required
before any Phase 0C work begins.