//! Persistent semantic document model.
//!
//! Persistent mutation is available only through [`HeadlessEditorCore`] and typed [`Command`]
//! values. Callers can inspect [`Document`] and [`Node`] through read-only APIs.
//! `DocumentSnapshot` is an explicit validated construction/persistence boundary;
//! it is not a normal editing API or a renderer/runtime cache representation.

mod change;
#[cfg(test)]
mod change_tests;
mod command;
mod editor;
mod order_sequence;

pub use change::{
    ChangeMergeError, DocumentChange, DocumentChangeSet, DocumentRevision, NodePlacement,
    PersistentProperty, StructuralGroupChange,
};
pub use command::{Command, CommandError, CommandOutcome};
pub use editor::{EditorError, HeadlessEditorCore, HistoryState, Selection, SelectionError};
pub use order_sequence::{OrderSequence, SequenceWork};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use visual_authoring_core_math::{Affine2, Rect, Vec2, DEFAULT_EPSILON};

/// Stable persistent identity for a document node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NodeId(Uuid);

impl NodeId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Persistent semantic node category, independent of any renderer type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Document,
    Frame,
    Group,
    Rectangle,
    Ellipse,
}

impl NodeKind {
    #[must_use]
    pub const fn can_contain_children(self) -> bool {
        matches!(self, Self::Document | Self::Frame | Self::Group)
    }
}

/// Geometry payload kept separate from node category and appearance.
#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Frame { size: Vec2 },
    Rectangle { size: Vec2 },
    Ellipse { size: Vec2 },
}

impl Geometry {
    #[must_use]
    pub const fn size(&self) -> Vec2 {
        match self {
            Self::Frame { size } | Self::Rectangle { size } | Self::Ellipse { size } => *size,
        }
    }
}

/// Persistent sRGB color. Conversion to the renderer's linear working space happens at the
/// derived RenderModel boundary so serialization and UI values remain stable and intuitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorRgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl ColorRgba {
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    fn is_valid(self) -> bool {
        [self.r, self.g, self.b, self.a]
            .into_iter()
            .all(|component| component.is_finite() && (0.0..=1.0).contains(&component))
    }
}

impl Default for ColorRgba {
    fn default() -> Self {
        Self::new(0.20, 0.58, 0.96, 1.0)
    }
}

/// Four independent rectangle corner radii in top-left, top-right, bottom-right,
/// bottom-left order. The renderer normalizes overlapping radii against geometry size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadii {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl CornerRadii {
    pub const fn uniform(radius: f64) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    fn is_valid(self) -> bool {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
        .into_iter()
        .all(|radius| radius.is_finite() && radius >= 0.0)
    }
}

/// Phase 1A supports a single, explicitly centered solid stroke alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: ColorRgba,
    pub width: f64,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: ColorRgba::new(0.08, 0.11, 0.16, 1.0),
            width: 0.0,
        }
    }
}

/// Versioned persistent primitive appearance. Opacity is independent from fill/stroke alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub fill: ColorRgba,
    pub opacity: f64,
    pub corner_radii: CornerRadii,
    pub stroke: Stroke,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            fill: ColorRgba::default(),
            opacity: 1.0,
            corner_radii: CornerRadii::default(),
            stroke: Stroke::default(),
        }
    }
}

/// Conservative metadata boundary; project-owned semantics should use typed fields.
pub type Metadata = BTreeMap<String, String>;

/// Versioned engine-owned provenance used to restore grouped children without user metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRestoration {
    pub version: u32,
    pub runs: Vec<GroupRestorationRun>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRestorationRun {
    pub children: Vec<NodeId>,
    pub before_anchor: Option<NodeId>,
    pub after_anchor: Option<NodeId>,
}

impl GroupRestoration {
    pub const VERSION: u32 = 2;

    pub fn children(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.runs
            .iter()
            .flat_map(|run| run.children.iter().copied())
    }
}

/// Hierarchy-free node input. All fields are persistent semantic data.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub local_transform: Affine2,
    pub visible: bool,
    pub locked: bool,
    pub geometry: Option<Geometry>,
    pub appearance: Appearance,
    pub metadata: Metadata,
}

impl NodeSpec {
    #[must_use]
    pub fn document(id: NodeId, name: impl Into<String>) -> Self {
        Self::new(id, name, NodeKind::Document, None)
    }

    #[must_use]
    pub fn frame(id: NodeId, name: impl Into<String>, size: Vec2) -> Self {
        Self::new(id, name, NodeKind::Frame, Some(Geometry::Frame { size }))
    }

    #[must_use]
    pub fn group(id: NodeId, name: impl Into<String>) -> Self {
        Self::new(id, name, NodeKind::Group, None)
    }

    #[must_use]
    pub fn rectangle(id: NodeId, name: impl Into<String>, size: Vec2) -> Self {
        Self::new(
            id,
            name,
            NodeKind::Rectangle,
            Some(Geometry::Rectangle { size }),
        )
    }

    #[must_use]
    pub fn ellipse(id: NodeId, name: impl Into<String>, size: Vec2) -> Self {
        Self::new(
            id,
            name,
            NodeKind::Ellipse,
            Some(Geometry::Ellipse { size }),
        )
    }

    #[must_use]
    pub fn new(
        id: NodeId,
        name: impl Into<String>,
        kind: NodeKind,
        geometry: Option<Geometry>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            local_transform: Affine2::IDENTITY,
            visible: true,
            locked: false,
            geometry,
            appearance: Appearance::default(),
            metadata: Metadata::new(),
        }
    }
}

/// Read-only public view of a registered persistent node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    spec: NodeSpec,
    parent: Option<NodeId>,
    children: OrderSequence,
    group_restoration: Option<GroupRestoration>,
}

impl Node {
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.spec.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.spec.name
    }

    #[must_use]
    pub const fn kind(&self) -> NodeKind {
        self.spec.kind
    }

    #[must_use]
    pub const fn local_transform(&self) -> Affine2 {
        self.spec.local_transform
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.spec.visible
    }

    #[must_use]
    pub const fn locked(&self) -> bool {
        self.spec.locked
    }

    #[must_use]
    pub fn geometry(&self) -> Option<&Geometry> {
        self.spec.geometry.as_ref()
    }

    #[must_use]
    pub const fn appearance(&self) -> Appearance {
        self.spec.appearance
    }

    #[must_use]
    pub fn metadata(&self) -> &Metadata {
        &self.spec.metadata
    }

    #[must_use]
    pub const fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    #[must_use]
    pub const fn children(&self) -> &OrderSequence {
        &self.children
    }

    #[must_use]
    pub const fn group_restoration(&self) -> Option<&GroupRestoration> {
        self.group_restoration.as_ref()
    }

    #[must_use]
    pub fn spec(&self) -> &NodeSpec {
        &self.spec
    }
}

/// A complete persistent node record used at validated subsystem boundaries.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSnapshot {
    pub spec: NodeSpec,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub group_restoration: Option<GroupRestoration>,
}

/// A hierarchy-complete semantic snapshot. It contains no computed/runtime state.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentSnapshot {
    pub root_id: NodeId,
    pub nodes: Vec<NodeSnapshot>,
}

/// Persistent document source of truth.
#[derive(Clone, Debug)]
pub struct Document {
    root_id: NodeId,
    nodes: BTreeMap<NodeId, Node>,
    sequence_work: SequenceWork,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.root_id == other.root_id && self.nodes == other.nodes
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum DocumentError {
    #[error("node {0} was not found")]
    NodeNotFound(NodeId),
    #[error("node ID {0} is already registered")]
    DuplicateNodeId(NodeId),
    #[error("only the document constructor may register a Document node")]
    CannotRegisterDocumentNode,
    #[error("root operation is not permitted for node {0}")]
    RootOperation(NodeId),
    #[error("node {0} cannot contain children")]
    InvalidParent(NodeId),
    #[error("node {node} is already parented by {parent}")]
    AlreadyParented { node: NodeId, parent: NodeId },
    #[error("node {0} is not attached")]
    NodeNotAttached(NodeId),
    #[error("child index {index} is out of bounds for parent {parent} (length {length})")]
    ChildIndexOutOfBounds {
        parent: NodeId,
        index: usize,
        length: usize,
    },
    #[error("attaching {child} below {parent} would create a cycle")]
    CycleDetected { parent: NodeId, child: NodeId },
    #[error("transform for node {0} contains non-finite values")]
    InvalidTransform(NodeId),
    #[error("derived transform calculation for node {0} produced non-finite values")]
    NonFiniteDerivedTransform(NodeId),
    #[error("coordinate conversion for node {0} produced non-finite values")]
    NonFiniteCoordinate(NodeId),
    #[error("world bounds calculation for node {0} produced non-finite values")]
    NonFiniteBounds(NodeId),
    #[error("world transform for node {0} is singular")]
    SingularWorldTransform(NodeId),
    #[error("geometry does not match node kind or contains an invalid size for node {0}")]
    InvalidGeometry(NodeId),
    #[error("appearance contains invalid values for node {0}")]
    InvalidAppearance(NodeId),
    #[error("internal subtree restore record is invalid: {0}")]
    InvalidSubtreeRestore(&'static str),
    #[error("document invariant violation: {0}")]
    Invariant(#[from] InvariantViolation),
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum InvariantViolation {
    #[error("root node {0} is missing")]
    MissingRoot(NodeId),
    #[error("root node {0} is not the sole Document node")]
    InvalidDocumentRoot(NodeId),
    #[error("root node {0} has a parent")]
    RootHasParent(NodeId),
    #[error("root node {0} must have the identity local transform")]
    RootTransform(NodeId),
    #[error("node {0} has a non-finite transform")]
    NonFiniteTransform(NodeId),
    #[error("node {0} has geometry inconsistent with its kind or size constraints")]
    InvalidGeometry(NodeId),
    #[error("node {0} has invalid appearance data")]
    InvalidAppearance(NodeId),
    #[error("node {node} refers to missing parent {parent}")]
    DanglingParent { node: NodeId, parent: NodeId },
    #[error("parent {parent} refers to missing child {child}")]
    DanglingChild { parent: NodeId, child: NodeId },
    #[error("parent/child references disagree for parent {parent} and child {child}")]
    ParentChildMismatch { parent: NodeId, child: NodeId },
    #[error("parent {parent} contains duplicate child {child}")]
    DuplicateChild { parent: NodeId, child: NodeId },
    #[error("node {0} has children but is not a container")]
    InvalidContainer(NodeId),
    #[error("node {node} uses unsupported group restoration version {version}")]
    UnsupportedGroupRestorationVersion { node: NodeId, version: u32 },
    #[error("node {0} has an invalid group restoration record")]
    InvalidGroupRestoration(NodeId),
    #[error("node {0} participates in a hierarchy cycle")]
    Cycle(NodeId),
}

impl Document {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_root(NodeSpec::document(NodeId::new(), name))
            .expect("the built-in root specification is valid")
    }

    pub fn with_root(root: NodeSpec) -> Result<Self, DocumentError> {
        if root.kind != NodeKind::Document || root.geometry.is_some() {
            return Err(InvariantViolation::InvalidDocumentRoot(root.id).into());
        }
        if !root
            .local_transform
            .approx_eq(Affine2::IDENTITY, DEFAULT_EPSILON)
        {
            return Err(InvariantViolation::RootTransform(root.id).into());
        }
        validate_node_spec(&root)?;

        let root_id = root.id;
        let mut nodes = BTreeMap::new();
        nodes.insert(
            root_id,
            Node {
                spec: root,
                parent: None,
                children: OrderSequence::default(),
                group_restoration: None,
            },
        );
        Ok(Self {
            root_id,
            nodes,
            sequence_work: SequenceWork::default(),
        })
    }

    pub fn from_snapshot(snapshot: DocumentSnapshot) -> Result<Self, DocumentError> {
        let mut nodes = BTreeMap::new();
        let mut sequence_work = SequenceWork::default();
        for node in snapshot.nodes {
            validate_node_spec(&node.spec)?;
            let id = node.spec.id;
            if nodes
                .insert(
                    id,
                    Node {
                        spec: node.spec,
                        parent: node.parent,
                        children: OrderSequence::from_ids_tracked(
                            node.children,
                            &mut sequence_work,
                        ),
                        group_restoration: node.group_restoration,
                    },
                )
                .is_some()
            {
                return Err(DocumentError::DuplicateNodeId(id));
            }
        }

        let document = Self {
            root_id: snapshot.root_id,
            nodes,
            sequence_work,
        };
        document.validate_invariants()?;
        Ok(document)
    }

    #[must_use]
    pub const fn root_id(&self) -> NodeId {
        self.root_id
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &Node> {
        self.nodes.values()
    }

    #[must_use]
    pub const fn sequence_work(&self) -> SequenceWork {
        self.sequence_work
    }

    pub(crate) fn reset_sequence_work(&mut self) {
        self.sequence_work = SequenceWork::default();
    }

    pub(crate) fn add_sequence_work(&mut self, work: SequenceWork) {
        self.sequence_work += work;
    }

    pub(crate) fn child_rank_tracked(
        &mut self,
        parent: NodeId,
        child: NodeId,
    ) -> Result<Option<usize>, DocumentError> {
        let mut work = SequenceWork::default();
        let rank = self
            .required_node(parent)?
            .children
            .rank_of_tracked(child, &mut work);
        self.sequence_work += work;
        Ok(rank)
    }

    #[must_use]
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            root_id: self.root_id,
            nodes: self
                .nodes
                .values()
                .map(|node| NodeSnapshot {
                    spec: node.spec.clone(),
                    parent: node.parent,
                    children: node.children.to_vec(),
                    group_restoration: node.group_restoration.clone(),
                })
                .collect(),
        }
    }

    pub(crate) fn register_node(&mut self, spec: NodeSpec) -> Result<NodeId, DocumentError> {
        if spec.kind == NodeKind::Document {
            return Err(DocumentError::CannotRegisterDocumentNode);
        }
        validate_node_spec(&spec)?;
        let id = spec.id;
        if self.nodes.contains_key(&id) {
            return Err(DocumentError::DuplicateNodeId(id));
        }
        self.nodes.insert(
            id,
            Node {
                spec,
                parent: None,
                children: OrderSequence::default(),
                group_restoration: None,
            },
        );
        Ok(id)
    }

    /// Registers and attaches a node atomically.
    pub(crate) fn create_node(
        &mut self,
        spec: NodeSpec,
        parent: NodeId,
        index: usize,
    ) -> Result<NodeId, DocumentError> {
        let id = self.register_node(spec)?;
        if let Err(error) = self.attach_child(parent, id, index) {
            self.nodes.remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    pub(crate) fn set_local_transform(
        &mut self,
        id: NodeId,
        transform: Affine2,
    ) -> Result<(), DocumentError> {
        if id == self.root_id {
            return Err(DocumentError::RootOperation(id));
        }
        if !transform.is_finite() {
            return Err(DocumentError::InvalidTransform(id));
        }
        self.node_mut(id)?.spec.local_transform = transform;
        Ok(())
    }

    pub(crate) fn set_name(&mut self, id: NodeId, name: String) -> Result<(), DocumentError> {
        self.node_mut(id)?.spec.name = name;
        Ok(())
    }

    pub(crate) fn set_visible(&mut self, id: NodeId, visible: bool) -> Result<(), DocumentError> {
        self.node_mut(id)?.spec.visible = visible;
        Ok(())
    }

    pub(crate) fn set_locked(&mut self, id: NodeId, locked: bool) -> Result<(), DocumentError> {
        self.node_mut(id)?.spec.locked = locked;
        Ok(())
    }

    pub(crate) fn set_geometry(
        &mut self,
        id: NodeId,
        geometry: Geometry,
    ) -> Result<(), DocumentError> {
        let mut replacement = self.required_node(id)?.spec.clone();
        replacement.geometry = Some(geometry);
        validate_node_spec(&replacement)?;
        self.node_mut(id)?.spec = replacement;
        Ok(())
    }

    pub(crate) fn set_appearance(
        &mut self,
        id: NodeId,
        appearance: Appearance,
    ) -> Result<(), DocumentError> {
        let mut replacement = self.required_node(id)?.spec.clone();
        replacement.appearance = appearance;
        validate_node_spec(&replacement)?;
        self.node_mut(id)?.spec = replacement;
        Ok(())
    }

    pub(crate) fn set_metadata(
        &mut self,
        id: NodeId,
        metadata: Metadata,
    ) -> Result<(), DocumentError> {
        self.node_mut(id)?.spec.metadata = metadata;
        Ok(())
    }

    pub(crate) fn attach_child(
        &mut self,
        parent: NodeId,
        child: NodeId,
        index: usize,
    ) -> Result<(), DocumentError> {
        self.validate_attachment(parent, child)?;

        if let Some(existing_parent) = self.required_node(child)?.parent {
            return Err(DocumentError::AlreadyParented {
                node: child,
                parent: existing_parent,
            });
        }

        let child_count = self.required_node(parent)?.children.len();
        if index > child_count {
            return Err(DocumentError::ChildIndexOutOfBounds {
                parent,
                index,
                length: child_count,
            });
        }

        let work = self.node_mut(parent)?.children.insert(index, child);
        self.node_mut(child)?.parent = Some(parent);
        self.sequence_work += work;
        Ok(())
    }

    pub(crate) fn detach(&mut self, child: NodeId) -> Result<(), DocumentError> {
        if child == self.root_id {
            return Err(DocumentError::RootOperation(child));
        }
        let parent = self
            .required_node(child)?
            .parent
            .ok_or(DocumentError::NodeNotAttached(child))?;
        self.remove_child_reference(parent, child)?;
        self.node_mut(child)?.parent = None;
        Ok(())
    }

    /// Reparents a node while retaining its current local transform.
    pub(crate) fn reparent(
        &mut self,
        child: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> Result<(), DocumentError> {
        self.reparent_internal(child, new_parent, index, None)
    }

    /// Applies the already-validated grouped state in one commit. Only the parent, group,
    /// and selected child records are mutated; no full Document clone is made.
    pub(crate) fn apply_group_patch(
        &mut self,
        group: &NodeSpec,
        parent: NodeId,
        index: usize,
        children: &[(NodeId, usize, Affine2, Affine2)],
        restoration: &GroupRestoration,
    ) -> Result<(), DocumentError> {
        validate_node_spec(group)?;
        if group.kind != NodeKind::Group || group.geometry.is_some() {
            return Err(DocumentError::InvalidSubtreeRestore(
                "group patch requires a Group node",
            ));
        }
        if self.nodes.contains_key(&group.id) {
            return Err(DocumentError::DuplicateNodeId(group.id));
        }
        let parent_node = self.required_node(parent)?;
        if !parent_node.kind().can_contain_children() {
            return Err(DocumentError::InvalidParent(parent));
        }
        if index >= parent_node.children.len() {
            return Err(DocumentError::ChildIndexOutOfBounds {
                parent,
                index,
                length: parent_node.children.len(),
            });
        }
        let targets = children
            .iter()
            .map(|(id, _, _, _)| *id)
            .collect::<BTreeSet<_>>();
        if targets.len() != children.len()
            || children.len() < 2
            || restoration.version != GroupRestoration::VERSION
            || restoration.runs.iter().any(|run| run.children.is_empty())
            || restoration.children().collect::<BTreeSet<_>>() != targets
            || restoration.children().count() != children.len()
            || restoration.runs.iter().any(|run| {
                run.before_anchor
                    .into_iter()
                    .chain(run.after_anchor)
                    .any(|anchor| anchor == group.id || targets.contains(&anchor))
            })
        {
            return Err(DocumentError::InvalidSubtreeRestore(
                "group patch child set or restoration is invalid",
            ));
        }
        let mut work = SequenceWork::default();
        for (id, _, ungrouped, grouped) in children {
            let node = self.required_node(*id)?;
            if node.parent != Some(parent)
                || node.local_transform() != *ungrouped
                || !grouped.is_finite()
            {
                return Err(DocumentError::InvalidSubtreeRestore(
                    "group patch precondition failed",
                ));
            }
        }
        if index > parent_node.children.len() - children.len() {
            return Err(DocumentError::InvalidSubtreeRestore(
                "group patch insertion rank is invalid",
            ));
        }

        let front_contiguous =
            restoration.runs.len() == 1 && restoration.runs[0].before_anchor.is_none();
        let group_children = OrderSequence::from_ids_tracked(
            children.iter().map(|(child, _, _, _)| *child),
            &mut work,
        );
        let mut remove_child = |id: NodeId, prefer_predecessor: bool| {
            work.entries_examined += 1;
            let parent_children = &mut self
                .nodes
                .get_mut(&parent)
                .expect("validated parent")
                .children;
            let removed = if prefer_predecessor {
                parent_children.remove_known_tracked_prefer_predecessor(id, &mut work)
            } else {
                parent_children.remove_known_tracked(id, &mut work)
            };
            assert!(removed);
        };
        if front_contiguous {
            for id in restoration.runs[0].children.iter().copied() {
                remove_child(id, false);
            }
        } else {
            for run in restoration.runs.iter().rev() {
                let prefer_predecessor = run.children.len() == 1;
                for id in run.children.iter().rev().copied() {
                    remove_child(id, prefer_predecessor);
                }
            }
        }
        work += self
            .nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .insert(index, group.id);
        self.nodes.insert(
            group.id,
            Node {
                spec: group.clone(),
                parent: Some(parent),
                children: group_children,
                group_restoration: Some(restoration.clone()),
            },
        );
        for (id, _, _, grouped) in children {
            let node = self.nodes.get_mut(id).expect("validated child");
            node.parent = Some(group.id);
            node.spec.local_transform = *grouped;
        }
        self.sequence_work += work;
        Ok(())
    }

    pub(crate) fn apply_ungroup_patch(
        &mut self,
        group: &NodeSpec,
        parent: NodeId,
        index: usize,
        children: &[(NodeId, usize, Affine2, Affine2)],
        restoration: &GroupRestoration,
    ) -> Result<(), DocumentError> {
        let group_node = self.required_node(group.id)?;
        if group_node.parent != Some(parent)
            || group_node.spec != *group
            || group_node.children.len() != children.len()
            || group_node.children.iter().copied().collect::<BTreeSet<_>>()
                != children
                    .iter()
                    .map(|entry| entry.0)
                    .collect::<BTreeSet<_>>()
            || group_node.group_restoration.as_ref() != Some(restoration)
        {
            return Err(DocumentError::InvalidSubtreeRestore(
                "ungroup patch precondition failed",
            ));
        }
        let parent_node = self.required_node(parent)?;
        let mut work = SequenceWork::default();
        if parent_node.children.rank_of_tracked(group.id, &mut work) != Some(index) {
            return Err(DocumentError::InvalidSubtreeRestore(
                "ungroup placement precondition failed",
            ));
        }
        let final_length = parent_node.children.len() - 1 + children.len();
        let mut restore_ranks = BTreeSet::new();
        for (id, restore_rank, ungrouped, grouped) in children {
            let node = self.required_node(*id)?;
            if node.parent != Some(group.id)
                || node.local_transform() != *grouped
                || !ungrouped.is_finite()
                || *restore_rank >= final_length
                || !restore_ranks.insert(*restore_rank)
            {
                return Err(DocumentError::InvalidSubtreeRestore(
                    "ungroup child precondition failed",
                ));
            }
        }

        assert!(self
            .nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .remove_known_tracked(group.id, &mut work));
        let mut placements = children.iter().collect::<Vec<_>>();
        placements.sort_by_key(|entry| entry.1);
        for (id, restore_rank, _, _) in placements {
            work += self
                .nodes
                .get_mut(&parent)
                .expect("validated parent")
                .children
                .insert(*restore_rank, *id);
        }
        for (id, _, ungrouped, _) in children {
            let node = self.nodes.get_mut(id).expect("validated child");
            node.parent = Some(parent);
            node.spec.local_transform = *ungrouped;
        }
        self.nodes.remove(&group.id);
        self.sequence_work += work;
        Ok(())
    }

    /// Reparents a node and adjusts its local transform so its world transform is unchanged.
    pub(crate) fn reparent_preserving_world(
        &mut self,
        child: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> Result<(), DocumentError> {
        let old_world = self.world_transform(child)?;
        let new_parent_world = self.world_transform(new_parent)?;
        let inverse_parent = new_parent_world
            .inverse()
            .ok_or(DocumentError::SingularWorldTransform(new_parent))?;
        let new_local = inverse_parent * old_world;
        if !new_local.is_finite() {
            return Err(DocumentError::NonFiniteDerivedTransform(child));
        }
        self.reparent_internal(child, new_parent, index, Some(new_local))
    }

    /// Deletes a node and its complete descendant subtree. The document root cannot be deleted.
    pub(crate) fn delete_subtree(&mut self, id: NodeId) -> Result<Vec<NodeId>, DocumentError> {
        if id == self.root_id {
            return Err(DocumentError::RootOperation(id));
        }
        self.required_node(id)?;

        // Complete the fallible traversal before mutating parent links or storage.
        let removed = self.collect_subtree_preorder(id)?;
        if let Some(parent) = self.required_node(id)?.parent {
            self.remove_child_reference(parent, id)?;
        }
        for removed_id in &removed {
            self.nodes.remove(removed_id);
        }
        Ok(removed)
    }

    pub(crate) fn restore_subtree(
        &mut self,
        snapshots: &[NodeSnapshot],
        placement: Option<(NodeId, usize)>,
    ) -> Result<(), DocumentError> {
        let root = snapshots
            .first()
            .ok_or(DocumentError::InvalidSubtreeRestore("empty record"))?;
        let root_id = root.spec.id;
        if root_id == self.root_id || root.spec.kind == NodeKind::Document {
            return Err(DocumentError::InvalidSubtreeRestore(
                "document root cannot be restored as a subtree",
            ));
        }

        let mut records = BTreeMap::new();
        for snapshot in snapshots {
            validate_node_spec(&snapshot.spec)?;
            if self.nodes.contains_key(&snapshot.spec.id)
                || records.insert(snapshot.spec.id, snapshot).is_some()
            {
                return Err(DocumentError::DuplicateNodeId(snapshot.spec.id));
            }
        }

        if root.parent != placement.map(|(parent, _)| parent) {
            return Err(DocumentError::InvalidSubtreeRestore(
                "root parent does not match placement",
            ));
        }

        for snapshot in snapshots {
            let id = snapshot.spec.id;
            if id != root_id {
                let parent = snapshot.parent.ok_or(DocumentError::InvalidSubtreeRestore(
                    "non-root record has no parent",
                ))?;
                if !records.contains_key(&parent) {
                    return Err(DocumentError::InvalidSubtreeRestore(
                        "non-root parent is outside the record",
                    ));
                }
            }
            if !snapshot.spec.kind.can_contain_children() && !snapshot.children.is_empty() {
                return Err(DocumentError::InvalidSubtreeRestore(
                    "non-container record has children",
                ));
            }
            let mut unique = BTreeSet::new();
            for child in &snapshot.children {
                if !unique.insert(*child) {
                    return Err(DocumentError::InvalidSubtreeRestore(
                        "duplicate child in record",
                    ));
                }
                let child_record =
                    records
                        .get(child)
                        .ok_or(DocumentError::InvalidSubtreeRestore(
                            "record refers to missing child",
                        ))?;
                if child_record.parent != Some(id) {
                    return Err(DocumentError::InvalidSubtreeRestore(
                        "record parent and child references disagree",
                    ));
                }
            }
        }

        let mut reached = BTreeSet::new();
        let mut stack = vec![root_id];
        while let Some(id) = stack.pop() {
            if !reached.insert(id) {
                return Err(DocumentError::InvalidSubtreeRestore(
                    "cycle or duplicate reachability in record",
                ));
            }
            stack.extend(records[&id].children.iter().rev().copied());
        }
        if reached.len() != records.len() {
            return Err(DocumentError::InvalidSubtreeRestore(
                "record contains nodes outside the restored subtree",
            ));
        }

        if let Some((parent, index)) = placement {
            let parent_node = self.required_node(parent)?;
            if !parent_node.kind().can_contain_children() {
                return Err(DocumentError::InvalidParent(parent));
            }
            if index > parent_node.children.len() {
                return Err(DocumentError::ChildIndexOutOfBounds {
                    parent,
                    index,
                    length: parent_node.children.len(),
                });
            }
        }

        let mut sequence_work = SequenceWork::default();
        if let Some((parent, index)) = placement {
            sequence_work += self
                .nodes
                .get_mut(&parent)
                .ok_or(DocumentError::NodeNotFound(parent))?
                .children
                .insert(index, root_id);
        }
        for snapshot in snapshots {
            let children = OrderSequence::from_ids_tracked(
                snapshot.children.iter().copied(),
                &mut sequence_work,
            );
            self.nodes.insert(
                snapshot.spec.id,
                Node {
                    spec: snapshot.spec.clone(),
                    parent: snapshot.parent,
                    children,
                    group_restoration: snapshot.group_restoration.clone(),
                },
            );
        }
        self.sequence_work += sequence_work;
        Ok(())
    }

    pub fn world_transform(&self, id: NodeId) -> Result<Affine2, DocumentError> {
        let mut current = id;
        let mut world = self.required_node(current)?.spec.local_transform;
        if !world.is_finite() {
            return Err(DocumentError::NonFiniteDerivedTransform(id));
        }
        let mut visited = BTreeSet::from([current]);

        while let Some(parent) = self.required_node(current)?.parent {
            if !visited.insert(parent) {
                return Err(InvariantViolation::Cycle(parent).into());
            }
            let parent_node = self.required_node(parent)?;
            let composed = parent_node.spec.local_transform * world;
            if !composed.is_finite() {
                return Err(DocumentError::NonFiniteDerivedTransform(id));
            }
            world = composed;
            current = parent;
        }
        Ok(world)
    }

    pub fn local_to_world(&self, id: NodeId, point: Vec2) -> Result<Vec2, DocumentError> {
        if !point.is_finite() {
            return Err(DocumentError::NonFiniteCoordinate(id));
        }
        let transformed = self.world_transform(id)?.transform_point(point);
        if !transformed.is_finite() {
            return Err(DocumentError::NonFiniteCoordinate(id));
        }
        Ok(transformed)
    }

    pub fn world_to_local(&self, id: NodeId, point: Vec2) -> Result<Vec2, DocumentError> {
        if !point.is_finite() {
            return Err(DocumentError::NonFiniteCoordinate(id));
        }
        let inverse = self
            .world_transform(id)?
            .inverse()
            .ok_or(DocumentError::SingularWorldTransform(id))?;
        let transformed = inverse.transform_point(point);
        if !transformed.is_finite() {
            return Err(DocumentError::NonFiniteCoordinate(id));
        }
        Ok(transformed)
    }

    pub fn local_bounds(&self, id: NodeId) -> Result<Option<Rect>, DocumentError> {
        let node = self.required_node(id)?;
        Ok(node.spec.geometry.as_ref().map(|geometry| {
            let size = geometry.size();
            Rect::from_size(size)
        }))
    }

    /// Returns a geometry-only axis-aligned world bound. Effects are intentionally excluded.
    pub fn world_bounds(&self, id: NodeId) -> Result<Option<Rect>, DocumentError> {
        let node = self.required_node(id)?;
        let Some(geometry) = node.spec.geometry.as_ref() else {
            return Ok(None);
        };
        let world = self.world_transform(id)?;
        let size = geometry.size();

        let bounds = match geometry {
            Geometry::Ellipse { .. } => {
                let local_center = size * 0.5;
                let radii = size * 0.5;
                let center = world.transform_point(local_center);
                let extent = Vec2::new(
                    (world.m11 * radii.x).hypot(world.m12 * radii.y),
                    (world.m21 * radii.x).hypot(world.m22 * radii.y),
                );
                if !center.is_finite() || !extent.is_finite() {
                    return Err(DocumentError::NonFiniteBounds(id));
                }
                Rect::from_min_max(center - extent, center + extent)
            }
            Geometry::Frame { .. } | Geometry::Rectangle { .. } => {
                world.transform_rect(Rect::from_size(size))
            }
        };
        if !bounds.is_finite() {
            return Err(DocumentError::NonFiniteBounds(id));
        }
        Ok(Some(bounds))
    }

    pub fn validate_invariants(&self) -> Result<(), InvariantViolation> {
        let root = self
            .nodes
            .get(&self.root_id)
            .ok_or(InvariantViolation::MissingRoot(self.root_id))?;
        let document_nodes = self
            .nodes
            .values()
            .filter(|node| node.spec.kind == NodeKind::Document)
            .count();
        if root.spec.kind != NodeKind::Document || document_nodes != 1 {
            return Err(InvariantViolation::InvalidDocumentRoot(self.root_id));
        }
        if root.parent.is_some() {
            return Err(InvariantViolation::RootHasParent(self.root_id));
        }
        if !root
            .spec
            .local_transform
            .approx_eq(Affine2::IDENTITY, DEFAULT_EPSILON)
        {
            return Err(InvariantViolation::RootTransform(self.root_id));
        }

        let mut hierarchy_edges = BTreeSet::new();
        for node in self.nodes.values() {
            validate_spec_invariant(&node.spec)?;

            if !node.spec.kind.can_contain_children() && !node.children.is_empty() {
                return Err(InvariantViolation::InvalidContainer(node.id()));
            }

            if let Some(restoration) = &node.group_restoration {
                if restoration.version != GroupRestoration::VERSION {
                    return Err(InvariantViolation::UnsupportedGroupRestorationVersion {
                        node: node.id(),
                        version: restoration.version,
                    });
                }
                let children = node.children.iter().copied().collect::<BTreeSet<_>>();
                let restored = restoration.children().collect::<Vec<_>>();
                let restored_unique = restored.iter().copied().collect::<BTreeSet<_>>();
                let invalid_anchor = restoration.runs.iter().any(|run| {
                    run.before_anchor
                        .into_iter()
                        .chain(run.after_anchor)
                        .any(|anchor| anchor == node.id() || children.contains(&anchor))
                });
                if node.spec.kind != NodeKind::Group
                    || restoration.runs.is_empty()
                    || restoration.runs.iter().any(|run| run.children.is_empty())
                    || restored.len() != node.children.len()
                    || restored_unique != children
                    || invalid_anchor
                {
                    return Err(InvariantViolation::InvalidGroupRestoration(node.id()));
                }
            }

            let mut unique_children = BTreeSet::new();
            for child_id in &node.children {
                if !unique_children.insert(*child_id) {
                    return Err(InvariantViolation::DuplicateChild {
                        parent: node.id(),
                        child: *child_id,
                    });
                }
                hierarchy_edges.insert((node.id(), *child_id));
                let child = self
                    .nodes
                    .get(child_id)
                    .ok_or(InvariantViolation::DanglingChild {
                        parent: node.id(),
                        child: *child_id,
                    })?;
                if child.parent != Some(node.id()) {
                    return Err(InvariantViolation::ParentChildMismatch {
                        parent: node.id(),
                        child: *child_id,
                    });
                }
            }

            if let Some(parent_id) = node.parent {
                let parent =
                    self.nodes
                        .get(&parent_id)
                        .ok_or(InvariantViolation::DanglingParent {
                            node: node.id(),
                            parent: parent_id,
                        })?;
                if !parent.spec.kind.can_contain_children() {
                    return Err(InvariantViolation::InvalidContainer(parent_id));
                }
            }
        }

        for node in self.nodes.values() {
            if let Some(parent) = node.parent {
                if !hierarchy_edges.contains(&(parent, node.id())) {
                    return Err(InvariantViolation::ParentChildMismatch {
                        parent,
                        child: node.id(),
                    });
                }
            }
        }

        // Parent-link cycle detection is iterative and visits each node at most once. This
        // keeps validated construction linear in hierarchy depth for BENCH-D.
        let mut visit_state = BTreeMap::new();
        for node_id in self.nodes.keys().copied() {
            visit_state.insert(node_id, 0_u8);
        }
        for start in self.nodes.keys().copied() {
            if visit_state[&start] == 2 {
                continue;
            }
            let mut path = Vec::new();
            let mut current = Some(start);
            while let Some(node_id) = current {
                match visit_state[&node_id] {
                    2 => break,
                    1 => return Err(InvariantViolation::Cycle(node_id)),
                    _ => {
                        visit_state.insert(node_id, 1);
                        path.push(node_id);
                        current = self.nodes[&node_id].parent;
                    }
                }
            }
            for node_id in path {
                visit_state.insert(node_id, 2);
            }
        }
        Ok(())
    }

    fn reparent_internal(
        &mut self,
        child: NodeId,
        new_parent: NodeId,
        index: usize,
        replacement_transform: Option<Affine2>,
    ) -> Result<(), DocumentError> {
        self.validate_attachment(new_parent, child)?;
        let old_parent = self.required_node(child)?.parent;

        let final_length = self.required_node(new_parent)?.children.len()
            - usize::from(old_parent == Some(new_parent));
        if index > final_length {
            return Err(DocumentError::ChildIndexOutOfBounds {
                parent: new_parent,
                index,
                length: final_length,
            });
        }

        if let Some(transform) = replacement_transform {
            if !transform.is_finite() {
                return Err(DocumentError::NonFiniteDerivedTransform(child));
            }
        }

        if let Some(parent) = old_parent {
            self.remove_child_reference(parent, child)?;
        }
        let work = self.node_mut(new_parent)?.children.insert(index, child);
        self.sequence_work += work;
        let child_node = self.node_mut(child)?;
        child_node.parent = Some(new_parent);
        if let Some(transform) = replacement_transform {
            child_node.spec.local_transform = transform;
        }
        Ok(())
    }

    fn validate_attachment(&self, parent: NodeId, child: NodeId) -> Result<(), DocumentError> {
        let parent_node = self.required_node(parent)?;
        self.required_node(child)?;
        if child == self.root_id {
            return Err(DocumentError::RootOperation(child));
        }
        if !parent_node.spec.kind.can_contain_children() {
            return Err(DocumentError::InvalidParent(parent));
        }

        let mut current = Some(parent);
        let mut visited = BTreeSet::new();
        while let Some(node_id) = current {
            if node_id == child {
                return Err(DocumentError::CycleDetected { parent, child });
            }
            if !visited.insert(node_id) {
                return Err(InvariantViolation::Cycle(node_id).into());
            }
            current = self.required_node(node_id)?.parent;
        }
        Ok(())
    }

    fn collect_subtree_preorder(&self, id: NodeId) -> Result<Vec<NodeId>, DocumentError> {
        let mut output = Vec::new();
        let mut stack = vec![id];
        let mut visited = BTreeSet::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                return Err(InvariantViolation::Cycle(current).into());
            }
            let node = self.required_node(current)?;
            output.push(current);
            stack.extend(node.children.iter().rev().copied());
        }

        Ok(output)
    }

    fn remove_child_reference(
        &mut self,
        parent: NodeId,
        child: NodeId,
    ) -> Result<(), DocumentError> {
        let mut work = SequenceWork::default();
        let parent_node = self.node_mut(parent)?;
        let position = parent_node
            .children
            .rank_of_tracked(child, &mut work)
            .ok_or(InvariantViolation::ParentChildMismatch { parent, child })?;
        parent_node
            .children
            .remove_at_tracked(position, &mut work)
            .expect("validated child rank");
        self.sequence_work += work;
        Ok(())
    }

    fn required_node(&self, id: NodeId) -> Result<&Node, DocumentError> {
        self.nodes.get(&id).ok_or(DocumentError::NodeNotFound(id))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, DocumentError> {
        self.nodes
            .get_mut(&id)
            .ok_or(DocumentError::NodeNotFound(id))
    }
}

fn validate_node_spec(spec: &NodeSpec) -> Result<(), DocumentError> {
    if !spec.local_transform.is_finite() {
        return Err(DocumentError::InvalidTransform(spec.id));
    }
    if !geometry_matches(spec) {
        return Err(DocumentError::InvalidGeometry(spec.id));
    }
    if !appearance_is_valid(spec.appearance) {
        return Err(DocumentError::InvalidAppearance(spec.id));
    }
    Ok(())
}

fn validate_spec_invariant(spec: &NodeSpec) -> Result<(), InvariantViolation> {
    if !spec.local_transform.is_finite() {
        return Err(InvariantViolation::NonFiniteTransform(spec.id));
    }
    if !geometry_matches(spec) {
        return Err(InvariantViolation::InvalidGeometry(spec.id));
    }
    if !appearance_is_valid(spec.appearance) {
        return Err(InvariantViolation::InvalidAppearance(spec.id));
    }
    Ok(())
}

fn geometry_matches(spec: &NodeSpec) -> bool {
    let size_is_valid = |size: Vec2| size.is_finite() && size.x >= 0.0 && size.y >= 0.0;
    match (spec.kind, &spec.geometry) {
        (NodeKind::Document | NodeKind::Group, None) => true,
        (NodeKind::Frame, Some(Geometry::Frame { size })) => {
            size.is_finite() && size.x > 0.0 && size.y > 0.0
        }
        (NodeKind::Rectangle, Some(Geometry::Rectangle { size }))
        | (NodeKind::Ellipse, Some(Geometry::Ellipse { size })) => size_is_valid(*size),
        _ => false,
    }
}

fn appearance_is_valid(appearance: Appearance) -> bool {
    appearance.fill.is_valid()
        && appearance.opacity.is_finite()
        && (0.0..=1.0).contains(&appearance.opacity)
        && appearance.corner_radii.is_valid()
        && appearance.stroke.color.is_valid()
        && appearance.stroke.width.is_finite()
        && appearance.stroke.width >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register_group(document: &mut Document, name: &str) -> NodeId {
        let id = NodeId::new();
        document
            .register_node(NodeSpec::group(id, name))
            .expect("group registration should succeed");
        id
    }

    #[test]
    fn generated_ids_are_unique_and_map_friendly() {
        let ids: BTreeSet<_> = (0..1_000).map(|_| NodeId::new()).collect();
        assert_eq!(ids.len(), 1_000);
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut document = Document::new("Root");
        let id = NodeId::new();
        document
            .register_node(NodeSpec::group(id, "First"))
            .unwrap();
        assert_eq!(
            document.register_node(NodeSpec::group(id, "Duplicate")),
            Err(DocumentError::DuplicateNodeId(id))
        );
    }

    #[test]
    fn attach_detach_and_child_order_are_explicit() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let first = register_group(&mut document, "First");
        let second = register_group(&mut document, "Second");
        document.attach_child(root, second, 0).unwrap();
        document.attach_child(root, first, 0).unwrap();
        assert_eq!(document.node(root).unwrap().children(), &[first, second]);

        document.detach(first).unwrap();
        assert_eq!(document.node(first).unwrap().parent(), None);
        assert_eq!(document.node(root).unwrap().children(), &[second]);
        document.validate_invariants().unwrap();
    }

    #[test]
    fn reparent_within_the_same_parent_reorders_children() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let first = register_group(&mut document, "First");
        let second = register_group(&mut document, "Second");
        let third = register_group(&mut document, "Third");
        document.attach_child(root, first, 0).unwrap();
        document.attach_child(root, second, 1).unwrap();
        document.attach_child(root, third, 2).unwrap();

        document.reparent(first, root, 2).unwrap();

        assert_eq!(
            document.node(root).unwrap().children(),
            &[second, third, first]
        );
        assert_eq!(document.node(first).unwrap().parent(), Some(root));
        document.validate_invariants().unwrap();
    }

    #[test]
    fn cycles_are_prevented_without_partial_mutation() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let parent = register_group(&mut document, "Parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, parent, 0).unwrap();
        document.attach_child(parent, child, 0).unwrap();

        assert_eq!(
            document.reparent(parent, child, 0),
            Err(DocumentError::CycleDetected {
                parent: child,
                child: parent
            })
        );
        assert_eq!(document.node(parent).unwrap().parent(), Some(root));
        assert_eq!(document.node(child).unwrap().parent(), Some(parent));
    }

    #[test]
    fn delete_semantics_remove_the_complete_subtree() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let parent = register_group(&mut document, "Parent");
        let child = register_group(&mut document, "Child");
        let sibling = register_group(&mut document, "Sibling");
        document.attach_child(root, parent, 0).unwrap();
        document.attach_child(parent, child, 0).unwrap();
        document.attach_child(root, sibling, 1).unwrap();

        assert_eq!(
            document.delete_subtree(parent).unwrap(),
            vec![parent, child]
        );
        assert!(document.node(parent).is_none());
        assert!(document.node(child).is_none());
        assert_eq!(document.node(root).unwrap().children(), &[sibling]);
        document.validate_invariants().unwrap();
    }

    #[test]
    fn deep_subtree_deletion_is_iterative_and_preserves_siblings() {
        const DEPTH: usize = 20_000;

        let mut document = Document::new("Root");
        let root = document.root_id();
        let first = register_group(&mut document, "Level 0");
        let sibling = register_group(&mut document, "Sibling");
        document.attach_child(root, first, 0).unwrap();
        document.attach_child(root, sibling, 1).unwrap();

        let mut expected = Vec::with_capacity(DEPTH);
        expected.push(first);
        let mut parent = first;
        for index in 1..DEPTH {
            let child = register_group(&mut document, &format!("Level {index}"));
            document
                .nodes
                .get_mut(&parent)
                .unwrap()
                .children
                .push(child);
            document.nodes.get_mut(&child).unwrap().parent = Some(parent);
            expected.push(child);
            parent = child;
        }

        let removed = document.delete_subtree(first).unwrap();

        assert_eq!(removed, expected);
        assert_eq!(document.node(root).unwrap().children(), &[sibling]);
        assert_eq!(document.len(), 2);
        document.validate_invariants().unwrap();
    }

    #[test]
    fn nested_world_transform_uses_parent_times_local() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let parent = register_group(&mut document, "Parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, parent, 0).unwrap();
        document.attach_child(parent, child, 0).unwrap();
        document
            .set_local_transform(
                parent,
                Affine2::translation(Vec2::new(10.0, 20.0))
                    * Affine2::rotation(std::f64::consts::FRAC_PI_2),
            )
            .unwrap();
        document
            .set_local_transform(child, Affine2::scale(Vec2::new(2.0, 3.0)))
            .unwrap();

        let expected = document.node(parent).unwrap().local_transform()
            * document.node(child).unwrap().local_transform();
        assert!(document
            .world_transform(child)
            .unwrap()
            .approx_eq(expected, DEFAULT_EPSILON));
    }

    #[test]
    fn ten_level_hierarchy_resolves_correctly() {
        let mut document = Document::new("Root");
        let mut parent = document.root_id();
        let mut expected = Affine2::IDENTITY;

        for index in 0..10 {
            let child = register_group(&mut document, &format!("Level {index}"));
            document.attach_child(parent, child, 0).unwrap();
            let local = Affine2::translation(Vec2::new(index as f64 + 1.0, -2.0))
                * Affine2::rotation(0.03 * index as f64)
                * Affine2::scale(Vec2::new(1.01, 0.99));
            document.set_local_transform(child, local).unwrap();
            expected = expected * local;
            parent = child;
        }

        assert!(document
            .world_transform(parent)
            .unwrap()
            .approx_eq(expected, 1.0e-8));
    }

    #[test]
    fn nested_finite_local_transforms_report_composition_overflow() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let parent = register_group(&mut document, "Parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, parent, 0).unwrap();
        document.attach_child(parent, child, 0).unwrap();
        document
            .set_local_transform(parent, Affine2::scale(Vec2::new(f64::MAX, 1.0)))
            .unwrap();
        document
            .set_local_transform(child, Affine2::scale(Vec2::new(2.0, 1.0)))
            .unwrap();

        assert_eq!(
            document.world_transform(child),
            Err(DocumentError::NonFiniteDerivedTransform(child))
        );
        assert_eq!(document.node(child).unwrap().parent(), Some(parent));
    }

    #[test]
    fn local_world_local_round_trip_handles_rotation_and_non_uniform_scale() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let parent = register_group(&mut document, "Parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, parent, 0).unwrap();
        document.attach_child(parent, child, 0).unwrap();
        document
            .set_local_transform(
                parent,
                Affine2::translation(Vec2::new(300.0, -120.0))
                    * Affine2::rotation(0.72)
                    * Affine2::scale(Vec2::new(2.5, 0.4)),
            )
            .unwrap();
        document
            .set_local_transform(
                child,
                Affine2::translation(Vec2::new(-15.0, 9.0))
                    * Affine2::rotation(-0.31)
                    * Affine2::scale(Vec2::new(-1.5, 3.0)),
            )
            .unwrap();

        let local = Vec2::new(8.0, -5.0);
        let world = document.local_to_world(child, local).unwrap();
        let round_trip = document.world_to_local(child, world).unwrap();
        assert!(round_trip.approx_eq(local, 1.0e-8));
    }

    #[test]
    fn non_finite_coordinate_results_are_typed_errors() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let node = register_group(&mut document, "Node");
        document.attach_child(root, node, 0).unwrap();
        document
            .set_local_transform(node, Affine2::scale(Vec2::new(f64::MAX, 1.0)))
            .unwrap();

        assert_eq!(
            document.local_to_world(node, Vec2::new(2.0, 0.0)),
            Err(DocumentError::NonFiniteCoordinate(node))
        );

        document
            .set_local_transform(node, Affine2::scale(Vec2::new(1.0e-150, 1.0e-150)))
            .unwrap();
        assert_eq!(
            document.world_to_local(node, Vec2::new(1.0e200, 0.0)),
            Err(DocumentError::NonFiniteCoordinate(node))
        );
    }

    #[test]
    fn reparent_can_preserve_world_transform() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let first_parent = register_group(&mut document, "First parent");
        let second_parent = register_group(&mut document, "Second parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, first_parent, 0).unwrap();
        document.attach_child(root, second_parent, 1).unwrap();
        document.attach_child(first_parent, child, 0).unwrap();
        document
            .set_local_transform(
                first_parent,
                Affine2::translation(Vec2::new(20.0, 30.0))
                    * Affine2::rotation(0.5)
                    * Affine2::scale(Vec2::new(2.0, 0.5)),
            )
            .unwrap();
        document
            .set_local_transform(
                second_parent,
                Affine2::translation(Vec2::new(-90.0, 13.0))
                    * Affine2::rotation(-0.2)
                    * Affine2::scale(Vec2::new(-1.2, 2.1)),
            )
            .unwrap();
        document
            .set_local_transform(
                child,
                Affine2::translation(Vec2::new(4.0, 5.0)) * Affine2::rotation(0.8),
            )
            .unwrap();
        let before = document.world_transform(child).unwrap();

        document
            .reparent_preserving_world(child, second_parent, 0)
            .unwrap();
        let after = document.world_transform(child).unwrap();
        assert!(after.approx_eq(before, 1.0e-8));
        assert_eq!(document.node(child).unwrap().parent(), Some(second_parent));
    }

    #[test]
    fn failed_world_preserving_reparent_is_atomic() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let new_parent = register_group(&mut document, "New parent");
        let child = register_group(&mut document, "Child");
        document.attach_child(root, new_parent, 0).unwrap();
        document.attach_child(root, child, 1).unwrap();
        document
            .set_local_transform(new_parent, Affine2::scale(Vec2::new(1.0e-150, 1.0e-150)))
            .unwrap();
        document
            .set_local_transform(child, Affine2::translation(Vec2::new(f64::MAX, 0.0)))
            .unwrap();
        let before = document.snapshot();

        assert_eq!(
            document.reparent_preserving_world(child, new_parent, 0),
            Err(DocumentError::NonFiniteDerivedTransform(child))
        );
        assert_eq!(document.snapshot(), before);
    }

    #[test]
    fn singular_world_transform_has_recoverable_failure() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let node = register_group(&mut document, "Singular");
        document.attach_child(root, node, 0).unwrap();
        document
            .set_local_transform(node, Affine2::scale(Vec2::new(1.0, 0.0)))
            .unwrap();
        assert_eq!(
            document.world_to_local(node, Vec2::ZERO),
            Err(DocumentError::SingularWorldTransform(node))
        );
    }

    #[test]
    fn rectangle_and_ellipse_world_bounds_are_geometry_aware() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let rectangle = NodeId::new();
        let ellipse = NodeId::new();
        document
            .create_node(
                NodeSpec::rectangle(rectangle, "Rectangle", Vec2::new(100.0, 50.0)),
                root,
                0,
            )
            .unwrap();
        document
            .create_node(
                NodeSpec::ellipse(ellipse, "Ellipse", Vec2::new(100.0, 50.0)),
                root,
                1,
            )
            .unwrap();
        let transform = Affine2::translation(Vec2::new(10.0, 20.0))
            * Affine2::rotation(std::f64::consts::FRAC_PI_2);
        document.set_local_transform(rectangle, transform).unwrap();
        document.set_local_transform(ellipse, transform).unwrap();

        let rectangle_bounds = document.world_bounds(rectangle).unwrap().unwrap();
        assert!(rectangle_bounds.approx_eq(
            Rect::from_min_max(Vec2::new(-40.0, 20.0), Vec2::new(10.0, 120.0)),
            1.0e-8
        ));
        let ellipse_bounds = document.world_bounds(ellipse).unwrap().unwrap();
        assert!(ellipse_bounds.approx_eq(rectangle_bounds, 1.0e-8));
    }

    #[test]
    fn non_orthogonal_ellipse_bounds_are_tighter_than_rectangle_bounds() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let rectangle = NodeId::new();
        let ellipse = NodeId::new();
        let size = Vec2::new(100.0, 50.0);
        document
            .create_node(NodeSpec::rectangle(rectangle, "Rectangle", size), root, 0)
            .unwrap();
        document
            .create_node(NodeSpec::ellipse(ellipse, "Ellipse", size), root, 1)
            .unwrap();
        let transform = Affine2::translation(Vec2::new(30.0, -20.0))
            * Affine2::rotation(std::f64::consts::FRAC_PI_6);
        document.set_local_transform(rectangle, transform).unwrap();
        document.set_local_transform(ellipse, transform).unwrap();

        let rectangle_bounds = document.world_bounds(rectangle).unwrap().unwrap();
        let ellipse_bounds = document.world_bounds(ellipse).unwrap().unwrap();
        let center = transform.transform_point(size * 0.5);
        let radii = size * 0.5;
        let extent = Vec2::new(
            (transform.m11 * radii.x).hypot(transform.m12 * radii.y),
            (transform.m21 * radii.x).hypot(transform.m22 * radii.y),
        );
        let expected = Rect::from_min_max(center - extent, center + extent);

        assert!(ellipse_bounds.approx_eq(expected, 1.0e-9));
        assert!(ellipse_bounds.width() < rectangle_bounds.width());
        assert!(ellipse_bounds.height() < rectangle_bounds.height());
    }

    #[test]
    fn ellipse_bounds_avoid_intermediate_square_overflow() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let ellipse = NodeId::new();
        document
            .create_node(
                NodeSpec::ellipse(ellipse, "Huge ellipse", Vec2::new(2.0e100, 1.0e100)),
                root,
                0,
            )
            .unwrap();
        document
            .set_local_transform(ellipse, Affine2::scale(Vec2::new(1.0e200, 1.0e200)))
            .unwrap();

        let bounds = document.world_bounds(ellipse).unwrap().unwrap();
        assert!(bounds.is_finite());
        assert!(bounds.approx_eq(
            Rect::from_min_max(Vec2::ZERO, Vec2::new(2.0e300, 1.0e300)),
            1.0e-9
        ));
    }

    #[test]
    fn non_finite_rectangle_and_ellipse_bounds_are_typed_errors() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let rectangle = NodeId::new();
        let ellipse = NodeId::new();
        document
            .create_node(
                NodeSpec::rectangle(rectangle, "Rectangle", Vec2::new(4.0, 4.0)),
                root,
                0,
            )
            .unwrap();
        document
            .create_node(
                NodeSpec::ellipse(ellipse, "Ellipse", Vec2::new(4.0, 4.0)),
                root,
                1,
            )
            .unwrap();
        let huge_scale = Affine2::scale(Vec2::new(f64::MAX, f64::MAX));
        document.set_local_transform(rectangle, huge_scale).unwrap();
        document.set_local_transform(ellipse, huge_scale).unwrap();

        assert_eq!(
            document.world_bounds(rectangle),
            Err(DocumentError::NonFiniteBounds(rectangle))
        );
        assert_eq!(
            document.world_bounds(ellipse),
            Err(DocumentError::NonFiniteBounds(ellipse))
        );
    }

    #[test]
    fn invariant_checker_catches_deliberately_corrupted_fixture() {
        let mut document = Document::new("Root");
        let root = document.root_id();
        let child = register_group(&mut document, "Child");
        document.attach_child(root, child, 0).unwrap();

        // Tests in this module can reach private state solely to prove the checker.
        document.nodes.get_mut(&child).unwrap().parent = None;
        assert_eq!(
            document.validate_invariants(),
            Err(InvariantViolation::ParentChildMismatch {
                parent: root,
                child
            })
        );
    }

    #[test]
    fn invalid_geometry_is_rejected_at_registration() {
        let mut document = Document::new("Root");
        let id = NodeId::new();
        assert_eq!(
            document.register_node(NodeSpec::rectangle(id, "Invalid", Vec2::new(-1.0, 50.0))),
            Err(DocumentError::InvalidGeometry(id))
        );
    }
}
