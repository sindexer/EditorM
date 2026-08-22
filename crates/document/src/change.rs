use thiserror::Error;

use crate::NodeId;

/// Persistent hierarchy placement before or after a semantic mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodePlacement {
    pub parent: NodeId,
    pub index: usize,
}

/// A persistent property that currently has no computed-scene consequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentProperty {
    Name,
    Locked,
    Metadata,
}

/// Direction of one bounded Group/Ungroup hierarchy patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructuralGroupChange {
    Grouped,
    Ungrouped,
}

/// Public semantic mutation description. It never exposes private history effects or a
/// mutable document capability.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentChange {
    NodesInserted {
        root: NodeId,
        nodes: Vec<NodeId>,
        placement: Option<NodePlacement>,
    },
    NodesRemoved {
        root: NodeId,
        nodes: Vec<NodeId>,
        placement: Option<NodePlacement>,
    },
    PlacementChanged {
        root: NodeId,
        subtree: Vec<NodeId>,
        before: Option<NodePlacement>,
        after: Option<NodePlacement>,
        local_transform_changed: bool,
    },
    /// One atomic structural operation. Consumers must apply this as a unit instead of
    /// replaying transient insert/move/remove states.
    StructuralGroupChanged {
        direction: StructuralGroupChange,
        group: NodeId,
        parent: NodeId,
        index: usize,
        children: Vec<NodeId>,
        positions: Vec<usize>,
    },
    LocalTransformChanged {
        node: NodeId,
    },
    GeometryChanged {
        node: NodeId,
    },
    VisibilityChanged {
        node: NodeId,
    },
    AppearanceChanged {
        node: NodeId,
        bounds_changed: bool,
    },
    PersistentPropertyChanged {
        node: NodeId,
        property: PersistentProperty,
    },
    FullDocumentReset,
}

/// Revisioned semantic changes produced by every persistent mutation path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRevision {
    pub before: u64,
    pub after: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentChangeSet {
    revision: DocumentRevision,
    changes: Vec<DocumentChange>,
}

impl DocumentChangeSet {
    #[must_use]
    pub fn unchanged(revision: u64) -> Self {
        Self {
            revision: DocumentRevision {
                before: revision,
                after: revision,
            },
            changes: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn changed(before: u64, after: u64, changes: Vec<DocumentChange>) -> Self {
        debug_assert!(after > before);
        Self {
            revision: DocumentRevision { before, after },
            changes,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> &DocumentRevision {
        &self.revision
    }

    #[must_use]
    pub fn changes(&self) -> &[DocumentChange] {
        &self.changes
    }

    #[must_use]
    pub fn changed_document(&self) -> bool {
        self.revision.before != self.revision.after
    }

    /// Merges consecutive change sets produced by one batched edit into a single set.
    ///
    /// Callers that apply several typed commands inside one transaction still owe consumers a
    /// single revisioned description of what changed. Merging is rejected when the parts do not
    /// form one contiguous revision chain, so a caller can never present unrelated or reordered
    /// revisions as one atomic batch.
    pub fn merge_consecutive(parts: &[Self], base_revision: u64) -> Result<Self, ChangeMergeError> {
        let mut current = base_revision;
        let mut changes = Vec::new();
        for part in parts {
            if part.revision.before != current {
                return Err(ChangeMergeError::NonConsecutive {
                    expected: current,
                    found: part.revision.before,
                });
            }
            current = part.revision.after;
            changes.extend(part.changes.iter().cloned());
        }
        if current == base_revision {
            return Ok(Self::unchanged(base_revision));
        }
        Ok(Self {
            revision: DocumentRevision {
                before: base_revision,
                after: current,
            },
            changes,
        })
    }
}

/// Failure of an attempted [`DocumentChangeSet`] merge.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ChangeMergeError {
    #[error("change set starts at revision {found} but revision {expected} was expected")]
    NonConsecutive { expected: u64, found: u64 },
}
