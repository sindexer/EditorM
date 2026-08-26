use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};

use thiserror::Error;
use visual_authoring_core_math::{Affine2, Vec2};

use crate::{
    Appearance, Document, DocumentChange, DocumentChangeSet, DocumentError, Geometry,
    GroupRestoration, GroupRestorationRun, Metadata, NodeId, NodeKind, NodePlacement, NodeSnapshot,
    NodeSpec, PathAnchor, PathAnchorId, PathGeometry, PersistentProperty, Stroke,
    StructuralGroupChange,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathHandle {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathSegmentKind {
    Straight,
    Cubic,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum PathEditError {
    #[error("path anchor {anchor} does not exist in node {target}")]
    MissingAnchor {
        target: NodeId,
        anchor: PathAnchorId,
    },
    #[error("path anchor {anchor} already exists in node {target}")]
    DuplicateAnchor {
        target: NodeId,
        anchor: PathAnchorId,
    },
    #[error("anchors {start} and {end} do not form a directed segment in node {target}")]
    NonAdjacentSegment {
        target: NodeId,
        start: PathAnchorId,
        end: PathAnchorId,
    },
    #[error("path split parameter must be finite and strictly inside (0, 1)")]
    InvalidSplitParameter,
    #[error("deleting anchor {anchor} would leave too few anchors in node {target}")]
    TooFewAnchors {
        target: NodeId,
        anchor: PathAnchorId,
    },
    #[error("closing path node {target} requires at least three anchors; found {actual}")]
    ClosingRequiresThree { target: NodeId, actual: usize },
}

/// Explicit, deterministic persistent-edit request.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    RegisterNode {
        spec: NodeSpec,
    },
    CreateNode {
        spec: NodeSpec,
        parent: NodeId,
        index: usize,
    },
    Attach {
        child: NodeId,
        parent: NodeId,
        index: usize,
    },
    Detach {
        child: NodeId,
    },
    DeleteSubtree {
        target: NodeId,
    },
    Reparent {
        child: NodeId,
        new_parent: NodeId,
        index: usize,
    },
    ReparentPreservingWorld {
        child: NodeId,
        new_parent: NodeId,
        index: usize,
    },
    Group {
        group: NodeSpec,
        targets: Vec<NodeId>,
    },
    Ungroup {
        target: NodeId,
    },
    SetLocalTransform {
        target: NodeId,
        transform: Affine2,
    },
    SetName {
        target: NodeId,
        name: String,
    },
    SetVisible {
        target: NodeId,
        visible: bool,
    },
    SetLocked {
        target: NodeId,
        locked: bool,
    },
    SetGeometry {
        target: NodeId,
        geometry: Geometry,
    },
    MovePathAnchor {
        target: NodeId,
        anchor: PathAnchorId,
        position: Vec2,
    },
    SetPathHandle {
        target: NodeId,
        anchor: PathAnchorId,
        handle: PathHandle,
        position: Option<Vec2>,
    },
    InsertPathAnchor {
        target: NodeId,
        start: PathAnchorId,
        end: PathAnchorId,
        anchor: PathAnchorId,
        parameter: f64,
    },
    DeletePathAnchor {
        target: NodeId,
        anchor: PathAnchorId,
    },
    SetPathClosed {
        target: NodeId,
        closed: bool,
    },
    SetPathSegmentKind {
        target: NodeId,
        start: PathAnchorId,
        end: PathAnchorId,
        kind: PathSegmentKind,
    },
    SetStroke {
        target: NodeId,
        stroke: Stroke,
    },
    SetAppearance {
        target: NodeId,
        appearance: Appearance,
    },
    SetMetadata {
        target: NodeId,
        metadata: Metadata,
    },
}

impl Command {
    #[must_use]
    pub const fn operation_name(&self) -> &'static str {
        match self {
            Self::RegisterNode { .. } => "register_node",
            Self::CreateNode { .. } => "create_node",
            Self::Attach { .. } => "attach",
            Self::Detach { .. } => "detach",
            Self::DeleteSubtree { .. } => "delete_subtree",
            Self::Reparent { .. } => "reparent",
            Self::ReparentPreservingWorld { .. } => "reparent_preserving_world",
            Self::Group { .. } => "group",
            Self::Ungroup { .. } => "ungroup",
            Self::SetLocalTransform { .. } => "set_local_transform",
            Self::SetName { .. } => "set_name",
            Self::SetVisible { .. } => "set_visible",
            Self::SetLocked { .. } => "set_locked",
            Self::SetGeometry { .. } => "set_geometry",
            Self::MovePathAnchor { .. } => "move_path_anchor",
            Self::SetPathHandle { .. } => "set_path_handle",
            Self::InsertPathAnchor { .. } => "insert_path_anchor",
            Self::DeletePathAnchor { .. } => "delete_path_anchor",
            Self::SetPathClosed { .. } => "set_path_closed",
            Self::SetPathSegmentKind { .. } => "set_path_segment_kind",
            Self::SetStroke { .. } => "set_stroke",
            Self::SetAppearance { .. } => "set_appearance",
            Self::SetMetadata { .. } => "set_metadata",
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum CommandError {
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("node {node} cannot be edited because node {locked_by} is locked")]
    LockedNode { node: NodeId, locked_by: NodeId },
    #[error("operation {operation} is unsupported for {kind:?} node {target}")]
    UnsupportedOperation {
        operation: &'static str,
        target: NodeId,
        kind: NodeKind,
    },
    #[error("invalid group operation: {0}")]
    InvalidGroup(&'static str),
    #[error(transparent)]
    PathEdit(#[from] PathEditError),
}

/// Observable result of one command application.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandOutcome {
    changed: bool,
    affected: Vec<NodeId>,
    change_set: DocumentChangeSet,
    sequence_work: crate::SequenceWork,
}

impl CommandOutcome {
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    #[must_use]
    pub fn affected(&self) -> &[NodeId] {
        &self.affected
    }

    #[must_use]
    pub const fn change_set(&self) -> &DocumentChangeSet {
        &self.change_set
    }

    pub(crate) fn set_change_set(&mut self, change_set: DocumentChangeSet) {
        self.change_set = change_set;
    }

    #[must_use]
    pub const fn sequence_work(&self) -> crate::SequenceWork {
        self.sequence_work
    }

    pub(crate) fn set_sequence_work(&mut self, sequence_work: crate::SequenceWork) {
        self.sequence_work = sequence_work;
    }

    fn unchanged(affected: Vec<NodeId>) -> Self {
        Self {
            changed: false,
            affected,
            change_set: DocumentChangeSet::unchanged(0),
            sequence_work: crate::SequenceWork::default(),
        }
    }

    fn with_change(affected: Vec<NodeId>) -> Self {
        Self {
            changed: true,
            affected,
            change_set: DocumentChangeSet::unchanged(0),
            sequence_work: crate::SequenceWork::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Placement {
    pub(crate) parent: NodeId,
    pub(crate) index: usize,
}

impl Placement {
    const fn public(self) -> NodePlacement {
        NodePlacement {
            parent: self.parent,
            index: self.index,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SubtreeRecord {
    nodes: Vec<NodeSnapshot>,
    placement: Option<Placement>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReversibleEffect {
    StructuralGroup {
        grouped_forward: bool,
        group: NodeSpec,
        parent: NodeId,
        index: usize,
        children: Vec<(NodeId, usize, Affine2, Affine2)>,
        restoration: GroupRestoration,
    },
    InsertNode {
        spec: NodeSpec,
        placement: Option<Placement>,
    },
    DeleteSubtree {
        record: SubtreeRecord,
    },
    MoveNode {
        target: NodeId,
        subtree: Vec<NodeId>,
        before: Option<Placement>,
        after: Option<Placement>,
        before_transform: Affine2,
        after_transform: Affine2,
    },
    SetLocalTransform {
        target: NodeId,
        before: Affine2,
        after: Affine2,
    },
    SetName {
        target: NodeId,
        before: String,
        after: String,
    },
    SetVisible {
        target: NodeId,
        before: bool,
        after: bool,
    },
    SetLocked {
        target: NodeId,
        before: bool,
        after: bool,
    },
    SetGeometry {
        target: NodeId,
        before: Geometry,
        after: Geometry,
    },
    SetAppearance {
        target: NodeId,
        before: Box<Appearance>,
        after: Box<Appearance>,
    },
    SetMetadata {
        target: NodeId,
        before: Metadata,
        after: Metadata,
    },
}

impl ReversibleEffect {
    pub(crate) fn forward_changes(&self) -> Vec<DocumentChange> {
        vec![self.semantic_change(true)]
    }

    pub(crate) fn backward_changes(&self) -> Vec<DocumentChange> {
        vec![self.semantic_change(false)]
    }

    fn semantic_change(&self, forward: bool) -> DocumentChange {
        match self {
            Self::StructuralGroup {
                grouped_forward,
                group,
                parent,
                index,
                children,
                ..
            } => {
                let grouped = if forward {
                    *grouped_forward
                } else {
                    !*grouped_forward
                };
                let mut placements = children
                    .iter()
                    .map(|(id, index, _, _)| (*id, *index))
                    .collect::<Vec<_>>();
                if !grouped {
                    placements.sort_by_key(|(_, index)| *index);
                }
                DocumentChange::StructuralGroupChanged {
                    direction: if grouped {
                        StructuralGroupChange::Grouped
                    } else {
                        StructuralGroupChange::Ungrouped
                    },
                    group: group.id,
                    parent: *parent,
                    index: *index,
                    children: placements.iter().map(|(id, _)| *id).collect(),
                    positions: placements.iter().map(|(_, index)| *index).collect(),
                }
            }
            Self::InsertNode { spec, placement } => {
                let placement = placement.map(Placement::public);
                if forward {
                    DocumentChange::NodesInserted {
                        root: spec.id,
                        nodes: vec![spec.id],
                        placement,
                    }
                } else {
                    DocumentChange::NodesRemoved {
                        root: spec.id,
                        nodes: vec![spec.id],
                        placement,
                    }
                }
            }
            Self::DeleteSubtree { record } => {
                let root = record.root_id();
                let nodes = record.node_ids();
                let placement = record.placement.map(Placement::public);
                if forward {
                    DocumentChange::NodesRemoved {
                        root,
                        nodes,
                        placement,
                    }
                } else {
                    DocumentChange::NodesInserted {
                        root,
                        nodes,
                        placement,
                    }
                }
            }
            Self::MoveNode {
                target,
                subtree,
                before,
                after,
                before_transform,
                after_transform,
            } => DocumentChange::PlacementChanged {
                root: *target,
                subtree: subtree.clone(),
                before: if forward { *before } else { *after }.map(Placement::public),
                after: if forward { *after } else { *before }.map(Placement::public),
                local_transform_changed: before_transform != after_transform,
            },
            Self::SetLocalTransform { target, .. } => {
                DocumentChange::LocalTransformChanged { node: *target }
            }
            Self::SetName { target, .. } => DocumentChange::PersistentPropertyChanged {
                node: *target,
                property: PersistentProperty::Name,
            },
            Self::SetVisible { target, .. } => DocumentChange::VisibilityChanged { node: *target },
            Self::SetLocked { target, .. } => DocumentChange::PersistentPropertyChanged {
                node: *target,
                property: PersistentProperty::Locked,
            },
            Self::SetGeometry { target, .. } => DocumentChange::GeometryChanged { node: *target },
            Self::SetAppearance {
                target,
                before,
                after,
            } => DocumentChange::AppearanceChanged {
                node: *target,
                bounds_changed: before.stroke != after.stroke,
            },
            Self::SetMetadata { target, .. } => DocumentChange::PersistentPropertyChanged {
                node: *target,
                property: PersistentProperty::Metadata,
            },
        }
    }

    pub(crate) fn apply_forward(&self, document: &mut Document) -> Result<(), CommandError> {
        match self {
            Self::StructuralGroup {
                grouped_forward,
                group,
                parent,
                index,
                children,
                restoration,
            } => {
                if *grouped_forward {
                    document.apply_group_patch(group, *parent, *index, children, restoration)?;
                } else {
                    document.apply_ungroup_patch(group, *parent, *index, children, restoration)?;
                }
            }
            Self::InsertNode { spec, placement } => {
                if let Some(placement) = placement {
                    document.create_node(spec.clone(), placement.parent, placement.index)?;
                } else {
                    document.register_node(spec.clone())?;
                }
            }
            Self::DeleteSubtree { record } => {
                document.delete_subtree(record.root_id())?;
            }
            Self::MoveNode {
                target,
                after,
                after_transform,
                ..
            } => apply_move(document, *target, *after, *after_transform)?,
            Self::SetLocalTransform { target, after, .. } => {
                document.set_local_transform(*target, *after)?;
            }
            Self::SetName { target, after, .. } => {
                document.set_name(*target, after.clone())?;
            }
            Self::SetVisible { target, after, .. } => {
                document.set_visible(*target, *after)?;
            }
            Self::SetLocked { target, after, .. } => {
                document.set_locked(*target, *after)?;
            }
            Self::SetGeometry { target, after, .. } => {
                document.set_geometry(*target, after.clone())?;
            }
            Self::SetAppearance { target, after, .. } => {
                document.set_appearance(*target, **after)?;
            }
            Self::SetMetadata { target, after, .. } => {
                document.set_metadata(*target, after.clone())?;
            }
        }
        Ok(())
    }

    pub(crate) fn apply_backward(&self, document: &mut Document) -> Result<(), CommandError> {
        match self {
            Self::StructuralGroup {
                grouped_forward,
                group,
                parent,
                index,
                children,
                restoration,
            } => {
                if *grouped_forward {
                    document.apply_ungroup_patch(group, *parent, *index, children, restoration)?;
                } else {
                    document.apply_group_patch(group, *parent, *index, children, restoration)?;
                }
            }
            Self::InsertNode { spec, .. } => {
                document.delete_subtree(spec.id)?;
            }
            Self::DeleteSubtree { record } => {
                document.restore_subtree(
                    &record.nodes,
                    record.placement.map(|place| (place.parent, place.index)),
                )?;
            }
            Self::MoveNode {
                target,
                before,
                before_transform,
                ..
            } => apply_move(document, *target, *before, *before_transform)?,
            Self::SetLocalTransform { target, before, .. } => {
                document.set_local_transform(*target, *before)?;
            }
            Self::SetName { target, before, .. } => {
                document.set_name(*target, before.clone())?;
            }
            Self::SetVisible { target, before, .. } => {
                document.set_visible(*target, *before)?;
            }
            Self::SetLocked { target, before, .. } => {
                document.set_locked(*target, *before)?;
            }
            Self::SetGeometry { target, before, .. } => {
                document.set_geometry(*target, before.clone())?;
            }
            Self::SetAppearance { target, before, .. } => {
                document.set_appearance(*target, **before)?;
            }
            Self::SetMetadata { target, before, .. } => {
                document.set_metadata(*target, before.clone())?;
            }
        }
        Ok(())
    }

    pub(crate) fn try_coalesce(&mut self, newer: Self) -> Option<Self> {
        match (self, newer) {
            (
                Self::InsertNode { spec, .. },
                Self::SetGeometry {
                    target,
                    after: newer_after,
                    ..
                },
            ) if spec.id == target => spec.geometry = Some(newer_after),
            (
                Self::SetLocalTransform { target, after, .. },
                Self::SetLocalTransform {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetName { target, after, .. },
                Self::SetName {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetVisible { target, after, .. },
                Self::SetVisible {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetLocked { target, after, .. },
                Self::SetLocked {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetGeometry { target, after, .. },
                Self::SetGeometry {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetAppearance { target, after, .. },
                Self::SetAppearance {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (
                Self::SetMetadata { target, after, .. },
                Self::SetMetadata {
                    target: newer_target,
                    after: newer_after,
                    ..
                },
            ) if *target == newer_target => *after = newer_after,
            (_, not_coalesced) => return Some(not_coalesced),
        }
        None
    }

    pub(crate) fn is_noop(&self) -> bool {
        match self {
            Self::StructuralGroup { .. } | Self::InsertNode { .. } | Self::DeleteSubtree { .. } => {
                false
            }
            Self::MoveNode {
                before,
                after,
                before_transform,
                after_transform,
                ..
            } => before == after && before_transform == after_transform,
            Self::SetLocalTransform { before, after, .. } => before == after,
            Self::SetName { before, after, .. } => before == after,
            Self::SetVisible { before, after, .. } => before == after,
            Self::SetLocked { before, after, .. } => before == after,
            Self::SetGeometry { before, after, .. } => before == after,
            Self::SetAppearance { before, after, .. } => before == after,
            Self::SetMetadata { before, after, .. } => before == after,
        }
    }
}

impl SubtreeRecord {
    fn root_id(&self) -> NodeId {
        self.nodes[0].spec.id
    }

    fn node_ids(&self) -> Vec<NodeId> {
        self.nodes.iter().map(|node| node.spec.id).collect()
    }
}

pub(crate) struct Execution {
    pub(crate) outcome: CommandOutcome,
    pub(crate) effect: Option<ReversibleEffect>,
}

pub(crate) fn execute_command(
    document: &mut Document,
    command: Command,
) -> Result<Execution, CommandError> {
    match command {
        Command::RegisterNode { spec } => {
            let stored = spec.clone();
            let id = document.register_node(spec)?;
            changed(
                vec![id],
                ReversibleEffect::InsertNode {
                    spec: stored,
                    placement: None,
                },
            )
        }
        Command::CreateNode {
            spec,
            parent,
            index,
        } => {
            ensure_editable(document, parent)?;
            let stored = spec.clone();
            let id = document.create_node(spec, parent, index)?;
            changed(
                vec![id],
                ReversibleEffect::InsertNode {
                    spec: stored,
                    placement: Some(Placement { parent, index }),
                },
            )
        }
        Command::Attach {
            child,
            parent,
            index,
        } => {
            ensure_editable(document, child)?;
            ensure_editable(document, parent)?;
            let transform = document.required_node(child)?.local_transform();
            let subtree = document.collect_subtree_preorder(child)?;
            document.attach_child(parent, child, index)?;
            changed(
                vec![child],
                ReversibleEffect::MoveNode {
                    target: child,
                    subtree,
                    before: None,
                    after: Some(Placement { parent, index }),
                    before_transform: transform,
                    after_transform: transform,
                },
            )
        }
        Command::Detach { child } => {
            ensure_editable(document, child)?;
            let before = required_placement(document, child)?;
            let transform = document.required_node(child)?.local_transform();
            let subtree = document.collect_subtree_preorder(child)?;
            document.detach(child)?;
            changed(
                vec![child],
                ReversibleEffect::MoveNode {
                    target: child,
                    subtree,
                    before: Some(before),
                    after: None,
                    before_transform: transform,
                    after_transform: transform,
                },
            )
        }
        Command::DeleteSubtree { target } => {
            ensure_subtree_editable(document, target)?;
            let record = capture_subtree(document, target)?;
            let affected = record.nodes.iter().map(|node| node.spec.id).collect();
            document.delete_subtree(target)?;
            changed(affected, ReversibleEffect::DeleteSubtree { record })
        }
        Command::Reparent {
            child,
            new_parent,
            index,
        } => reparent(document, child, new_parent, index, false),
        Command::ReparentPreservingWorld {
            child,
            new_parent,
            index,
        } => reparent(document, child, new_parent, index, true),
        Command::Group { group, targets } => group_nodes(document, group, targets),
        Command::Ungroup { target } => ungroup_node(document, target),
        Command::SetLocalTransform { target, transform } => {
            ensure_editable(document, target)?;
            let before = document.required_node(target)?.local_transform();
            if before == transform {
                return Ok(unchanged(vec![target]));
            }
            document.set_local_transform(target, transform)?;
            changed(
                vec![target],
                ReversibleEffect::SetLocalTransform {
                    target,
                    before,
                    after: transform,
                },
            )
        }
        Command::SetName { target, name } => {
            ensure_editable(document, target)?;
            let before = document.required_node(target)?.name().to_owned();
            if before == name {
                return Ok(unchanged(vec![target]));
            }
            document.set_name(target, name.clone())?;
            changed(
                vec![target],
                ReversibleEffect::SetName {
                    target,
                    before,
                    after: name,
                },
            )
        }
        Command::SetVisible { target, visible } => {
            ensure_editable(document, target)?;
            let before = document.required_node(target)?.visible();
            if before == visible {
                return Ok(unchanged(vec![target]));
            }
            document.set_visible(target, visible)?;
            changed(
                vec![target],
                ReversibleEffect::SetVisible {
                    target,
                    before,
                    after: visible,
                },
            )
        }
        Command::SetLocked { target, locked } => {
            let before = document.required_node(target)?.locked();
            ensure_lock_change_allowed(document, target, before, locked)?;
            if before == locked {
                return Ok(unchanged(vec![target]));
            }
            document.set_locked(target, locked)?;
            changed(
                vec![target],
                ReversibleEffect::SetLocked {
                    target,
                    before,
                    after: locked,
                },
            )
        }
        Command::SetGeometry { target, geometry } => {
            ensure_editable(document, target)?;
            let node = document.required_node(target)?;
            let before = node
                .geometry()
                .cloned()
                .ok_or(CommandError::UnsupportedOperation {
                    operation: "set_geometry",
                    target,
                    kind: node.kind(),
                })?;
            if before == geometry {
                return Ok(unchanged(vec![target]));
            }
            document.set_geometry(target, geometry.clone())?;
            changed(
                vec![target],
                ReversibleEffect::SetGeometry {
                    target,
                    before,
                    after: geometry,
                },
            )
        }
        Command::MovePathAnchor {
            target,
            anchor,
            position,
        } => edit_path(document, target, "move_path_anchor", move |path| {
            let index = anchor_index(path, target, anchor)?;
            let delta = position - path.anchors[index].position;
            path.anchors[index].position = position;
            path.anchors[index].handle_in =
                path.anchors[index].handle_in.map(|handle| handle + delta);
            path.anchors[index].handle_out =
                path.anchors[index].handle_out.map(|handle| handle + delta);
            Ok(())
        }),
        Command::SetPathHandle {
            target,
            anchor,
            handle,
            position,
        } => edit_path(document, target, "set_path_handle", move |path| {
            let index = anchor_index(path, target, anchor)?;
            match handle {
                PathHandle::Incoming => path.anchors[index].handle_in = position,
                PathHandle::Outgoing => path.anchors[index].handle_out = position,
            }
            Ok(())
        }),
        Command::InsertPathAnchor {
            target,
            start,
            end,
            anchor,
            parameter,
        } => edit_path(document, target, "insert_path_anchor", move |path| {
            if !parameter.is_finite() || parameter <= 0.0 || parameter >= 1.0 {
                return Err(PathEditError::InvalidSplitParameter.into());
            }
            if path.anchors.iter().any(|candidate| candidate.id == anchor) {
                return Err(PathEditError::DuplicateAnchor { target, anchor }.into());
            }
            let (start_index, end_index) = segment_indices(path, target, start, end)?;
            let first = path.anchors[start_index].clone();
            let last = path.anchors[end_index].clone();
            let cubic = first.handle_out.is_some() || last.handle_in.is_some();
            let p0 = first.position;
            let p1 = first.handle_out.unwrap_or(p0);
            let p3 = last.position;
            let p2 = last.handle_in.unwrap_or(p3);
            let q0 = lerp(p0, p1, parameter);
            let q1 = lerp(p1, p2, parameter);
            let q2 = lerp(p2, p3, parameter);
            let r0 = lerp(q0, q1, parameter);
            let r1 = lerp(q1, q2, parameter);
            let split = lerp(r0, r1, parameter);
            if cubic {
                path.anchors[start_index].handle_out = Some(q0);
                path.anchors[end_index].handle_in = Some(q2);
            }
            let mut inserted = PathAnchor::new(anchor, split);
            if cubic {
                inserted.handle_in = Some(r0);
                inserted.handle_out = Some(r1);
            }
            let insertion_index = if end_index == 0 {
                path.anchors.len()
            } else {
                end_index
            };
            path.anchors.insert(insertion_index, inserted);
            Ok(())
        }),
        Command::DeletePathAnchor { target, anchor } => {
            edit_path(document, target, "delete_path_anchor", move |path| {
                let index = anchor_index(path, target, anchor)?;
                let minimum = if path.closed { 3 } else { 2 };
                if path.anchors.len() <= minimum {
                    return Err(PathEditError::TooFewAnchors { target, anchor }.into());
                }
                path.anchors.remove(index);
                Ok(())
            })
        }
        Command::SetPathClosed { target, closed } => {
            edit_path(document, target, "set_path_closed", move |path| {
                if closed && path.anchors.len() < 3 {
                    return Err(PathEditError::ClosingRequiresThree {
                        target,
                        actual: path.anchors.len(),
                    }
                    .into());
                }
                path.closed = closed;
                Ok(())
            })
        }
        Command::SetPathSegmentKind {
            target,
            start,
            end,
            kind,
        } => edit_path(document, target, "set_path_segment_kind", move |path| {
            let (start_index, end_index) = segment_indices(path, target, start, end)?;
            match kind {
                PathSegmentKind::Straight => {
                    path.anchors[start_index].handle_out = None;
                    path.anchors[end_index].handle_in = None;
                }
                PathSegmentKind::Cubic => {
                    let p0 = path.anchors[start_index].position;
                    let p3 = path.anchors[end_index].position;
                    if path.anchors[start_index].handle_out.is_none()
                        && path.anchors[end_index].handle_in.is_none()
                    {
                        path.anchors[start_index].handle_out = Some(lerp(p0, p3, 1.0 / 3.0));
                        path.anchors[end_index].handle_in = Some(lerp(p0, p3, 2.0 / 3.0));
                    }
                }
            }
            Ok(())
        }),
        Command::SetStroke { target, stroke } => {
            ensure_editable(document, target)?;
            let mut after = document.required_node(target)?.appearance();
            after.stroke = stroke;
            let before = document.required_node(target)?.appearance();
            if before == after {
                return Ok(unchanged(vec![target]));
            }
            document.set_appearance(target, after)?;
            changed(
                vec![target],
                ReversibleEffect::SetAppearance {
                    target,
                    before: Box::new(before),
                    after: Box::new(after),
                },
            )
        }
        Command::SetAppearance { target, appearance } => {
            ensure_editable(document, target)?;
            let before = document.required_node(target)?.appearance();
            if before == appearance {
                return Ok(unchanged(vec![target]));
            }
            document.set_appearance(target, appearance)?;
            changed(
                vec![target],
                ReversibleEffect::SetAppearance {
                    target,
                    before: Box::new(before),
                    after: Box::new(appearance),
                },
            )
        }
        Command::SetMetadata { target, metadata } => {
            ensure_editable(document, target)?;
            let before = document.required_node(target)?.metadata().clone();
            if before == metadata {
                return Ok(unchanged(vec![target]));
            }
            document.set_metadata(target, metadata.clone())?;
            changed(
                vec![target],
                ReversibleEffect::SetMetadata {
                    target,
                    before,
                    after: metadata,
                },
            )
        }
    }
}

fn group_nodes(
    document: &mut Document,
    group: NodeSpec,
    targets: Vec<NodeId>,
) -> Result<Execution, CommandError> {
    if group.kind != NodeKind::Group || group.geometry.is_some() {
        return Err(CommandError::InvalidGroup(
            "group specification must be a Group node",
        ));
    }
    if targets.len() < 2 {
        return Err(CommandError::InvalidGroup(
            "at least two nodes are required",
        ));
    }
    let unique = targets.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != targets.len() {
        return Err(CommandError::InvalidGroup("target nodes must be unique"));
    }
    if document.node(group.id).is_some() {
        return Err(DocumentError::DuplicateNodeId(group.id).into());
    }

    let parent = document
        .required_node(targets[0])?
        .parent()
        .ok_or(DocumentError::NodeNotAttached(targets[0]))?;
    ensure_editable(document, parent)?;
    for target in &targets {
        ensure_editable(document, *target)?;
        if document.required_node(*target)?.parent() != Some(parent) {
            return Err(CommandError::InvalidGroup(
                "target nodes must share one parent",
            ));
        }
    }

    let parent_children = document.required_node(parent)?.children();
    let mut planning_work = crate::SequenceWork::default();
    let mut placed = targets
        .iter()
        .map(|target| {
            parent_children
                .placement_of_tracked(*target, &mut planning_work)
                .map(|(rank, before, after)| (*target, rank, before, after))
                .ok_or(CommandError::InvalidGroup(
                    "target placement is inconsistent",
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sort_comparisons = Cell::new(0_u64);
    placed.sort_by(|left, right| {
        sort_comparisons.set(sort_comparisons.get() + 1);
        left.1.cmp(&right.1)
    });
    planning_work.rank_order_comparisons += sort_comparisons.get();
    let group_index = placed[0].1;

    let mut runs = Vec::new();
    let mut start = 0;
    while start < placed.len() {
        let mut end = start + 1;
        while end < placed.len() && placed[end].1 == placed[end - 1].1 + 1 {
            planning_work.entries_examined += 1;
            planning_work.rank_order_comparisons += 1;
            end += 1;
        }
        if end < placed.len() {
            planning_work.entries_examined += 1;
            planning_work.rank_order_comparisons += 1;
        }
        let before_anchor = placed[start].2;
        let after_anchor = placed[end - 1].3;
        let children = placed[start..end]
            .iter()
            .map(|(child, _, _, _)| *child)
            .collect::<Vec<_>>();
        planning_work.entries_examined += (end - start) as u64;
        planning_work.entries_copied += (end - start) as u64;
        runs.push(GroupRestorationRun {
            children,
            before_anchor,
            after_anchor,
        });
        start = end;
    }
    let restoration = GroupRestoration {
        version: GroupRestoration::VERSION,
        runs,
    };

    let parent_world = document.world_transform(parent)?;
    let group_world = parent_world * group.local_transform;
    if !group_world.is_finite() {
        return Err(DocumentError::NonFiniteDerivedTransform(group.id).into());
    }
    let inverse_group = group_world
        .inverse()
        .ok_or(DocumentError::SingularWorldTransform(group.id))?;
    let mut child_states = Vec::with_capacity(placed.len());
    for (target, original_index, _, _) in &placed {
        let ungrouped = document.required_node(*target)?.local_transform();
        let grouped = inverse_group * document.world_transform(*target)?;
        if !grouped.is_finite() {
            return Err(DocumentError::NonFiniteDerivedTransform(*target).into());
        }
        child_states.push((*target, *original_index, ungrouped, grouped));
    }

    let effect = ReversibleEffect::StructuralGroup {
        grouped_forward: true,
        group: group.clone(),
        parent,
        index: group_index,
        children: child_states,
        restoration,
    };
    effect.apply_forward(document)?;
    document.add_sequence_work(planning_work);
    let mut affected = vec![parent, group.id];
    affected.extend(placed.into_iter().map(|(target, _, _, _)| target));
    changed(affected, effect)
}

fn ungroup_node(document: &mut Document, target: NodeId) -> Result<Execution, CommandError> {
    ensure_editable(document, target)?;
    let node = document.required_node(target)?;
    if node.kind() != NodeKind::Group {
        return Err(CommandError::UnsupportedOperation {
            operation: "ungroup",
            target,
            kind: node.kind(),
        });
    }
    let group = node.spec().clone();
    let restoration = node
        .group_restoration()
        .cloned()
        .ok_or(CommandError::InvalidGroup(
            "group restoration record is missing",
        ))?;
    if restoration.version != GroupRestoration::VERSION {
        return Err(CommandError::InvalidGroup(
            "group restoration version is unsupported",
        ));
    }
    let children = node.children().to_vec();
    let placement = required_placement(document, target)?;
    let mut planning_work = crate::SequenceWork::default();
    planning_work.entries_examined += children.len() as u64;
    planning_work.entries_copied += children.len() as u64;
    if children.is_empty() || restoration.runs.iter().any(|run| run.children.is_empty()) {
        return Err(CommandError::InvalidGroup(
            "group restoration record is inconsistent",
        ));
    }

    let mut run_by_child = HashMap::with_capacity(children.len());
    for (run_index, run) in restoration.runs.iter().enumerate() {
        for child in &run.children {
            planning_work.entries_examined += 1;
            if run_by_child.insert(*child, run_index).is_some()
                || run.before_anchor == Some(*child)
                || run.after_anchor == Some(*child)
                || run.before_anchor == Some(target)
                || run.after_anchor == Some(target)
            {
                return Err(CommandError::InvalidGroup(
                    "group restoration anchor or child set is invalid",
                ));
            }
        }
    }
    if run_by_child.len() != children.len()
        || children
            .iter()
            .any(|child| !run_by_child.contains_key(child))
    {
        return Err(CommandError::InvalidGroup(
            "group restoration child set is inconsistent",
        ));
    }

    ensure_editable(document, placement.parent)?;
    let parent_inverse = document
        .world_transform(placement.parent)?
        .inverse()
        .ok_or(DocumentError::SingularWorldTransform(placement.parent))?;
    let parent_children = document.required_node(placement.parent)?.children();
    let group_rank = parent_children
        .rank_of_tracked(target, &mut planning_work)
        .ok_or(CommandError::InvalidGroup(
            "group placement is inconsistent",
        ))?;

    let mut current_run_children = vec![Vec::new(); restoration.runs.len()];
    for child in &children {
        planning_work.entries_examined += 1;
        current_run_children[run_by_child[child]].push(*child);
    }
    let run_plans = restoration
        .runs
        .iter()
        .enumerate()
        .map(|(ordinal, run)| {
            let base = if let Some(before) = run
                .before_anchor
                .and_then(|anchor| parent_children.rank_of_tracked(anchor, &mut planning_work))
            {
                before - usize::from(group_rank < before) + 1
            } else if let Some(after) = run
                .after_anchor
                .and_then(|anchor| parent_children.rank_of_tracked(anchor, &mut planning_work))
            {
                after - usize::from(group_rank < after)
            } else {
                group_rank
            };
            (
                base,
                ordinal,
                std::mem::take(&mut current_run_children[ordinal]),
            )
        })
        .collect::<Vec<_>>();
    let mut restore_rank = HashMap::with_capacity(children.len());
    let mut planned_ranks = Vec::with_capacity(children.len());
    for (base, _, run_children) in run_plans {
        // Runs are processed in their original canonical order. Every child restored by a
        // previous run must therefore precede this run, including children whose final rank is
        // above the compressed anchor base after earlier insertions.
        let shifted = base + planned_ranks.len();
        let start = planned_ranks
            .last()
            .map_or(shifted, |previous| shifted.max(previous + 1));
        for (offset, child) in run_children.into_iter().enumerate() {
            planning_work.entries_examined += 1;
            let rank = start + offset;
            restore_rank.insert(child, rank);
            planned_ranks.push(rank);
        }
    }

    let mut child_states = Vec::with_capacity(children.len());
    for child in &children {
        ensure_editable(document, *child)?;
        let grouped = document.required_node(*child)?.local_transform();
        let ungrouped = parent_inverse * document.world_transform(*child)?;
        if !ungrouped.is_finite() {
            return Err(DocumentError::NonFiniteDerivedTransform(*child).into());
        }
        child_states.push((*child, restore_rank[child], ungrouped, grouped));
    }

    let effect = ReversibleEffect::StructuralGroup {
        grouped_forward: false,
        group,
        parent: placement.parent,
        index: placement.index,
        children: child_states,
        restoration,
    };
    effect.apply_forward(document)?;
    document.add_sequence_work(planning_work);
    let mut affected = vec![placement.parent, target];
    affected.extend(children);
    changed(affected, effect)
}

fn reparent(
    document: &mut Document,
    child: NodeId,
    new_parent: NodeId,
    index: usize,
    preserve_world: bool,
) -> Result<Execution, CommandError> {
    ensure_editable(document, child)?;
    ensure_editable(document, new_parent)?;
    let before = optional_placement(document, child)?;
    let before_transform = document.required_node(child)?.local_transform();
    let subtree = document.collect_subtree_preorder(child)?;
    if preserve_world {
        document.reparent_preserving_world(child, new_parent, index)?;
    } else {
        document.reparent(child, new_parent, index)?;
    }
    let after = Some(Placement {
        parent: new_parent,
        index,
    });
    let after_transform = document.required_node(child)?.local_transform();
    let effect = ReversibleEffect::MoveNode {
        target: child,
        subtree,
        before,
        after,
        before_transform,
        after_transform,
    };
    if effect.is_noop() {
        Ok(unchanged(vec![child]))
    } else {
        changed(vec![child], effect)
    }
}

fn edit_path(
    document: &mut Document,
    target: NodeId,
    operation: &'static str,
    edit: impl FnOnce(&mut PathGeometry) -> Result<(), CommandError>,
) -> Result<Execution, CommandError> {
    ensure_editable(document, target)?;
    let node = document.required_node(target)?;
    let before = node
        .geometry()
        .cloned()
        .ok_or(CommandError::UnsupportedOperation {
            operation,
            target,
            kind: node.kind(),
        })?;
    let mut after = match &before {
        Geometry::Path(path) => path.clone(),
        _ => {
            return Err(CommandError::UnsupportedOperation {
                operation,
                target,
                kind: node.kind(),
            });
        }
    };
    edit(&mut after)?;
    let after = Geometry::Path(after);
    if before == after {
        return Ok(unchanged(vec![target]));
    }
    document.set_geometry(target, after.clone())?;
    changed(
        vec![target],
        ReversibleEffect::SetGeometry {
            target,
            before,
            after,
        },
    )
}

fn anchor_index(
    path: &PathGeometry,
    target: NodeId,
    anchor: PathAnchorId,
) -> Result<usize, PathEditError> {
    path.anchors
        .iter()
        .position(|candidate| candidate.id == anchor)
        .ok_or(PathEditError::MissingAnchor { target, anchor })
}

fn segment_indices(
    path: &PathGeometry,
    target: NodeId,
    start: PathAnchorId,
    end: PathAnchorId,
) -> Result<(usize, usize), PathEditError> {
    let start_index = anchor_index(path, target, start)?;
    let end_index = anchor_index(path, target, end)?;
    let is_next = end_index == start_index + 1;
    let is_closing = path.closed && start_index + 1 == path.anchors.len() && end_index == 0;
    if !is_next && !is_closing {
        return Err(PathEditError::NonAdjacentSegment { target, start, end });
    }
    Ok((start_index, end_index))
}

fn lerp(start: Vec2, end: Vec2, parameter: f64) -> Vec2 {
    start + (end - start) * parameter
}

fn changed(affected: Vec<NodeId>, effect: ReversibleEffect) -> Result<Execution, CommandError> {
    Ok(Execution {
        outcome: CommandOutcome::with_change(affected),
        effect: Some(effect),
    })
}

fn unchanged(affected: Vec<NodeId>) -> Execution {
    Execution {
        outcome: CommandOutcome::unchanged(affected),
        effect: None,
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod path_edit_tests {
    use super::*;
    use crate::{DashPattern, EditorError, HeadlessEditorCore, StrokeCap, StrokeJoin};

    fn editor_with_path() -> (HeadlessEditorCore, NodeId, [PathAnchorId; 4]) {
        let mut editor = HeadlessEditorCore::blank("Root");
        let target = NodeId::new();
        let ids = [
            PathAnchorId::new(),
            PathAnchorId::new(),
            PathAnchorId::new(),
            PathAnchorId::new(),
        ];
        let mut anchors = vec![
            PathAnchor::new(ids[0], Vec2::new(0.0, 0.0)),
            PathAnchor::new(ids[1], Vec2::new(90.0, 30.0)),
            PathAnchor::new(ids[2], Vec2::new(140.0, 100.0)),
            PathAnchor::new(ids[3], Vec2::new(20.0, 120.0)),
        ];
        anchors[0].handle_out = Some(Vec2::new(25.0, -30.0));
        anchors[1].handle_in = Some(Vec2::new(65.0, 60.0));
        let root = editor.document().root_id();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::path(
                    target,
                    "Editable path",
                    PathGeometry {
                        closed: false,
                        anchors,
                    },
                ),
                parent: root,
                index: 0,
            })
            .unwrap();
        (editor, target, ids)
    }

    fn path(editor: &HeadlessEditorCore, target: NodeId) -> PathGeometry {
        match editor.document().node(target).unwrap().geometry().unwrap() {
            Geometry::Path(path) => path.clone(),
            _ => panic!("fixture must remain a path"),
        }
    }

    fn prove_exact_undo_redo(editor: &mut HeadlessEditorCore, command: Command) {
        let before = editor.document().clone();
        let outcome = editor.dispatch(command).unwrap();
        assert!(outcome.changed());
        let after = editor.document().clone();
        editor.undo().unwrap();
        assert_eq!(*editor.document(), before);
        editor.redo().unwrap();
        assert_eq!(*editor.document(), after);
    }

    #[test]
    fn every_phase2b_path_command_is_one_exact_undoable_operation() {
        let (mut editor, target, ids) = editor_with_path();
        prove_exact_undo_redo(
            &mut editor,
            Command::MovePathAnchor {
                target,
                anchor: ids[0],
                position: Vec2::new(10.0, 15.0),
            },
        );
        assert_eq!(
            path(&editor, target).anchors[0].handle_out,
            Some(Vec2::new(35.0, -15.0))
        );
        prove_exact_undo_redo(
            &mut editor,
            Command::SetPathHandle {
                target,
                anchor: ids[2],
                handle: PathHandle::Incoming,
                position: Some(Vec2::new(120.0, 70.0)),
            },
        );
        prove_exact_undo_redo(
            &mut editor,
            Command::SetPathSegmentKind {
                target,
                start: ids[2],
                end: ids[3],
                kind: PathSegmentKind::Cubic,
            },
        );
        let inserted = PathAnchorId::new();
        prove_exact_undo_redo(
            &mut editor,
            Command::InsertPathAnchor {
                target,
                start: ids[0],
                end: ids[1],
                anchor: inserted,
                parameter: 0.4,
            },
        );
        prove_exact_undo_redo(
            &mut editor,
            Command::DeletePathAnchor {
                target,
                anchor: ids[3],
            },
        );
        prove_exact_undo_redo(
            &mut editor,
            Command::SetPathClosed {
                target,
                closed: true,
            },
        );
        prove_exact_undo_redo(
            &mut editor,
            Command::SetStroke {
                target,
                stroke: Stroke {
                    width: 5.0,
                    cap: StrokeCap::Round,
                    join: StrokeJoin::Bevel,
                    miter_limit: 6.0,
                    dash_pattern: DashPattern::new(&[7.0, 2.0]).unwrap(),
                    ..Stroke::default()
                },
            },
        );
    }

    #[test]
    fn de_casteljau_insertion_preserves_the_curve_and_surviving_ids() {
        let (mut editor, target, ids) = editor_with_path();
        let before = path(&editor, target);
        let original = before.segments().next().unwrap();
        let inserted = PathAnchorId::new();
        let split = 0.37;
        editor
            .dispatch(Command::InsertPathAnchor {
                target,
                start: ids[0],
                end: ids[1],
                anchor: inserted,
                parameter: split,
            })
            .unwrap();
        let after = path(&editor, target);
        assert_eq!(
            after
                .anchors
                .iter()
                .map(|anchor| anchor.id)
                .collect::<Vec<_>>(),
            vec![ids[0], inserted, ids[1], ids[2], ids[3]]
        );
        let segments = after.segments().take(2).collect::<Vec<_>>();
        for step in 0..=100 {
            let parameter = f64::from(step) / 100.0;
            let preserved = if parameter <= split {
                segments[0].evaluate(parameter / split)
            } else {
                segments[1].evaluate((parameter - split) / (1.0 - split))
            };
            assert!(original.evaluate(parameter).approx_eq(preserved, 1.0e-9));
        }
    }

    #[test]
    fn stale_invalid_and_non_adjacent_path_requests_are_failure_atomic() {
        let (mut editor, target, ids) = editor_with_path();
        let missing = PathAnchorId::new();
        let invalid = [
            (
                Command::MovePathAnchor {
                    target,
                    anchor: missing,
                    position: Vec2::ZERO,
                },
                PathEditError::MissingAnchor {
                    target,
                    anchor: missing,
                },
            ),
            (
                Command::SetPathSegmentKind {
                    target,
                    start: ids[0],
                    end: ids[2],
                    kind: PathSegmentKind::Straight,
                },
                PathEditError::NonAdjacentSegment {
                    target,
                    start: ids[0],
                    end: ids[2],
                },
            ),
            (
                Command::InsertPathAnchor {
                    target,
                    start: ids[0],
                    end: ids[1],
                    anchor: ids[2],
                    parameter: 0.5,
                },
                PathEditError::DuplicateAnchor {
                    target,
                    anchor: ids[2],
                },
            ),
            (
                Command::InsertPathAnchor {
                    target,
                    start: ids[0],
                    end: ids[1],
                    anchor: missing,
                    parameter: 0.0,
                },
                PathEditError::InvalidSplitParameter,
            ),
        ];
        for (command, expected) in invalid {
            let before = editor.document().clone();
            let history = editor.history_state();
            assert_eq!(
                editor.dispatch(command),
                Err(EditorError::Command(CommandError::PathEdit(expected)))
            );
            assert_eq!(*editor.document(), before);
            assert_eq!(editor.history_state(), history);
        }

        let two_anchor = PathGeometry {
            closed: false,
            anchors: path(&editor, target).anchors[..2].to_vec(),
        };
        editor
            .dispatch(Command::SetGeometry {
                target,
                geometry: Geometry::Path(two_anchor),
            })
            .unwrap();
        for (command, expected) in [
            (
                Command::DeletePathAnchor {
                    target,
                    anchor: ids[0],
                },
                PathEditError::TooFewAnchors {
                    target,
                    anchor: ids[0],
                },
            ),
            (
                Command::SetPathClosed {
                    target,
                    closed: true,
                },
                PathEditError::ClosingRequiresThree { target, actual: 2 },
            ),
        ] {
            let before = editor.document().clone();
            let history = editor.history_state();
            assert_eq!(
                editor.dispatch(command),
                Err(EditorError::Command(CommandError::PathEdit(expected)))
            );
            assert_eq!(*editor.document(), before);
            assert_eq!(editor.history_state(), history);
        }
    }
}

fn apply_move(
    document: &mut Document,
    target: NodeId,
    destination: Option<Placement>,
    transform: Affine2,
) -> Result<(), CommandError> {
    let current_parent = document.required_node(target)?.parent();
    match (current_parent, destination) {
        (Some(_), Some(place)) => document.reparent(target, place.parent, place.index)?,
        (None, Some(place)) => document.attach_child(place.parent, target, place.index)?,
        (Some(_), None) => document.detach(target)?,
        (None, None) => {}
    }
    document.set_local_transform(target, transform)?;
    Ok(())
}

fn required_placement(document: &mut Document, id: NodeId) -> Result<Placement, CommandError> {
    optional_placement(document, id)?.ok_or(DocumentError::NodeNotAttached(id).into())
}

fn optional_placement(
    document: &mut Document,
    id: NodeId,
) -> Result<Option<Placement>, CommandError> {
    let node = document.required_node(id)?;
    let Some(parent) = node.parent() else {
        return Ok(None);
    };
    let index = document
        .child_rank_tracked(parent, id)?
        .ok_or(crate::InvariantViolation::ParentChildMismatch { parent, child: id })
        .map_err(DocumentError::from)?;
    Ok(Some(Placement { parent, index }))
}

fn capture_subtree(document: &mut Document, target: NodeId) -> Result<SubtreeRecord, CommandError> {
    let ids = document.collect_subtree_preorder(target)?;
    let nodes = ids
        .into_iter()
        .map(|id| {
            let node = document.required_node(id)?;
            Ok(NodeSnapshot {
                spec: node.spec().clone(),
                parent: node.parent(),
                children: node.children().to_vec(),
                group_restoration: node.group_restoration().cloned(),
            })
        })
        .collect::<Result<Vec<_>, DocumentError>>()?;
    Ok(SubtreeRecord {
        nodes,
        placement: optional_placement(document, target)?,
    })
}

fn ensure_editable(document: &Document, node: NodeId) -> Result<(), CommandError> {
    let mut current = Some(node);
    let mut visited = BTreeSet::new();
    while let Some(candidate) = current {
        if !visited.insert(candidate) {
            return Err(DocumentError::from(crate::InvariantViolation::Cycle(candidate)).into());
        }
        let candidate_node = document.required_node(candidate)?;
        if candidate_node.locked() {
            return Err(CommandError::LockedNode {
                node,
                locked_by: candidate,
            });
        }
        current = candidate_node.parent();
    }
    Ok(())
}

fn ensure_ancestors_editable(document: &Document, node: NodeId) -> Result<(), CommandError> {
    let mut current = document.required_node(node)?.parent();
    let mut visited = BTreeSet::new();
    while let Some(candidate) = current {
        if !visited.insert(candidate) {
            return Err(DocumentError::from(crate::InvariantViolation::Cycle(candidate)).into());
        }
        let candidate_node = document.required_node(candidate)?;
        if candidate_node.locked() {
            return Err(CommandError::LockedNode {
                node,
                locked_by: candidate,
            });
        }
        current = candidate_node.parent();
    }
    Ok(())
}

fn ensure_subtree_editable(document: &Document, target: NodeId) -> Result<(), CommandError> {
    ensure_editable(document, target)?;
    for node_id in document.collect_subtree_preorder(target)? {
        if document.required_node(node_id)?.locked() {
            return Err(CommandError::LockedNode {
                node: node_id,
                locked_by: node_id,
            });
        }
    }
    Ok(())
}

fn ensure_lock_change_allowed(
    document: &Document,
    target: NodeId,
    before: bool,
    after: bool,
) -> Result<(), CommandError> {
    if before || !after {
        ensure_ancestors_editable(document, target)
    } else {
        ensure_editable(document, target)
    }
}
