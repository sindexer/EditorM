use thiserror::Error;

use crate::command::{execute_command, ReversibleEffect};
use crate::{
    Command, CommandError, CommandOutcome, Document, DocumentChange, DocumentChangeSet,
    InvariantViolation, NodeId,
};

/// Ordered, ephemeral editor selection. It is never document truth.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Selection {
    ordered: Vec<NodeId>,
    primary: Option<NodeId>,
}

impl Selection {
    #[must_use]
    pub fn ordered(&self) -> &[NodeId] {
        &self.ordered
    }

    #[must_use]
    pub const fn primary(&self) -> Option<NodeId> {
        self.primary
    }

    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.ordered.contains(&id)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    fn replace(&mut self, id: NodeId) {
        self.ordered.clear();
        self.ordered.push(id);
        self.primary = Some(id);
    }

    fn add(&mut self, id: NodeId) -> bool {
        if self.contains(id) {
            self.primary = Some(id);
            false
        } else {
            self.ordered.push(id);
            self.primary = Some(id);
            true
        }
    }

    fn remove(&mut self, id: NodeId) -> bool {
        let Some(position) = self.ordered.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.ordered.remove(position);
        if self.primary == Some(id) {
            self.primary = self.ordered.last().copied();
        }
        true
    }

    fn toggle(&mut self, id: NodeId) -> bool {
        if self.contains(id) {
            self.remove(id);
            false
        } else {
            self.add(id);
            true
        }
    }

    fn clear(&mut self) {
        self.ordered.clear();
        self.primary = None;
    }

    /// Replaces the whole selection with an already validated, de-duplicated order.
    fn replace_many(&mut self, ids: &[NodeId]) {
        self.ordered.clear();
        for id in ids {
            if !self.ordered.contains(id) {
                self.ordered.push(*id);
            }
        }
        self.primary = self.ordered.last().copied();
    }

    fn sanitize(&mut self, document: &Document) -> usize {
        let previous = self.ordered.len();
        self.ordered.retain(|id| document.node(*id).is_some());
        if self.primary.is_some_and(|id| !self.ordered.contains(&id)) {
            self.primary = self.ordered.last().copied();
        }
        previous - self.ordered.len()
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SelectionError {
    #[error("node {0} cannot be selected because it does not exist")]
    NodeNotFound(NodeId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HistoryState {
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum EditorError {
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("a transaction is already active")]
    TransactionAlreadyActive,
    #[error("no transaction is active")]
    NoActiveTransaction,
    #[error("operation {operation} is not allowed while a transaction is active")]
    TransactionActive { operation: &'static str },
    #[error("replacement document is invalid: {0}")]
    InvalidReplacement(InvariantViolation),
    #[error("internal history replay failed: {0}")]
    HistoryReplay(CommandError),
    #[error("document revision {revision} cannot be advanced")]
    RevisionExhausted { revision: u64 },
}

#[derive(Clone, Debug, Default, PartialEq)]
struct HistoryEntry {
    effects: Vec<ReversibleEffect>,
}

impl HistoryEntry {
    fn from_effect(effect: ReversibleEffect) -> Self {
        Self {
            effects: vec![effect],
        }
    }

    fn normalize(&mut self) {
        self.effects.retain(|effect| !effect.is_noop());
    }

    fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    fn forward_changes(&self) -> Vec<DocumentChange> {
        self.effects
            .iter()
            .flat_map(ReversibleEffect::forward_changes)
            .collect()
    }

    fn backward_changes(&self) -> Vec<DocumentChange> {
        self.effects
            .iter()
            .rev()
            .flat_map(ReversibleEffect::backward_changes)
            .collect()
    }

    fn undo(&self, document: &mut Document) -> Result<(), CommandError> {
        let mut applied: Vec<&ReversibleEffect> = Vec::new();
        for effect in self.effects.iter().rev() {
            if let Err(error) = effect.apply_backward(document) {
                for applied_effect in applied.iter().rev() {
                    let _ = applied_effect.apply_forward(document);
                }
                return Err(error);
            }
            applied.push(effect);
        }
        Ok(())
    }

    fn redo(&self, document: &mut Document) -> Result<(), CommandError> {
        let mut applied: Vec<&ReversibleEffect> = Vec::new();
        for effect in &self.effects {
            if let Err(error) = effect.apply_forward(document) {
                for applied_effect in applied.iter().rev() {
                    let _ = applied_effect.apply_backward(document);
                }
                return Err(error);
            }
            applied.push(effect);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ActiveTransaction {
    effects: Vec<ReversibleEffect>,
    update_count: usize,
}

/// Owns the persistent document, command history, active transaction, and selection session.
pub struct HeadlessEditorCore {
    document: Document,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    transaction: Option<ActiveTransaction>,
    selection: Selection,
    revision: u64,
}

impl HeadlessEditorCore {
    pub fn new(document: Document) -> Result<Self, EditorError> {
        document
            .validate_invariants()
            .map_err(EditorError::InvalidReplacement)?;
        Ok(Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            transaction: None,
            selection: Selection::default(),
            revision: 0,
        })
    }

    #[must_use]
    pub fn blank(name: impl Into<String>) -> Self {
        Self::new(Document::new(name)).expect("the built-in document is valid")
    }

    /// Returns the persistent document through a read-only boundary.
    ///
    /// Raw mutators are crate-private and therefore unavailable to external consumers:
    ///
    /// ```compile_fail
    /// use visual_authoring_document::{Document, NodeId, NodeSpec};
    /// let mut document = Document::new("Root");
    /// document.register_node(NodeSpec::group(NodeId::new(), "Bypass"));
    /// ```
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    #[must_use]
    pub const fn selection(&self) -> &Selection {
        &self.selection
    }

    #[must_use]
    pub const fn history_state(&self) -> HistoryState {
        HistoryState {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub const fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[must_use]
    pub const fn transaction_active(&self) -> bool {
        self.transaction.is_some()
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn dispatch(&mut self, command: Command) -> Result<CommandOutcome, EditorError> {
        if self.transaction.is_some() {
            return Err(EditorError::TransactionActive {
                operation: "dispatch",
            });
        }
        self.ensure_revision_available()?;
        let previous_sequence_work = self.document.sequence_work();
        self.document.reset_sequence_work();
        let mut execution = match execute_command(&mut self.document, command) {
            Ok(execution) => execution,
            Err(error) => {
                self.document.reset_sequence_work();
                self.document.add_sequence_work(previous_sequence_work);
                return Err(error.into());
            }
        };
        execution
            .outcome
            .set_sequence_work(self.document.sequence_work());
        let changes = if let Some(effect) = execution.effect {
            let changes = effect.forward_changes();
            self.redo.clear();
            self.undo.push(HistoryEntry::from_effect(effect));
            self.selection.sanitize(&self.document);
            changes
        } else {
            Vec::new()
        };
        let change_set = self.advance_revision(changes)?;
        execution.outcome.set_change_set(change_set);
        Ok(execution.outcome)
    }

    pub fn begin_transaction(&mut self) -> Result<(), EditorError> {
        if self.transaction.is_some() {
            return Err(EditorError::TransactionAlreadyActive);
        }
        self.transaction = Some(ActiveTransaction::default());
        Ok(())
    }

    pub fn update_transaction(&mut self, command: Command) -> Result<CommandOutcome, EditorError> {
        if self.transaction.is_none() {
            return Err(EditorError::NoActiveTransaction);
        }
        self.ensure_revision_available()?;
        let previous_sequence_work = self.document.sequence_work();
        self.document.reset_sequence_work();
        let mut execution = match execute_command(&mut self.document, command) {
            Ok(execution) => execution,
            Err(error) => {
                self.document.reset_sequence_work();
                self.document.add_sequence_work(previous_sequence_work);
                return Err(error.into());
            }
        };
        execution
            .outcome
            .set_sequence_work(self.document.sequence_work());
        let mut changes = Vec::new();
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(EditorError::NoActiveTransaction)?;
        transaction.update_count += 1;
        if let Some(effect) = execution.effect {
            changes.extend(effect.forward_changes());
            if let Some(last) = transaction.effects.last_mut() {
                if let Some(not_coalesced) = last.try_coalesce(effect) {
                    transaction.effects.push(not_coalesced);
                }
            } else {
                transaction.effects.push(effect);
            }
            self.selection.sanitize(&self.document);
        }
        let change_set = self.advance_revision(changes)?;
        execution.outcome.set_change_set(change_set);
        Ok(execution.outcome)
    }

    /// Commits the active transaction. Returns `true` when one history entry was recorded.
    pub fn commit_transaction(&mut self) -> Result<bool, EditorError> {
        let transaction = self
            .transaction
            .take()
            .ok_or(EditorError::NoActiveTransaction)?;
        let mut entry = HistoryEntry {
            effects: transaction.effects,
        };
        entry.normalize();
        if entry.is_empty() {
            return Ok(false);
        }
        self.redo.clear();
        self.undo.push(entry);
        Ok(true)
    }

    pub fn rollback_transaction(&mut self) -> Result<DocumentChangeSet, EditorError> {
        let active = self
            .transaction
            .as_ref()
            .ok_or(EditorError::NoActiveTransaction)?;
        if !active.effects.is_empty() {
            self.ensure_revision_available()?;
        }
        self.document.reset_sequence_work();
        let transaction = self
            .transaction
            .take()
            .ok_or(EditorError::NoActiveTransaction)?;
        let ActiveTransaction {
            effects,
            update_count,
        } = transaction;
        let entry = HistoryEntry { effects };
        let changes = entry.backward_changes();
        if let Err(error) = entry.undo(&mut self.document) {
            self.transaction = Some(ActiveTransaction {
                effects: entry.effects,
                update_count,
            });
            return Err(EditorError::HistoryReplay(error));
        }
        self.selection.sanitize(&self.document);
        self.advance_revision(changes)
    }

    /// Returns a change set when an entry was undone.
    pub fn undo(&mut self) -> Result<Option<DocumentChangeSet>, EditorError> {
        self.reject_during_transaction("undo")?;
        if self.undo.is_empty() {
            return Ok(None);
        }
        self.ensure_revision_available()?;
        self.document.reset_sequence_work();
        let entry = self.undo.pop().expect("history entry exists");
        let changes = entry.backward_changes();
        if let Err(error) = entry.undo(&mut self.document) {
            self.undo.push(entry);
            return Err(EditorError::HistoryReplay(error));
        }
        self.redo.push(entry);
        self.selection.sanitize(&self.document);
        Ok(Some(self.advance_revision(changes)?))
    }

    /// Returns a change set when an entry was redone.
    pub fn redo(&mut self) -> Result<Option<DocumentChangeSet>, EditorError> {
        self.reject_during_transaction("redo")?;
        if self.redo.is_empty() {
            return Ok(None);
        }
        self.ensure_revision_available()?;
        self.document.reset_sequence_work();
        let entry = self.redo.pop().expect("history entry exists");
        let changes = entry.forward_changes();
        if let Err(error) = entry.redo(&mut self.document) {
            self.redo.push(entry);
            return Err(EditorError::HistoryReplay(error));
        }
        self.undo.push(entry);
        self.selection.sanitize(&self.document);
        Ok(Some(self.advance_revision(changes)?))
    }

    /// Replaces the complete document at a controlled load boundary.
    ///
    /// Replacement is rejected during a transaction. On success history is cleared and
    /// selection is sanitized to IDs that also exist in the replacement document.
    pub fn replace_document(
        &mut self,
        document: Document,
    ) -> Result<DocumentChangeSet, EditorError> {
        self.reject_during_transaction("replace_document")?;
        self.ensure_revision_available()?;
        document
            .validate_invariants()
            .map_err(EditorError::InvalidReplacement)?;
        self.document = document;
        self.undo.clear();
        self.redo.clear();
        self.selection.sanitize(&self.document);
        self.advance_revision(vec![DocumentChange::FullDocumentReset])
    }

    pub fn into_document(self) -> Result<Document, EditorError> {
        if self.transaction.is_some() {
            return Err(EditorError::TransactionActive {
                operation: "into_document",
            });
        }
        Ok(self.document)
    }

    pub fn select_only(&mut self, id: NodeId) -> Result<(), SelectionError> {
        self.require_selectable(id)?;
        self.selection.replace(id);
        Ok(())
    }

    /// Adds an ID and makes it primary. Existing selected IDs keep their order.
    pub fn add_to_selection(&mut self, id: NodeId) -> Result<bool, SelectionError> {
        self.require_selectable(id)?;
        Ok(self.selection.add(id))
    }

    /// Replaces the selection with several IDs. Every ID is validated before any selection
    /// state changes, so a rejected request leaves the previous selection intact.
    pub fn select_many(&mut self, ids: &[NodeId]) -> Result<usize, SelectionError> {
        for id in ids {
            self.require_selectable(*id)?;
        }
        self.selection.replace_many(ids);
        Ok(self.selection.len())
    }

    /// Adds several already existing IDs to the selection, keeping current order first.
    pub fn extend_selection(&mut self, ids: &[NodeId]) -> Result<usize, SelectionError> {
        for id in ids {
            self.require_selectable(*id)?;
        }
        let mut merged = self.selection.ordered.clone();
        merged.extend_from_slice(ids);
        self.selection.replace_many(&merged);
        Ok(self.selection.len())
    }

    pub fn remove_from_selection(&mut self, id: NodeId) -> bool {
        self.selection.remove(id)
    }

    /// Toggles an existing document ID. Returns whether it is selected afterward.
    pub fn toggle_selection(&mut self, id: NodeId) -> Result<bool, SelectionError> {
        self.require_selectable(id)?;
        Ok(self.selection.toggle(id))
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Removes selected IDs that do not exist in the current document.
    pub fn sanitize_selection(&mut self) -> usize {
        self.selection.sanitize(&self.document)
    }

    fn advance_revision(
        &mut self,
        changes: Vec<DocumentChange>,
    ) -> Result<DocumentChangeSet, EditorError> {
        if changes.is_empty() {
            return Ok(DocumentChangeSet::unchanged(self.revision));
        }
        let before = self.revision;
        let after = self.next_revision()?;
        self.revision = after;
        Ok(DocumentChangeSet::changed(before, after, changes))
    }

    fn next_revision(&self) -> Result<u64, EditorError> {
        self.revision
            .checked_add(1)
            .ok_or(EditorError::RevisionExhausted {
                revision: self.revision,
            })
    }

    fn ensure_revision_available(&self) -> Result<(), EditorError> {
        self.next_revision().map(|_| ())
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn set_revision_for_test(&mut self, revision: u64) {
        self.revision = revision;
    }

    fn require_selectable(&self, id: NodeId) -> Result<(), SelectionError> {
        if self.document.node(id).is_some() {
            Ok(())
        } else {
            Err(SelectionError::NodeNotFound(id))
        }
    }

    fn reject_during_transaction(&self, operation: &'static str) -> Result<(), EditorError> {
        if self.transaction.is_some() {
            Err(EditorError::TransactionActive { operation })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;
    use visual_authoring_core_math::{Affine2, Vec2, DEFAULT_EPSILON};

    use super::*;
    use crate::{Appearance, DocumentError, Geometry, NodeSpec};

    fn create_group(editor: &mut HeadlessEditorCore, parent: NodeId, name: &str) -> NodeId {
        let id = NodeId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::group(id, name),
                parent,
                index: editor.document().node(parent).unwrap().children().len(),
            })
            .unwrap();
        id
    }

    fn create_rectangle(editor: &mut HeadlessEditorCore, parent: NodeId, name: &str) -> NodeId {
        let id = NodeId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(id, name, Vec2::new(20.0, 10.0)),
                parent,
                index: editor.document().node(parent).unwrap().children().len(),
            })
            .unwrap();
        id
    }

    fn assert_atomic_error(editor: &mut HeadlessEditorCore, command: Command) -> CommandError {
        let before = editor.document().snapshot();
        let history = editor.history_state();
        let error = editor.dispatch(command).unwrap_err();
        assert_eq!(editor.document().snapshot(), before);
        assert_eq!(editor.history_state(), history);
        match error {
            EditorError::Command(error) => error,
            other => panic!("expected command error, got {other:?}"),
        }
    }

    #[test]
    fn typed_commands_cover_all_persistent_mutation_families() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let detached = NodeId::new();
        let outcome = editor
            .dispatch(Command::RegisterNode {
                spec: NodeSpec::group(detached, "Detached"),
            })
            .unwrap();
        assert!(outcome.changed());
        assert_eq!(outcome.affected(), &[detached]);
        assert_eq!(editor.document().node(detached).unwrap().parent(), None);

        editor
            .dispatch(Command::Attach {
                child: detached,
                parent: root,
                index: 0,
            })
            .unwrap();
        editor
            .dispatch(Command::Detach { child: detached })
            .unwrap();
        editor
            .dispatch(Command::Attach {
                child: detached,
                parent: root,
                index: 0,
            })
            .unwrap();

        let rectangle = create_rectangle(&mut editor, root, "Rectangle");
        let transform = Affine2::translation(Vec2::new(12.0, 8.0))
            * Affine2::rotation(0.25)
            * Affine2::scale(Vec2::new(2.0, -0.5));
        editor
            .dispatch(Command::SetLocalTransform {
                target: rectangle,
                transform,
            })
            .unwrap();
        editor
            .dispatch(Command::SetName {
                target: rectangle,
                name: "Renamed".into(),
            })
            .unwrap();
        editor
            .dispatch(Command::SetVisible {
                target: rectangle,
                visible: false,
            })
            .unwrap();
        editor
            .dispatch(Command::SetGeometry {
                target: rectangle,
                geometry: Geometry::Rectangle {
                    size: Vec2::new(40.0, 30.0),
                },
            })
            .unwrap();
        editor
            .dispatch(Command::SetAppearance {
                target: rectangle,
                appearance: Appearance {
                    opacity: 0.4,
                    ..Appearance::default()
                },
            })
            .unwrap();
        let metadata = BTreeMap::from([("role".into(), "badge".into())]);
        editor
            .dispatch(Command::SetMetadata {
                target: rectangle,
                metadata: metadata.clone(),
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocked {
                target: rectangle,
                locked: true,
            })
            .unwrap();

        let node = editor.document().node(rectangle).unwrap();
        assert_eq!(node.local_transform(), transform);
        assert_eq!(node.name(), "Renamed");
        assert!(!node.visible());
        assert!(node.locked());
        assert_eq!(node.geometry().unwrap().size(), Vec2::new(40.0, 30.0));
        assert_eq!(
            node.appearance(),
            Appearance {
                opacity: 0.4,
                ..Appearance::default()
            }
        );
        assert_eq!(node.metadata(), &metadata);
        editor.document().validate_invariants().unwrap();
    }

    #[test]
    fn every_command_family_reports_typed_errors_without_partial_mutation() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let group = create_group(&mut editor, root, "Group");
        let child = create_group(&mut editor, group, "Child");
        let missing = NodeId::new();

        let invalid_spec = NodeSpec {
            local_transform: Affine2::from_components(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0),
            ..NodeSpec::group(NodeId::new(), "Invalid")
        };
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::RegisterNode { spec: invalid_spec }),
            CommandError::Document(DocumentError::InvalidTransform(_))
        ));
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::CreateNode {
                spec: NodeSpec::group(NodeId::new(), "Missing parent"), parent: missing, index: 0,
            }),
            CommandError::Document(DocumentError::NodeNotFound(id)) if id == missing
        ));

        let commands = vec![
            Command::Attach {
                child: missing,
                parent: root,
                index: 0,
            },
            Command::Detach { child: missing },
            Command::DeleteSubtree { target: missing },
            Command::Reparent {
                child: missing,
                new_parent: root,
                index: 0,
            },
            Command::ReparentPreservingWorld {
                child: missing,
                new_parent: root,
                index: 0,
            },
            Command::SetLocalTransform {
                target: missing,
                transform: Affine2::IDENTITY,
            },
            Command::SetName {
                target: missing,
                name: "Missing".into(),
            },
            Command::SetVisible {
                target: missing,
                visible: false,
            },
            Command::SetLocked {
                target: missing,
                locked: true,
            },
            Command::SetGeometry {
                target: missing,
                geometry: Geometry::Rectangle {
                    size: Vec2::new(1.0, 1.0),
                },
            },
            Command::SetAppearance {
                target: missing,
                appearance: Appearance::default(),
            },
            Command::SetMetadata {
                target: missing,
                metadata: BTreeMap::new(),
            },
        ];
        for command in commands {
            assert!(matches!(
                assert_atomic_error(&mut editor, command),
                CommandError::Document(DocumentError::NodeNotFound(id)) if id == missing
            ));
        }

        assert!(matches!(
            assert_atomic_error(
                &mut editor,
                Command::Reparent {
                    child: group,
                    new_parent: child,
                    index: 0,
                }
            ),
            CommandError::Document(DocumentError::CycleDetected { .. })
        ));
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::DeleteSubtree { target: root }),
            CommandError::Document(DocumentError::RootOperation(id)) if id == root
        ));
        assert!(matches!(
            assert_atomic_error(
                &mut editor,
                Command::Reparent {
                    child,
                    new_parent: root,
                    index: 99,
                }
            ),
            CommandError::Document(DocumentError::ChildIndexOutOfBounds { .. })
        ));
        assert!(matches!(
            assert_atomic_error(
                &mut editor,
                Command::SetGeometry {
                    target: group,
                    geometry: Geometry::Rectangle {
                        size: Vec2::new(1.0, 1.0)
                    },
                }
            ),
            CommandError::UnsupportedOperation { .. }
        ));
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::SetAppearance {
                target: child, appearance: Appearance { opacity: 2.0, ..Appearance::default() },
            }),
            CommandError::Document(DocumentError::InvalidAppearance(id)) if id == child
        ));
    }

    #[test]
    fn duplicate_create_and_invalid_container_are_atomic() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let rectangle = create_rectangle(&mut editor, root, "Rectangle");
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::CreateNode {
                spec: NodeSpec::group(rectangle, "Duplicate"), parent: root, index: 1,
            }),
            CommandError::Document(DocumentError::DuplicateNodeId(id)) if id == rectangle
        ));
        assert!(matches!(
            assert_atomic_error(&mut editor, Command::CreateNode {
                spec: NodeSpec::group(NodeId::new(), "Child"), parent: rectangle, index: 0,
            }),
            CommandError::Document(DocumentError::InvalidParent(id)) if id == rectangle
        ));
    }

    #[test]
    fn delete_undo_restores_complete_subtree_ids_data_and_order() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let before = create_group(&mut editor, root, "Before");
        let subtree = create_group(&mut editor, root, "Subtree");
        let child = create_rectangle(&mut editor, subtree, "Child");
        let grandchild = create_group(&mut editor, subtree, "Grandchild");
        let after = create_group(&mut editor, root, "After");
        editor
            .dispatch(Command::SetMetadata {
                target: child,
                metadata: BTreeMap::from([("key".into(), "value".into())]),
            })
            .unwrap();
        let complete_before = editor.document().snapshot();
        editor.select_only(child).unwrap();
        editor.add_to_selection(grandchild).unwrap();

        editor
            .dispatch(Command::DeleteSubtree { target: subtree })
            .unwrap();
        assert!(editor.document().node(subtree).is_none());
        assert!(editor.selection().is_empty());
        assert_eq!(
            editor.document().node(root).unwrap().children(),
            &[before, after]
        );

        assert!(editor.undo().unwrap().is_some());
        assert_eq!(editor.document().snapshot(), complete_before);
        assert_eq!(
            editor.document().node(root).unwrap().children(),
            &[before, subtree, after]
        );
        assert_eq!(
            editor.document().node(subtree).unwrap().children(),
            &[child, grandchild]
        );
        assert!(editor.selection().is_empty());
        assert!(editor.redo().unwrap().is_some());
        assert!(editor.document().node(subtree).is_none());
    }

    #[test]
    fn world_preserving_reparent_is_exactly_undoable_and_redoable() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let first = create_group(&mut editor, root, "First");
        let second = create_group(&mut editor, root, "Second");
        let child = create_rectangle(&mut editor, first, "Child");
        editor
            .dispatch(Command::SetLocalTransform {
                target: first,
                transform: Affine2::translation(Vec2::new(30.0, 10.0))
                    * Affine2::rotation(0.4)
                    * Affine2::scale(Vec2::new(2.0, 0.75)),
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocalTransform {
                target: second,
                transform: Affine2::translation(Vec2::new(-20.0, 50.0))
                    * Affine2::rotation(-0.2)
                    * Affine2::scale(Vec2::new(0.5, 1.5)),
            })
            .unwrap();
        let snapshot_before = editor.document().snapshot();
        let world_before = editor.document().world_transform(child).unwrap();
        editor
            .dispatch(Command::ReparentPreservingWorld {
                child,
                new_parent: second,
                index: 0,
            })
            .unwrap();
        let snapshot_after = editor.document().snapshot();
        assert!(editor
            .document()
            .world_transform(child)
            .unwrap()
            .approx_eq(world_before, DEFAULT_EPSILON * 64.0));
        editor.undo().unwrap();
        assert_eq!(editor.document().snapshot(), snapshot_before);
        editor.redo().unwrap();
        assert_eq!(editor.document().snapshot(), snapshot_after);
    }
    #[test]
    fn one_hundred_twenty_eight_drag_updates_commit_as_one_history_step() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let node = create_rectangle(&mut editor, root, "Dragged");
        let depth_before = editor.history_state().undo_depth;
        editor.begin_transaction().unwrap();
        for step in 1..=128 {
            editor
                .update_transaction(Command::SetLocalTransform {
                    target: node,
                    transform: Affine2::translation(Vec2::new(f64::from(step), 0.0)),
                })
                .unwrap();
        }
        assert!(editor.commit_transaction().unwrap());
        assert_eq!(editor.history_state().undo_depth, depth_before + 1);
        assert_eq!(
            editor.document().node(node).unwrap().local_transform(),
            Affine2::translation(Vec2::new(128.0, 0.0))
        );
        editor.undo().unwrap();
        assert_eq!(
            editor.document().node(node).unwrap().local_transform(),
            Affine2::IDENTITY
        );
        editor.redo().unwrap();
        assert_eq!(
            editor.document().node(node).unwrap().local_transform(),
            Affine2::translation(Vec2::new(128.0, 0.0))
        );
    }

    #[test]
    fn transaction_rollback_restores_exact_semantic_state() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let node = create_rectangle(&mut editor, root, "Node");
        let before = editor.document().snapshot();
        let history = editor.history_state();
        editor.begin_transaction().unwrap();
        editor
            .update_transaction(Command::SetName {
                target: node,
                name: "Preview".into(),
            })
            .unwrap();
        editor
            .update_transaction(Command::SetLocalTransform {
                target: node,
                transform: Affine2::translation(Vec2::new(40.0, 50.0)),
            })
            .unwrap();
        editor.rollback_transaction().unwrap();
        assert_eq!(editor.document().snapshot(), before);
        assert_eq!(editor.history_state(), history);
        assert!(!editor.transaction_active());
    }

    #[test]
    fn failed_transaction_update_keeps_prior_preview_consistent() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let node = create_rectangle(&mut editor, root, "Node");
        let initial = editor.document().snapshot();
        editor.begin_transaction().unwrap();
        editor
            .update_transaction(Command::SetName {
                target: node,
                name: "Valid preview".into(),
            })
            .unwrap();
        let preview = editor.document().snapshot();
        let error = editor
            .update_transaction(Command::SetAppearance {
                target: node,
                appearance: Appearance {
                    opacity: f64::INFINITY,
                    ..Appearance::default()
                },
            })
            .unwrap_err();
        assert!(matches!(error,
            EditorError::Command(CommandError::Document(DocumentError::InvalidAppearance(id)))
                if id == node));
        assert_eq!(editor.document().snapshot(), preview);
        assert!(editor.transaction_active());
        editor.rollback_transaction().unwrap();
        assert_eq!(editor.document().snapshot(), initial);
    }

    #[test]
    fn empty_nested_and_history_transaction_policies_are_explicit() {
        let mut editor = HeadlessEditorCore::blank("Root");
        editor.begin_transaction().unwrap();
        assert_eq!(
            editor.begin_transaction(),
            Err(EditorError::TransactionAlreadyActive)
        );
        assert_eq!(
            editor.undo(),
            Err(EditorError::TransactionActive { operation: "undo" })
        );
        assert_eq!(
            editor.redo(),
            Err(EditorError::TransactionActive { operation: "redo" })
        );
        assert_eq!(
            editor.dispatch(Command::SetName {
                target: editor.document().root_id(),
                name: "No bypass".into(),
            }),
            Err(EditorError::TransactionActive {
                operation: "dispatch"
            })
        );
        assert!(!editor.commit_transaction().unwrap());
        assert_eq!(editor.history_state(), HistoryState::default());
        assert_eq!(
            editor.commit_transaction(),
            Err(EditorError::NoActiveTransaction)
        );
    }

    #[test]
    fn divergent_edit_after_undo_invalidates_redo_branch() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let node = create_rectangle(&mut editor, root, "Node");
        editor
            .dispatch(Command::SetName {
                target: node,
                name: "First branch".into(),
            })
            .unwrap();
        editor.undo().unwrap();
        assert!(editor.can_redo());
        editor
            .dispatch(Command::SetVisible {
                target: node,
                visible: false,
            })
            .unwrap();
        assert!(!editor.can_redo());
        assert_eq!(editor.history_state().redo_depth, 0);
    }

    #[test]
    fn all_command_effects_round_trip_initial_and_final_snapshots() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let initial = editor.document().snapshot();
        let root = editor.document().root_id();
        let detached = NodeId::new();
        let group = NodeId::new();
        let frame = NodeId::new();
        let rectangle = NodeId::new();
        let commands = vec![
            Command::RegisterNode {
                spec: NodeSpec::group(detached, "Detached"),
            },
            Command::Attach {
                child: detached,
                parent: root,
                index: 0,
            },
            Command::CreateNode {
                spec: NodeSpec::group(group, "Group"),
                parent: root,
                index: 1,
            },
            Command::CreateNode {
                spec: NodeSpec::frame(frame, "Frame", Vec2::new(100.0, 80.0)),
                parent: root,
                index: 2,
            },
            Command::CreateNode {
                spec: NodeSpec::rectangle(rectangle, "Rectangle", Vec2::new(10.0, 20.0)),
                parent: group,
                index: 0,
            },
            Command::SetLocalTransform {
                target: rectangle,
                transform: Affine2::translation(Vec2::new(4.0, 5.0)),
            },
            Command::SetName {
                target: rectangle,
                name: "Renamed".into(),
            },
            Command::SetVisible {
                target: rectangle,
                visible: false,
            },
            Command::SetGeometry {
                target: rectangle,
                geometry: Geometry::Rectangle {
                    size: Vec2::new(30.0, 40.0),
                },
            },
            Command::SetAppearance {
                target: rectangle,
                appearance: Appearance {
                    opacity: 0.25,
                    ..Appearance::default()
                },
            },
            Command::SetMetadata {
                target: rectangle,
                metadata: BTreeMap::from([("semantic".into(), "test".into())]),
            },
            Command::ReparentPreservingWorld {
                child: rectangle,
                new_parent: frame,
                index: 0,
            },
            Command::Detach { child: detached },
            Command::Attach {
                child: detached,
                parent: group,
                index: 0,
            },
            Command::SetLocked {
                target: rectangle,
                locked: true,
            },
            Command::SetLocked {
                target: rectangle,
                locked: false,
            },
            Command::DeleteSubtree { target: group },
        ];
        for command in commands {
            editor.dispatch(command).unwrap();
        }
        let final_snapshot = editor.document().snapshot();
        while editor.can_undo() {
            assert!(editor.undo().unwrap().is_some());
        }
        assert_eq!(editor.document().snapshot(), initial);
        while editor.can_redo() {
            assert!(editor.redo().unwrap().is_some());
        }
        assert_eq!(editor.document().snapshot(), final_snapshot);
        assert_eq!(editor.document().node(rectangle).unwrap().id(), rectangle);
    }

    proptest! {
        #[test]
        fn randomized_property_command_sequences_undo_and_redo_exactly(
            operations in prop::collection::vec((0_u8..4, -1_000_i32..1_000_i32), 1..80)
        ) {
            let mut editor = HeadlessEditorCore::blank("Root");
            let initial = editor.document().snapshot();
            let root = editor.document().root_id();
            let node = NodeId::new();
            editor.dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(node, "Property target", Vec2::new(10.0, 10.0)),
                parent: root, index: 0,
            }).unwrap();
            for (kind, value) in operations {
                let command = match kind {
                    0 => Command::SetLocalTransform {
                        target: node,
                        transform: Affine2::translation(Vec2::new(f64::from(value), f64::from(-value))),
                    },
                    1 => Command::SetName { target: node, name: format!("Name {value}") },
                    2 => Command::SetVisible { target: node, visible: value % 2 == 0 },
                    _ => Command::SetAppearance {
                        target: node,
                        appearance: Appearance {
                            opacity: f64::from(value.unsigned_abs() % 1_001) / 1_000.0,
                            ..Appearance::default()
                        },
                    },
                };
                editor.dispatch(command).unwrap();
            }
            let final_snapshot = editor.document().snapshot();
            while editor.can_undo() { editor.undo().unwrap(); }
            prop_assert_eq!(editor.document().snapshot(), initial);
            while editor.can_redo() { editor.redo().unwrap(); }
            prop_assert_eq!(editor.document().snapshot(), final_snapshot);
        }
    }
    #[test]
    fn selection_order_toggle_primary_and_missing_id_policy_are_explicit() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let first = create_group(&mut editor, root, "First");
        let second = create_group(&mut editor, root, "Second");
        let third = create_group(&mut editor, root, "Third");
        editor.select_only(first).unwrap();
        editor.add_to_selection(second).unwrap();
        editor.add_to_selection(third).unwrap();
        assert_eq!(editor.selection().ordered(), &[first, second, third]);
        assert_eq!(editor.selection().primary(), Some(third));
        assert!(!editor.toggle_selection(second).unwrap());
        assert_eq!(editor.selection().ordered(), &[first, third]);
        assert!(editor.toggle_selection(second).unwrap());
        assert_eq!(editor.selection().ordered(), &[first, third, second]);
        assert_eq!(editor.selection().primary(), Some(second));
        assert!(editor.remove_from_selection(second));
        assert_eq!(editor.selection().primary(), Some(third));
        let missing = NodeId::new();
        assert_eq!(
            editor.add_to_selection(missing),
            Err(SelectionError::NodeNotFound(missing))
        );
        editor.clear_selection();
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn locked_nodes_are_selectable_but_edits_follow_explicit_ancestor_policy() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let parent = create_group(&mut editor, root, "Parent");
        let child = create_rectangle(&mut editor, parent, "Child");
        editor
            .dispatch(Command::SetLocked {
                target: parent,
                locked: true,
            })
            .unwrap();
        editor.select_only(child).unwrap();
        assert_eq!(editor.selection().primary(), Some(child));
        assert_eq!(
            assert_atomic_error(
                &mut editor,
                Command::SetName {
                    target: child,
                    name: "Blocked".into(),
                }
            ),
            CommandError::LockedNode {
                node: child,
                locked_by: parent
            }
        );
        assert_eq!(
            assert_atomic_error(&mut editor, Command::DeleteSubtree { target: parent }),
            CommandError::LockedNode {
                node: parent,
                locked_by: parent
            }
        );
        editor
            .dispatch(Command::SetLocked {
                target: parent,
                locked: false,
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocked {
                target: child,
                locked: true,
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocked {
                target: child,
                locked: false,
            })
            .unwrap();
        editor
            .dispatch(Command::SetName {
                target: child,
                name: "Allowed".into(),
            })
            .unwrap();
    }

    #[test]
    fn locked_descendant_prevents_ancestor_subtree_deletion() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let parent = create_group(&mut editor, root, "Parent");
        let child = create_group(&mut editor, parent, "Child");
        editor
            .dispatch(Command::SetLocked {
                target: child,
                locked: true,
            })
            .unwrap();
        assert_eq!(
            assert_atomic_error(&mut editor, Command::DeleteSubtree { target: parent }),
            CommandError::LockedNode {
                node: child,
                locked_by: child
            }
        );
    }

    #[test]
    fn document_replacement_clears_history_and_sanitizes_selection_atomically() {
        let shared = NodeId::new();
        let mut editor = HeadlessEditorCore::blank("Original");
        let original_root = editor.document().root_id();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::group(shared, "Original shared"),
                parent: original_root,
                index: 0,
            })
            .unwrap();
        editor.select_only(shared).unwrap();
        assert!(editor.can_undo());

        let mut replacement_editor = HeadlessEditorCore::blank("Replacement");
        let replacement_root = replacement_editor.document().root_id();
        replacement_editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::group(shared, "Replacement shared"),
                parent: replacement_root,
                index: 0,
            })
            .unwrap();
        editor
            .replace_document(replacement_editor.into_document().unwrap())
            .unwrap();
        assert_eq!(editor.history_state(), HistoryState::default());
        assert_eq!(editor.selection().ordered(), &[shared]);
        assert_eq!(
            editor.document().node(shared).unwrap().name(),
            "Replacement shared"
        );
        editor
            .replace_document(Document::new("No shared ID"))
            .unwrap();
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn invalid_replacement_and_replacement_during_transaction_preserve_document() {
        let mut editor = HeadlessEditorCore::blank("Original");
        let before = editor.document().snapshot();
        let mut invalid = Document::new("Invalid");
        let invalid_root = invalid.root_id();
        invalid.nodes.remove(&invalid_root);
        assert!(matches!(
            editor.replace_document(invalid),
            Err(EditorError::InvalidReplacement(
                InvariantViolation::MissingRoot(_)
            ))
        ));
        assert_eq!(editor.document().snapshot(), before);
        editor.begin_transaction().unwrap();
        assert_eq!(
            editor.replace_document(Document::new("Blocked")),
            Err(EditorError::TransactionActive {
                operation: "replace_document"
            })
        );
        assert_eq!(editor.document().snapshot(), before);
        editor.rollback_transaction().unwrap();
    }

    #[test]
    fn non_finite_and_extreme_numeric_commands_never_partially_mutate_or_panic() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let parent = create_group(&mut editor, root, "Parent");
        let child = create_rectangle(&mut editor, parent, "Child");
        let before = editor.document().snapshot();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            editor.dispatch(Command::SetLocalTransform {
                target: child,
                transform: Affine2::from_components(f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0),
            })
        }));
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(),
            Err(EditorError::Command(CommandError::Document(DocumentError::InvalidTransform(id))))
                if id == child));
        assert_eq!(editor.document().snapshot(), before);

        editor
            .dispatch(Command::SetLocalTransform {
                target: parent,
                transform: Affine2::scale(Vec2::new(f64::MAX, f64::MAX)),
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocalTransform {
                target: child,
                transform: Affine2::scale(Vec2::new(2.0, 2.0)),
            })
            .unwrap();
        assert_eq!(
            editor.document().world_transform(child),
            Err(DocumentError::NonFiniteDerivedTransform(child))
        );
        assert!(editor
            .document()
            .node(parent)
            .unwrap()
            .local_transform()
            .is_finite());
        assert!(editor
            .document()
            .node(child)
            .unwrap()
            .local_transform()
            .is_finite());
    }
    #[test]
    fn same_parent_reorder_undo_and_redo_preserve_exact_child_order() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let first = create_group(&mut editor, root, "First");
        let second = create_group(&mut editor, root, "Second");
        let third = create_group(&mut editor, root, "Third");
        let before = editor.document().snapshot();

        editor
            .dispatch(Command::Reparent {
                child: first,
                new_parent: root,
                index: 2,
            })
            .unwrap();
        let after = editor.document().snapshot();
        assert_eq!(
            editor.document().node(root).unwrap().children(),
            &[second, third, first]
        );
        editor.undo().unwrap();
        assert_eq!(editor.document().snapshot(), before);
        editor.redo().unwrap();
        assert_eq!(editor.document().snapshot(), after);
    }
    #[test]
    fn locked_parent_blocks_create_and_reparent_commands_atomically() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let locked_parent = create_group(&mut editor, root, "Locked parent");
        let movable = create_group(&mut editor, root, "Movable");
        editor
            .dispatch(Command::SetLocked {
                target: locked_parent,
                locked: true,
            })
            .unwrap();

        assert_eq!(
            assert_atomic_error(
                &mut editor,
                Command::CreateNode {
                    spec: NodeSpec::group(NodeId::new(), "Blocked child"),
                    parent: locked_parent,
                    index: 0,
                },
            ),
            CommandError::LockedNode {
                node: locked_parent,
                locked_by: locked_parent,
            }
        );
        assert_eq!(
            assert_atomic_error(
                &mut editor,
                Command::Reparent {
                    child: movable,
                    new_parent: locked_parent,
                    index: 0,
                },
            ),
            CommandError::LockedNode {
                node: locked_parent,
                locked_by: locked_parent,
            }
        );
    }

    #[test]
    fn failed_world_preserving_command_with_extreme_finite_values_is_atomic() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let new_parent = create_group(&mut editor, root, "New parent");
        let child = create_rectangle(&mut editor, root, "Child");
        editor
            .dispatch(Command::SetLocalTransform {
                target: new_parent,
                transform: Affine2::scale(Vec2::new(1.0e-150, 1.0e-150)),
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocalTransform {
                target: child,
                transform: Affine2::translation(Vec2::new(f64::MAX, 0.0)),
            })
            .unwrap();

        assert_eq!(
            assert_atomic_error(
                &mut editor,
                Command::ReparentPreservingWorld {
                    child,
                    new_parent,
                    index: 0,
                },
            ),
            CommandError::Document(DocumentError::NonFiniteDerivedTransform(child))
        );
    }

    #[test]
    fn revision_overflow_is_typed_and_atomic_across_all_mutation_paths() {
        let exhausted = EditorError::RevisionExhausted { revision: u64::MAX };

        let mut dispatch_editor = HeadlessEditorCore::blank("dispatch");
        let dispatch_root = dispatch_editor.document().root_id();
        dispatch_editor.set_revision_for_test(u64::MAX);
        let dispatch_document = dispatch_editor.document().snapshot();
        let dispatch_history = dispatch_editor.history_state();
        assert_eq!(
            dispatch_editor.dispatch(Command::CreateNode {
                spec: NodeSpec::group(NodeId::new(), "blocked"),
                parent: dispatch_root,
                index: 0,
            }),
            Err(exhausted.clone())
        );
        assert_eq!(dispatch_editor.document().snapshot(), dispatch_document);
        assert_eq!(dispatch_editor.history_state(), dispatch_history);
        assert_eq!(dispatch_editor.revision(), u64::MAX);

        let mut transaction_editor = HeadlessEditorCore::blank("transaction");
        let transaction_root = transaction_editor.document().root_id();
        let transaction_child = create_group(
            &mut transaction_editor,
            transaction_root,
            "transaction child",
        );
        transaction_editor.begin_transaction().unwrap();
        transaction_editor.set_revision_for_test(u64::MAX - 1);
        let preview = transaction_editor
            .update_transaction(Command::SetName {
                target: transaction_child,
                name: "preview".to_owned(),
            })
            .unwrap();
        assert_eq!(preview.change_set().revision().after, u64::MAX);
        let preview_document = transaction_editor.document().snapshot();
        let preview_history = transaction_editor.history_state();
        assert_eq!(
            transaction_editor.rollback_transaction(),
            Err(exhausted.clone())
        );
        assert_eq!(transaction_editor.document().snapshot(), preview_document);
        assert_eq!(transaction_editor.history_state(), preview_history);
        assert!(transaction_editor.transaction_active());
        assert_eq!(transaction_editor.revision(), u64::MAX);

        let mut update_editor = HeadlessEditorCore::blank("update");
        let update_root = update_editor.document().root_id();
        let update_child = create_group(&mut update_editor, update_root, "update child");
        update_editor.begin_transaction().unwrap();
        update_editor.set_revision_for_test(u64::MAX);
        let update_document = update_editor.document().snapshot();
        assert_eq!(
            update_editor.update_transaction(Command::SetName {
                target: update_child,
                name: "blocked".to_owned(),
            }),
            Err(exhausted.clone())
        );
        assert_eq!(update_editor.document().snapshot(), update_document);
        assert!(update_editor.transaction_active());

        let mut undo_editor = HeadlessEditorCore::blank("undo");
        let undo_root = undo_editor.document().root_id();
        create_group(&mut undo_editor, undo_root, "undo child");
        undo_editor.set_revision_for_test(u64::MAX);
        let undo_document = undo_editor.document().snapshot();
        let undo_history = undo_editor.history_state();
        assert_eq!(undo_editor.undo(), Err(exhausted.clone()));
        assert_eq!(undo_editor.document().snapshot(), undo_document);
        assert_eq!(undo_editor.history_state(), undo_history);

        let mut redo_editor = HeadlessEditorCore::blank("redo");
        let redo_root = redo_editor.document().root_id();
        create_group(&mut redo_editor, redo_root, "redo child");
        redo_editor.undo().unwrap();
        redo_editor.set_revision_for_test(u64::MAX);
        let redo_document = redo_editor.document().snapshot();
        let redo_history = redo_editor.history_state();
        assert_eq!(redo_editor.redo(), Err(exhausted.clone()));
        assert_eq!(redo_editor.document().snapshot(), redo_document);
        assert_eq!(redo_editor.history_state(), redo_history);

        let mut replace_editor = HeadlessEditorCore::blank("replace");
        replace_editor.set_revision_for_test(u64::MAX);
        let replace_document = replace_editor.document().snapshot();
        assert_eq!(
            replace_editor.replace_document(Document::new("blocked replacement")),
            Err(exhausted)
        );
        assert_eq!(replace_editor.document().snapshot(), replace_document);
        assert_eq!(replace_editor.revision(), u64::MAX);
    }
}
