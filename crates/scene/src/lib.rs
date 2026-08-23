//! Rebuildable computed scene, incremental dirty propagation, and indexed hit testing.

mod bounds;

use std::cell::Cell;
use std::collections::BTreeMap;

use bounds::{union, BoundsAggregate};
use thiserror::Error;
use visual_authoring_core_math::{Affine2, Rect, Vec2};
use visual_authoring_document::{
    Appearance, Document, DocumentChange, DocumentChangeSet, Geometry, InvariantViolation, NodeId,
    OrderSequence, SequenceWork, StructuralGroupChange,
};
use visual_authoring_spatial::{RTreeIndex, SpatialError, SpatialIndex};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtyCategories {
    pub transform: bool,
    pub geometry: bool,
    pub appearance: bool,
    pub hierarchy: bool,
    pub visibility: bool,
}

impl DirtyCategories {
    const fn transform() -> Self {
        Self {
            transform: true,
            geometry: false,
            appearance: false,
            hierarchy: false,
            visibility: false,
        }
    }

    const fn geometry() -> Self {
        Self {
            transform: false,
            geometry: true,
            appearance: false,
            hierarchy: false,
            visibility: false,
        }
    }

    const fn appearance() -> Self {
        Self {
            transform: false,
            geometry: false,
            appearance: true,
            hierarchy: false,
            visibility: false,
        }
    }

    const fn hierarchy() -> Self {
        Self {
            transform: true,
            geometry: true,
            appearance: false,
            hierarchy: true,
            visibility: true,
        }
    }

    const fn visibility() -> Self {
        Self {
            transform: false,
            geometry: false,
            appearance: false,
            hierarchy: false,
            visibility: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDerivedState {
    NonFiniteWorldTransform,
    NonFiniteWorldBounds,
    InvalidAncestorWorldTransform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    id: NodeId,
    parent: Option<NodeId>,
    children: OrderSequence,
    attached: bool,
    effective_visible: bool,
    world_transform: Option<Affine2>,
    invalid: Option<InvalidDerivedState>,
    own_world_bounds: Option<Rect>,
    subtree_world_bounds: Option<Rect>,
    indexed: bool,
    last_dirty: DirtyCategories,
    last_changed_revision: u64,
    aggregate: BoundsAggregate,
}

impl SceneNode {
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
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
    pub const fn attached(&self) -> bool {
        self.attached
    }

    #[must_use]
    pub const fn effective_visible(&self) -> bool {
        self.effective_visible
    }

    #[must_use]
    pub const fn world_transform(&self) -> Option<Affine2> {
        self.world_transform
    }

    #[must_use]
    pub const fn invalid(&self) -> Option<InvalidDerivedState> {
        self.invalid
    }

    #[must_use]
    pub const fn own_world_bounds(&self) -> Option<Rect> {
        self.own_world_bounds
    }

    #[must_use]
    pub const fn subtree_world_bounds(&self) -> Option<Rect> {
        self.subtree_world_bounds
    }

    #[must_use]
    pub const fn indexed(&self) -> bool {
        self.indexed
    }

    #[must_use]
    pub const fn last_dirty(&self) -> DirtyCategories {
        self.last_dirty
    }

    #[must_use]
    pub const fn last_changed_revision(&self) -> u64 {
        self.last_changed_revision
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneNodeSnapshot {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub sibling_index: usize,
    pub attached: bool,
    pub effective_visible: bool,
    pub world_transform: Option<Affine2>,
    pub invalid: Option<InvalidDerivedState>,
    pub own_world_bounds: Option<Rect>,
    pub subtree_world_bounds: Option<Rect>,
    pub indexed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneUpdateStats {
    pub dirty_nodes: u64,
    pub visited_scene_nodes: u64,
    pub world_transforms_recomputed: u64,
    pub bounds_recomputed: u64,
    pub ancestor_aggregate_bounds_updated: u64,
    pub spatial_entries_inserted: u64,
    pub spatial_entries_removed: u64,
    pub spatial_entries_updated: u64,
    pub spatial_candidate_count: u64,
    pub exact_geometry_hit_test_count: u64,
    pub order_nodes_visited: u64,
    pub sibling_search_steps: u64,
    pub full_scene_rebuild_count: u64,
    pub fallback_rebuild_count: u64,
    pub sequence_work: SequenceWork,
    pub document_revision: u64,
    pub scene_revision: u64,
}

impl SceneUpdateStats {
    fn accumulate(&mut self, update: Self) {
        self.dirty_nodes += update.dirty_nodes;
        self.visited_scene_nodes += update.visited_scene_nodes;
        self.world_transforms_recomputed += update.world_transforms_recomputed;
        self.bounds_recomputed += update.bounds_recomputed;
        self.ancestor_aggregate_bounds_updated += update.ancestor_aggregate_bounds_updated;
        self.spatial_entries_inserted += update.spatial_entries_inserted;
        self.spatial_entries_removed += update.spatial_entries_removed;
        self.spatial_entries_updated += update.spatial_entries_updated;
        self.spatial_candidate_count += update.spatial_candidate_count;
        self.exact_geometry_hit_test_count += update.exact_geometry_hit_test_count;
        self.order_nodes_visited += update.order_nodes_visited;
        self.sibling_search_steps += update.sibling_search_steps;
        self.full_scene_rebuild_count += update.full_scene_rebuild_count;
        self.fallback_rebuild_count += update.fallback_rebuild_count;
        self.sequence_work += update.sequence_work;
        self.document_revision = update.document_revision;
        self.scene_revision = update.scene_revision;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SceneMetrics {
    pub total_document_nodes: u64,
    pub attached_scene_nodes: u64,
    pub effective_visible_nodes: u64,
    pub invalid_derived_nodes: u64,
    pub indexed_nodes: u64,
    pub last_update: SceneUpdateStats,
    pub cumulative: SceneUpdateStats,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SceneError {
    #[error("document is invalid: {0}")]
    InvalidDocument(InvariantViolation),
    #[error("scene record {0} was not found")]
    MissingSceneNode(NodeId),
    #[error("document node {0} was not found")]
    MissingDocumentNode(NodeId),
    #[error("scene revision {scene} does not match change-set base revision {document}")]
    RevisionMismatch { scene: u64, document: u64 },
    #[error("spatial update failed: {0}")]
    Spatial(#[from] SpatialError),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SceneQueryResult {
    ids: Vec<NodeId>,
    candidate_count: usize,
    order_nodes_visited: u64,
    sibling_search_steps: u64,
}

impl SceneQueryResult {
    #[must_use]
    pub fn ids(&self) -> &[NodeId] {
        &self.ids
    }

    #[must_use]
    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    #[must_use]
    pub const fn order_nodes_visited(&self) -> u64 {
        self.order_nodes_visited
    }

    #[must_use]
    pub const fn sibling_search_steps(&self) -> u64 {
        self.sibling_search_steps
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HitTestResult {
    topmost: Option<NodeId>,
    all: Vec<NodeId>,
    candidate_count: usize,
    exact_geometry_test_count: usize,
}

impl HitTestResult {
    #[must_use]
    pub const fn topmost(&self) -> Option<NodeId> {
        self.topmost
    }

    #[must_use]
    pub fn all(&self) -> &[NodeId] {
        &self.all
    }

    #[must_use]
    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    #[must_use]
    pub const fn exact_geometry_test_count(&self) -> usize {
        self.exact_geometry_test_count
    }
}

#[derive(Clone, Copy, Debug)]
struct PlacementUpdate {
    before: Option<visual_authoring_document::NodePlacement>,
    after: Option<visual_authoring_document::NodePlacement>,
    local_transform_changed: bool,
}

#[derive(Clone, Debug)]
struct DerivedValues {
    parent: Option<NodeId>,
    attached: bool,
    effective_visible: bool,
    world_transform: Option<Affine2>,
    invalid: Option<InvalidDerivedState>,
    own_world_bounds: Option<Rect>,
}

/// Runtime cache fully derivable from a persistent Document.
#[derive(Clone, Debug)]
pub struct ComputedScene {
    root_id: NodeId,
    nodes: BTreeMap<NodeId, SceneNode>,
    spatial: RTreeIndex,
    scene_revision: u64,
    metrics: SceneMetrics,
}

impl ComputedScene {
    pub fn build(document: &Document, revision: u64) -> Result<Self, SceneError> {
        document
            .validate_invariants()
            .map_err(SceneError::InvalidDocument)?;
        let mut scene = Self {
            root_id: document.root_id(),
            nodes: BTreeMap::new(),
            spatial: RTreeIndex::default(),
            scene_revision: revision,
            metrics: SceneMetrics::default(),
        };
        let mut roots = vec![document.root_id()];
        roots.extend(
            document
                .nodes()
                .filter(|node| node.parent().is_none() && node.id() != document.root_id())
                .map(|node| node.id()),
        );
        let mut stack: Vec<_> = roots.into_iter().rev().collect();
        let mut order = Vec::with_capacity(document.len());
        let mut construction_work = SequenceWork::default();
        while let Some(id) = stack.pop() {
            let derived = scene.derive_values(document, id)?;
            let document_node = document
                .node(id)
                .ok_or(SceneError::MissingDocumentNode(id))?;
            let node = SceneNode {
                id,
                parent: derived.parent,
                children: OrderSequence::from_ids_tracked(
                    document_node.children().iter().copied(),
                    &mut construction_work,
                ),
                attached: derived.attached,
                effective_visible: derived.effective_visible,
                world_transform: derived.world_transform,
                invalid: derived.invalid,
                own_world_bounds: derived.own_world_bounds,
                subtree_world_bounds: derived.own_world_bounds,
                indexed: false,
                last_dirty: DirtyCategories::hierarchy(),
                last_changed_revision: revision,
                aggregate: BoundsAggregate::default(),
            };
            stack.extend(document_node.children().iter().rev().copied());
            scene.nodes.insert(id, node);
            order.push(id);
        }
        let mut initial_aggregate_updates = 0_u64;
        for id in order.iter().rev() {
            let children = scene.nodes[id].children.clone();
            for child in children.iter().copied() {
                let child_bounds = scene.nodes[&child].subtree_world_bounds;
                if scene
                    .nodes
                    .get_mut(id)
                    .expect("record exists")
                    .aggregate
                    .set(child, child_bounds)
                {
                    initial_aggregate_updates += 1;
                }
            }
            let child_bounds = scene.nodes[id].aggregate.bounds();
            let own = scene.nodes[id].own_world_bounds;
            scene
                .nodes
                .get_mut(id)
                .expect("record exists")
                .subtree_world_bounds = union(own, child_bounds);
        }
        let mut stats = SceneUpdateStats {
            dirty_nodes: document.len() as u64,
            visited_scene_nodes: document.len() as u64,
            world_transforms_recomputed: document.len() as u64,
            bounds_recomputed: document.len() as u64,
            ancestor_aggregate_bounds_updated: initial_aggregate_updates,
            full_scene_rebuild_count: 1,
            sequence_work: construction_work,
            document_revision: revision,
            scene_revision: revision,
            ..SceneUpdateStats::default()
        };
        for id in order {
            let desired = scene.desired_spatial_bounds(id);
            if let Some(bounds) = desired {
                scene.spatial.insert(id, bounds)?;
                scene.nodes.get_mut(&id).expect("record exists").indexed = true;
                stats.spatial_entries_inserted += 1;
            }
        }
        scene.metrics.total_document_nodes = scene.nodes.len() as u64;
        scene.metrics.attached_scene_nodes =
            scene.nodes.values().filter(|node| node.attached).count() as u64;
        scene.metrics.effective_visible_nodes = scene
            .nodes
            .values()
            .filter(|node| node.effective_visible)
            .count() as u64;
        scene.metrics.invalid_derived_nodes = scene
            .nodes
            .values()
            .filter(|node| node.invalid.is_some())
            .count() as u64;
        scene.metrics.indexed_nodes = scene.spatial.entry_count() as u64;
        scene.finish_stats(stats);
        Ok(scene)
    }

    /// Rebuilds after an unexpected incremental-sync failure and records the recovery path.
    pub fn rebuild_after_failure(
        &mut self,
        document: &Document,
        revision: u64,
    ) -> Result<SceneUpdateStats, SceneError> {
        let cumulative = self.metrics.cumulative;
        let mut rebuilt = Self::build(document, revision)?;
        rebuilt.metrics.last_update.fallback_rebuild_count = 1;
        rebuilt.metrics.cumulative = cumulative;
        rebuilt
            .metrics
            .cumulative
            .accumulate(rebuilt.metrics.last_update);
        let stats = rebuilt.metrics.last_update;
        *self = rebuilt;
        Ok(stats)
    }

    #[must_use]
    pub const fn root_id(&self) -> NodeId {
        self.root_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.scene_revision
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&SceneNode> {
        self.nodes.get(&id)
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
    pub const fn metrics(&self) -> &SceneMetrics {
        &self.metrics
    }

    #[must_use]
    pub fn semantic_snapshot(&self) -> BTreeMap<NodeId, SceneNodeSnapshot> {
        self.nodes
            .values()
            .map(|node| {
                (
                    node.id,
                    SceneNodeSnapshot {
                        id: node.id,
                        parent: node.parent,
                        children: node.children.to_vec(),
                        sibling_index: node
                            .parent
                            .and_then(|parent| self.nodes.get(&parent)?.children.rank_of(node.id))
                            .unwrap_or(0),
                        attached: node.attached,
                        effective_visible: node.effective_visible,
                        world_transform: node.world_transform,
                        invalid: node.invalid,
                        own_world_bounds: node.own_world_bounds,
                        subtree_world_bounds: node.subtree_world_bounds,
                        indexed: node.indexed,
                    },
                )
            })
            .collect()
    }

    pub fn apply_changes(
        &mut self,
        document: &Document,
        change_set: &DocumentChangeSet,
    ) -> Result<SceneUpdateStats, SceneError> {
        if self.scene_revision != change_set.revision().before {
            return Err(SceneError::RevisionMismatch {
                scene: self.scene_revision,
                document: change_set.revision().before,
            });
        }
        if !change_set.changed_document() {
            let stats = SceneUpdateStats {
                document_revision: self.scene_revision,
                scene_revision: self.scene_revision,
                ..SceneUpdateStats::default()
            };
            self.finish_stats(stats);
            return Ok(stats);
        }
        if change_set
            .changes()
            .iter()
            .any(|change| matches!(change, DocumentChange::FullDocumentReset))
        {
            let cumulative = self.metrics.cumulative;
            let mut rebuilt = Self::build(document, change_set.revision().after)?;
            rebuilt.metrics.cumulative = cumulative;
            rebuilt
                .metrics
                .cumulative
                .accumulate(rebuilt.metrics.last_update);
            let stats = rebuilt.metrics.last_update;
            *self = rebuilt;
            return Ok(stats);
        }

        let target_revision = change_set.revision().after;
        let mut stats = SceneUpdateStats::default();
        for change in change_set.changes() {
            match change {
                DocumentChange::NodesInserted { root, .. } => {
                    self.insert_subtree(document, *root, target_revision, &mut stats)?;
                }
                DocumentChange::NodesRemoved {
                    root,
                    nodes,
                    placement,
                } => {
                    self.remove_subtree(document, *root, nodes, *placement, &mut stats)?;
                }
                DocumentChange::PlacementChanged {
                    root,
                    before,
                    after,
                    local_transform_changed,
                    ..
                } => self.update_placement(
                    document,
                    *root,
                    PlacementUpdate {
                        before: *before,
                        after: *after,
                        local_transform_changed: *local_transform_changed,
                    },
                    target_revision,
                    &mut stats,
                )?,
                change @ DocumentChange::StructuralGroupChanged { .. } => self
                    .apply_structural_group_change(document, change, target_revision, &mut stats)?,
                DocumentChange::LocalTransformChanged { node } => self.refresh_subtree(
                    document,
                    *node,
                    DirtyCategories::transform(),
                    target_revision,
                    &mut stats,
                )?,
                DocumentChange::GeometryChanged { node } => {
                    self.refresh_geometry(document, *node, target_revision, &mut stats)?
                }
                DocumentChange::VisibilityChanged { node } => {
                    self.refresh_visibility_subtree(document, *node, target_revision, &mut stats)?
                }
                DocumentChange::AppearanceChanged {
                    node,
                    bounds_changed,
                } => {
                    if *bounds_changed {
                        self.refresh_geometry(document, *node, target_revision, &mut stats)?;
                    } else {
                        let record = self
                            .nodes
                            .get_mut(node)
                            .ok_or(SceneError::MissingSceneNode(*node))?;
                        record.last_dirty = DirtyCategories::appearance();
                        record.last_changed_revision = target_revision;
                        stats.dirty_nodes += 1;
                        stats.visited_scene_nodes += 1;
                    }
                }
                DocumentChange::PersistentPropertyChanged { .. }
                | DocumentChange::FullDocumentReset => {}
            }
        }
        self.scene_revision = target_revision;
        stats.document_revision = target_revision;
        stats.scene_revision = target_revision;
        self.metrics.total_document_nodes = self.nodes.len() as u64;
        self.metrics.indexed_nodes = self.spatial.entry_count() as u64;
        self.finish_stats(stats);
        Ok(stats)
    }

    pub fn query_rect_candidates(&mut self, bounds: Rect) -> Result<SceneQueryResult, SceneError> {
        let query = self.spatial.query_rect(bounds)?;
        let mut ids = query.ids().to_vec();
        let (order_nodes_visited, sibling_search_steps) = self.sort_top_to_bottom(&mut ids);
        let stats = SceneUpdateStats {
            spatial_candidate_count: query.candidate_count() as u64,
            order_nodes_visited,
            sibling_search_steps,
            document_revision: self.scene_revision,
            scene_revision: self.scene_revision,
            ..SceneUpdateStats::default()
        };
        self.finish_stats(stats);
        Ok(SceneQueryResult {
            candidate_count: query.candidate_count(),
            ids,
            order_nodes_visited,
            sibling_search_steps,
        })
    }

    pub fn hit_test_world_point(
        &mut self,
        document: &Document,
        point: Vec2,
    ) -> Result<HitTestResult, SceneError> {
        let query = self.spatial.query_point(point)?;
        let mut hits = Vec::new();
        let mut exact = 0_usize;
        for id in query.ids() {
            exact += 1;
            if self.exact_hit(document, *id, point) {
                hits.push(*id);
            }
        }
        let (order_nodes_visited, sibling_search_steps) = self.sort_top_to_bottom(&mut hits);
        let stats = SceneUpdateStats {
            spatial_candidate_count: query.candidate_count() as u64,
            exact_geometry_hit_test_count: exact as u64,
            order_nodes_visited,
            sibling_search_steps,
            document_revision: self.scene_revision,
            scene_revision: self.scene_revision,
            ..SceneUpdateStats::default()
        };
        self.finish_stats(stats);
        Ok(HitTestResult {
            topmost: hits.first().copied(),
            all: hits,
            candidate_count: query.candidate_count(),
            exact_geometry_test_count: exact,
        })
    }

    fn derive_values(&self, document: &Document, id: NodeId) -> Result<DerivedValues, SceneError> {
        let node = document
            .node(id)
            .ok_or(SceneError::MissingDocumentNode(id))?;
        let (attached, parent_visible, parent_world) = if let Some(parent) = node.parent() {
            let parent_record = self
                .nodes
                .get(&parent)
                .ok_or(SceneError::MissingSceneNode(parent))?;
            (
                parent_record.attached,
                parent_record.effective_visible,
                parent_record.world_transform,
            )
        } else {
            (id == document.root_id(), true, Some(Affine2::IDENTITY))
        };
        let local = node.local_transform();
        let (world_transform, mut invalid) = if node.parent().is_none() {
            if local.is_finite() {
                (Some(local), None)
            } else {
                (None, Some(InvalidDerivedState::NonFiniteWorldTransform))
            }
        } else if let Some(parent_world) = parent_world {
            let world = parent_world * local;
            if world.is_finite() {
                (Some(world), None)
            } else {
                (None, Some(InvalidDerivedState::NonFiniteWorldTransform))
            }
        } else {
            (
                None,
                Some(InvalidDerivedState::InvalidAncestorWorldTransform),
            )
        };
        let own_world_bounds = if let Some(world) = world_transform {
            match geometry_world_bounds(node.geometry(), node.appearance(), world) {
                Ok(bounds) => bounds,
                Err(()) => {
                    invalid = Some(InvalidDerivedState::NonFiniteWorldBounds);
                    None
                }
            }
        } else {
            None
        };
        Ok(DerivedValues {
            parent: node.parent(),
            attached,
            effective_visible: attached && parent_visible && node.visible(),
            world_transform,
            invalid,
            own_world_bounds,
        })
    }

    fn desired_spatial_bounds(&self, id: NodeId) -> Option<Rect> {
        let node = self.nodes.get(&id)?;
        let bounds = node.own_world_bounds?;
        (node.attached
            && node.effective_visible
            && node.invalid.is_none()
            && bounds.width() > 0.0
            && bounds.height() > 0.0)
            .then_some(bounds)
    }

    fn refresh_subtree(
        &mut self,
        document: &Document,
        root: NodeId,
        dirty: DirtyCategories,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let node = document
                .node(id)
                .ok_or(SceneError::MissingDocumentNode(id))?;
            stack.extend(node.children().iter().rev().copied());
            order.push(id);
        }
        for id in &order {
            let old = self
                .nodes
                .get(id)
                .cloned()
                .ok_or(SceneError::MissingSceneNode(*id))?;
            let derived = self.derive_values(document, *id)?;
            self.adjust_current_totals(&old, &derived);
            {
                let record = self.nodes.get_mut(id).expect("record exists");
                record.parent = derived.parent;
                record.attached = derived.attached;
                record.effective_visible = derived.effective_visible;
                record.world_transform = derived.world_transform;
                record.invalid = derived.invalid;
                record.own_world_bounds = derived.own_world_bounds;
                record.last_dirty = dirty;
                record.last_changed_revision = revision;
            }
            self.sync_spatial(*id, old.indexed, old.own_world_bounds, stats)?;
            stats.dirty_nodes += 1;
            stats.visited_scene_nodes += 1;
            stats.world_transforms_recomputed += 1;
            stats.bounds_recomputed += 1;
        }

        for id in order.iter().rev().copied() {
            let children = self.nodes[&id].children.clone();
            for child in children.iter().copied() {
                let bounds = self.nodes[&child].subtree_world_bounds;
                if self
                    .nodes
                    .get_mut(&id)
                    .expect("record exists")
                    .aggregate
                    .set(child, bounds)
                {
                    stats.ancestor_aggregate_bounds_updated += 1;
                }
            }
            self.recompute_subtree_bound(id);
        }
        if let Some(parent) = self.nodes[&root].parent {
            self.propagate_child_bound(parent, root, stats)?;
        }
        Ok(())
    }

    fn refresh_geometry(
        &mut self,
        document: &Document,
        id: NodeId,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let old = self
            .nodes
            .get(&id)
            .cloned()
            .ok_or(SceneError::MissingSceneNode(id))?;
        let node = document
            .node(id)
            .ok_or(SceneError::MissingDocumentNode(id))?;
        let (bounds, invalid) = if let Some(world) = old.world_transform {
            match geometry_world_bounds(node.geometry(), node.appearance(), world) {
                Ok(bounds) => (bounds, None),
                Err(()) => (None, Some(InvalidDerivedState::NonFiniteWorldBounds)),
            }
        } else {
            (None, old.invalid)
        };
        let mut derived = DerivedValues {
            parent: old.parent,
            attached: old.attached,
            effective_visible: old.effective_visible,
            world_transform: old.world_transform,
            invalid,
            own_world_bounds: bounds,
        };
        if old.world_transform.is_none() {
            derived.invalid = old.invalid;
        }
        self.adjust_current_totals(&old, &derived);
        {
            let record = self.nodes.get_mut(&id).expect("record exists");
            record.own_world_bounds = bounds;
            record.invalid = derived.invalid;
            record.last_dirty = DirtyCategories::geometry();
            record.last_changed_revision = revision;
        }
        self.sync_spatial(id, old.indexed, old.own_world_bounds, stats)?;
        self.recompute_subtree_bound(id);
        if let Some(parent) = self.nodes[&id].parent {
            self.propagate_child_bound(parent, id, stats)?;
        }
        stats.dirty_nodes += 1;
        stats.visited_scene_nodes += 1;
        stats.bounds_recomputed += 1;
        Ok(())
    }

    fn refresh_visibility_subtree(
        &mut self,
        document: &Document,
        root: NodeId,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let document_node = document
                .node(id)
                .ok_or(SceneError::MissingDocumentNode(id))?;
            let old = self
                .nodes
                .get(&id)
                .cloned()
                .ok_or(SceneError::MissingSceneNode(id))?;
            let (attached, parent_visible) = if let Some(parent) = document_node.parent() {
                let parent = self
                    .nodes
                    .get(&parent)
                    .ok_or(SceneError::MissingSceneNode(parent))?;
                (parent.attached, parent.effective_visible)
            } else {
                (id == document.root_id(), true)
            };
            let effective_visible = attached && parent_visible && document_node.visible();
            if old.attached != attached {
                adjust_counter(
                    &mut self.metrics.attached_scene_nodes,
                    old.attached,
                    attached,
                );
            }
            if old.effective_visible != effective_visible {
                adjust_counter(
                    &mut self.metrics.effective_visible_nodes,
                    old.effective_visible,
                    effective_visible,
                );
            }
            {
                let record = self.nodes.get_mut(&id).expect("record exists");
                record.parent = document_node.parent();
                record.children = OrderSequence::from_ids_tracked(
                    document_node.children().iter().copied(),
                    &mut stats.sequence_work,
                );
                record.attached = attached;
                record.effective_visible = effective_visible;
                record.last_dirty = DirtyCategories::visibility();
                record.last_changed_revision = revision;
            }
            self.sync_spatial(id, old.indexed, old.own_world_bounds, stats)?;
            stack.extend(document_node.children().iter().rev().copied());
            stats.dirty_nodes += 1;
            stats.visited_scene_nodes += 1;
        }
        Ok(())
    }

    fn update_placement(
        &mut self,
        document: &Document,
        root: NodeId,
        placement: PlacementUpdate,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        if let Some(before) = placement.before {
            self.remove_child_contribution(before.parent, root, stats)?;
            let (_, work) = self
                .nodes
                .get_mut(&before.parent)
                .ok_or(SceneError::MissingSceneNode(before.parent))?
                .children
                .remove_id(root)
                .ok_or(SceneError::MissingSceneNode(root))?;
            stats.sequence_work += work;
        }
        if let Some(after) = placement.after {
            let work = self
                .nodes
                .get_mut(&after.parent)
                .ok_or(SceneError::MissingSceneNode(after.parent))?
                .children
                .insert(after.index, root);
            stats.sequence_work += work;
        }
        if placement.before.is_some_and(|before| {
            placement
                .after
                .is_some_and(|after| before.parent == after.parent)
        }) && !placement.local_transform_changed
        {
            let parent = placement.after.expect("same-parent placement").parent;
            let record = self
                .nodes
                .get_mut(&root)
                .ok_or(SceneError::MissingSceneNode(root))?;
            record.parent = Some(parent);
            record.last_dirty = DirtyCategories::hierarchy();
            record.last_changed_revision = revision;
            stats.dirty_nodes += 1;
            stats.visited_scene_nodes += 1;
            return Ok(());
        }
        self.refresh_subtree(
            document,
            root,
            DirtyCategories::hierarchy(),
            revision,
            stats,
        )
    }

    fn apply_structural_group_change(
        &mut self,
        document: &Document,
        change: &DocumentChange,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let DocumentChange::StructuralGroupChanged {
            direction,
            group,
            parent,
            index,
            children,
            positions,
        } = change
        else {
            unreachable!("structural group helper requires a structural group change");
        };
        let (direction, group, parent, index) = (*direction, *group, *parent, *index);
        match direction {
            StructuralGroupChange::Grouped => {
                let front_contiguous = positions.len() == children.len()
                    && positions.first().copied() == Some(0)
                    && positions.last().copied() == children.len().checked_sub(1);
                let derived = self.derive_values(document, group)?;
                let mut record = SceneNode {
                    id: group,
                    parent: Some(parent),
                    children: OrderSequence::from_ids_tracked(
                        children.iter().copied(),
                        &mut stats.sequence_work,
                    ),
                    attached: derived.attached,
                    effective_visible: derived.effective_visible,
                    world_transform: derived.world_transform,
                    invalid: derived.invalid,
                    own_world_bounds: derived.own_world_bounds,
                    subtree_world_bounds: derived.own_world_bounds,
                    indexed: false,
                    last_dirty: DirtyCategories::hierarchy(),
                    last_changed_revision: revision,
                    aggregate: BoundsAggregate::default(),
                };
                let mut remove_child = |offset: usize| -> Result<(), SceneError> {
                    let child = children[offset];
                    let rank = positions[offset];
                    let adjacent_before = if front_contiguous || offset == 0 {
                        false
                    } else {
                        stats.sequence_work.rank_order_comparisons += 1;
                        positions[offset - 1].checked_add(1) == Some(rank)
                    };
                    let adjacent_after = if front_contiguous || offset + 1 >= positions.len() {
                        false
                    } else {
                        stats.sequence_work.rank_order_comparisons += 1;
                        rank.checked_add(1) == Some(positions[offset + 1])
                    };
                    let prefer_predecessor = !adjacent_before && !adjacent_after;
                    self.remove_child_contribution(parent, child, stats)?;
                    stats.sequence_work.entries_examined += 1;
                    let parent_children = &mut self
                        .nodes
                        .get_mut(&parent)
                        .expect("validated scene parent")
                        .children;
                    let removed = if prefer_predecessor {
                        parent_children.remove_known_tracked_prefer_predecessor(
                            child,
                            &mut stats.sequence_work,
                        )
                    } else {
                        parent_children.remove_known_tracked(child, &mut stats.sequence_work)
                    };
                    if !removed {
                        return Err(SceneError::MissingSceneNode(child));
                    }
                    let child_record = self
                        .nodes
                        .get_mut(&child)
                        .ok_or(SceneError::MissingSceneNode(child))?;
                    child_record.parent = Some(group);
                    child_record.last_dirty = DirtyCategories::hierarchy();
                    child_record.last_changed_revision = revision;
                    record
                        .aggregate
                        .set(child, child_record.subtree_world_bounds);
                    stats.dirty_nodes += 1;
                    stats.visited_scene_nodes += 1;
                    stats.order_nodes_visited += 1;
                    Ok(())
                };
                if front_contiguous {
                    for offset in 0..children.len() {
                        remove_child(offset)?;
                    }
                } else {
                    for offset in (0..children.len()).rev() {
                        remove_child(offset)?;
                    }
                }
                let work = self
                    .nodes
                    .get_mut(&parent)
                    .expect("validated scene parent")
                    .children
                    .insert(index, group);
                stats.sequence_work += work;
                record.subtree_world_bounds =
                    union(record.own_world_bounds, record.aggregate.bounds());
                self.increment_current_for_node(&record);
                self.nodes.insert(group, record);
                let group_bounds = self.nodes[&group].subtree_world_bounds;
                self.nodes
                    .get_mut(&parent)
                    .expect("validated scene parent")
                    .aggregate
                    .set(group, group_bounds);
                self.recompute_subtree_bound(parent);
                stats.dirty_nodes += 2;
                stats.visited_scene_nodes += 2;
                stats.world_transforms_recomputed += 1;
                stats.bounds_recomputed += 1;
            }
            StructuralGroupChange::Ungrouped => {
                self.remove_child_contribution(parent, group, stats)?;
                if !self
                    .nodes
                    .get_mut(&parent)
                    .expect("validated scene parent")
                    .children
                    .remove_known_tracked(group, &mut stats.sequence_work)
                {
                    return Err(SceneError::MissingSceneNode(group));
                }
                let removed = self
                    .nodes
                    .remove(&group)
                    .ok_or(SceneError::MissingSceneNode(group))?;
                self.decrement_current_for_node(&removed);
                for (offset, child) in children.iter().copied().enumerate() {
                    let restored_index = positions.get(offset).copied().unwrap_or(index + offset);
                    let work = self
                        .nodes
                        .get_mut(&parent)
                        .expect("validated scene parent")
                        .children
                        .insert(restored_index, child);
                    stats.sequence_work += work;
                    let child_record = self
                        .nodes
                        .get_mut(&child)
                        .ok_or(SceneError::MissingSceneNode(child))?;
                    child_record.parent = Some(parent);
                    child_record.last_dirty = DirtyCategories::hierarchy();
                    child_record.last_changed_revision = revision;
                    let bounds = child_record.subtree_world_bounds;
                    self.nodes
                        .get_mut(&parent)
                        .expect("validated scene parent")
                        .aggregate
                        .set(child, bounds);
                    stats.dirty_nodes += 1;
                    stats.visited_scene_nodes += 1;
                    stats.order_nodes_visited += 1;
                }
                self.recompute_subtree_bound(parent);
                stats.dirty_nodes += 2;
                stats.visited_scene_nodes += 2;
            }
        }
        if let Some(grandparent) = self.nodes[&parent].parent {
            self.propagate_child_bound(grandparent, parent, stats)?;
        }
        Ok(())
    }

    fn insert_subtree(
        &mut self,
        document: &Document,
        root: NodeId,
        revision: u64,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let mut stack = vec![root];
        let mut order = Vec::new();
        while let Some(id) = stack.pop() {
            let derived = self.derive_values(document, id)?;
            let document_node = document
                .node(id)
                .ok_or(SceneError::MissingDocumentNode(id))?;
            stack.extend(document_node.children().iter().rev().copied());
            let record = SceneNode {
                id,
                parent: derived.parent,
                children: OrderSequence::from_ids_tracked(
                    document_node.children().iter().copied(),
                    &mut stats.sequence_work,
                ),
                attached: derived.attached,
                effective_visible: derived.effective_visible,
                world_transform: derived.world_transform,
                invalid: derived.invalid,
                own_world_bounds: derived.own_world_bounds,
                subtree_world_bounds: derived.own_world_bounds,
                indexed: false,
                last_dirty: DirtyCategories::hierarchy(),
                last_changed_revision: revision,
                aggregate: BoundsAggregate::default(),
            };
            self.increment_current_for_node(&record);
            self.nodes.insert(id, record);
            order.push(id);
            stats.dirty_nodes += 1;
            stats.visited_scene_nodes += 1;
            stats.world_transforms_recomputed += 1;
            stats.bounds_recomputed += 1;
        }
        for id in order.iter().rev().copied() {
            let children = self.nodes[&id].children.clone();
            for child in children.iter().copied() {
                let bounds = self.nodes[&child].subtree_world_bounds;
                self.nodes
                    .get_mut(&id)
                    .expect("record exists")
                    .aggregate
                    .set(child, bounds);
            }
            self.recompute_subtree_bound(id);
        }
        for id in order {
            self.sync_spatial(id, false, None, stats)?;
        }
        if let Some(parent) = self.nodes[&root].parent {
            let rank = document
                .node(parent)
                .and_then(|node| node.children().rank_of(root))
                .ok_or(SceneError::MissingDocumentNode(root))?;
            let work = self
                .nodes
                .get_mut(&parent)
                .ok_or(SceneError::MissingSceneNode(parent))?
                .children
                .insert(rank, root);
            stats.sequence_work += work;
            self.propagate_child_bound(parent, root, stats)?;
        }
        Ok(())
    }

    fn remove_subtree(
        &mut self,
        _document: &Document,
        root: NodeId,
        nodes: &[NodeId],
        placement: Option<visual_authoring_document::NodePlacement>,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        if let Some(place) = placement {
            self.remove_child_contribution(place.parent, root, stats)?;
            let (_, work) = self
                .nodes
                .get_mut(&place.parent)
                .ok_or(SceneError::MissingSceneNode(place.parent))?
                .children
                .remove_id(root)
                .ok_or(SceneError::MissingSceneNode(root))?;
            stats.sequence_work += work;
        }
        for id in nodes {
            let record = self
                .nodes
                .remove(id)
                .ok_or(SceneError::MissingSceneNode(*id))?;
            if record.indexed {
                self.spatial.remove(*id)?;
                stats.spatial_entries_removed += 1;
            }
            self.decrement_current_for_node(&record);
            stats.dirty_nodes += 1;
            stats.visited_scene_nodes += 1;
        }
        Ok(())
    }

    fn sync_spatial(
        &mut self,
        id: NodeId,
        was_indexed: bool,
        old_bounds: Option<Rect>,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let desired = self.desired_spatial_bounds(id);
        match (was_indexed, desired) {
            (true, Some(bounds)) if old_bounds != Some(bounds) => {
                self.spatial.update(id, bounds)?;
                stats.spatial_entries_updated += 1;
            }
            (true, None) => {
                self.spatial.remove(id)?;
                stats.spatial_entries_removed += 1;
            }
            (false, Some(bounds)) => {
                self.spatial.insert(id, bounds)?;
                stats.spatial_entries_inserted += 1;
            }
            _ => {}
        }
        self.nodes.get_mut(&id).expect("record exists").indexed = desired.is_some();
        Ok(())
    }

    fn recompute_subtree_bound(&mut self, id: NodeId) -> bool {
        let record = self.nodes.get(&id).expect("record exists");
        let updated = union(record.own_world_bounds, record.aggregate.bounds());
        let record = self.nodes.get_mut(&id).expect("record exists");
        let changed = record.subtree_world_bounds != updated;
        record.subtree_world_bounds = updated;
        changed
    }

    fn propagate_child_bound(
        &mut self,
        mut parent: NodeId,
        mut child: NodeId,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        loop {
            let child_bounds = self
                .nodes
                .get(&child)
                .ok_or(SceneError::MissingSceneNode(child))?
                .subtree_world_bounds;
            let old_parent_bounds = self
                .nodes
                .get(&parent)
                .ok_or(SceneError::MissingSceneNode(parent))?
                .subtree_world_bounds;
            let contribution_changed = self
                .nodes
                .get_mut(&parent)
                .expect("record exists")
                .aggregate
                .set(child, child_bounds);
            stats.visited_scene_nodes += 1;
            if contribution_changed {
                stats.ancestor_aggregate_bounds_updated += 1;
            }
            self.recompute_subtree_bound(parent);
            let new_parent_bounds = self.nodes[&parent].subtree_world_bounds;
            if new_parent_bounds == old_parent_bounds {
                break;
            }
            child = parent;
            let Some(next_parent) = self.nodes[&parent].parent else {
                break;
            };
            parent = next_parent;
        }
        Ok(())
    }

    fn remove_child_contribution(
        &mut self,
        parent: NodeId,
        child: NodeId,
        stats: &mut SceneUpdateStats,
    ) -> Result<(), SceneError> {
        let old = self
            .nodes
            .get(&parent)
            .ok_or(SceneError::MissingSceneNode(parent))?
            .subtree_world_bounds;
        if self
            .nodes
            .get_mut(&parent)
            .expect("record exists")
            .aggregate
            .remove(child)
        {
            stats.ancestor_aggregate_bounds_updated += 1;
        }
        stats.visited_scene_nodes += 1;
        self.recompute_subtree_bound(parent);
        if self.nodes[&parent].subtree_world_bounds != old {
            if let Some(grandparent) = self.nodes[&parent].parent {
                self.propagate_child_bound(grandparent, parent, stats)?;
            }
        }
        Ok(())
    }

    fn exact_hit(&self, document: &Document, id: NodeId, point: Vec2) -> bool {
        let Some(scene_node) = self.nodes.get(&id) else {
            return false;
        };
        if !scene_node.indexed {
            return false;
        }
        let Some(world) = scene_node.world_transform else {
            return false;
        };
        let Some(inverse) = world.inverse() else {
            return false;
        };
        let local = inverse.transform_point(point);
        if !local.is_finite() {
            return false;
        }
        let Some(node) = document.node(id) else {
            return false;
        };
        let Some(geometry) = node.geometry() else {
            return false;
        };
        let size = geometry.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return false;
        }
        let half_stroke = node.appearance().stroke.width * 0.5;
        match geometry {
            Geometry::Frame { .. } | Geometry::Rectangle { .. } => {
                local.x >= -half_stroke
                    && local.x <= size.x + half_stroke
                    && local.y >= -half_stroke
                    && local.y <= size.y + half_stroke
            }
            Geometry::Ellipse { .. } => ellipse_distance(local, size) <= half_stroke,
            // Path hit-testing arrives with the Phase 2A editing/rendering checkpoint.
            Geometry::Path(_) => false,
        }
    }

    fn sort_top_to_bottom(&self, ids: &mut [NodeId]) -> (u64, u64) {
        let order_nodes_visited = Cell::new(0_u64);
        ids.sort_by(|left, right| {
            self.z_order_path(*right, &order_nodes_visited)
                .cmp(&self.z_order_path(*left, &order_nodes_visited))
                .then_with(|| right.cmp(left))
        });
        (order_nodes_visited.get(), 0)
    }

    fn z_order_path(&self, id: NodeId, order_nodes_visited: &Cell<u64>) -> Vec<u128> {
        let mut path = Vec::new();
        let mut current = id;
        while let Some(node) = self.nodes.get(&current) {
            order_nodes_visited.set(order_nodes_visited.get().saturating_add(1));
            let Some(parent) = node.parent else {
                break;
            };
            let rank = self
                .nodes
                .get(&parent)
                .and_then(|parent_node| parent_node.children.rank_of(current))
                .unwrap_or(0);
            path.push(rank as u128);
            current = parent;
        }
        path.reverse();
        path
    }

    fn adjust_current_totals(&mut self, old: &SceneNode, new: &DerivedValues) {
        adjust_counter(
            &mut self.metrics.attached_scene_nodes,
            old.attached,
            new.attached,
        );
        adjust_counter(
            &mut self.metrics.effective_visible_nodes,
            old.effective_visible,
            new.effective_visible,
        );
        adjust_counter(
            &mut self.metrics.invalid_derived_nodes,
            old.invalid.is_some(),
            new.invalid.is_some(),
        );
    }

    fn increment_current_for_node(&mut self, node: &SceneNode) {
        self.metrics.total_document_nodes += 1;
        self.metrics.attached_scene_nodes += u64::from(node.attached);
        self.metrics.effective_visible_nodes += u64::from(node.effective_visible);
        self.metrics.invalid_derived_nodes += u64::from(node.invalid.is_some());
    }

    fn decrement_current_for_node(&mut self, node: &SceneNode) {
        self.metrics.total_document_nodes -= 1;
        self.metrics.attached_scene_nodes -= u64::from(node.attached);
        self.metrics.effective_visible_nodes -= u64::from(node.effective_visible);
        self.metrics.invalid_derived_nodes -= u64::from(node.invalid.is_some());
    }

    fn finish_stats(&mut self, stats: SceneUpdateStats) {
        self.metrics.last_update = stats;
        self.metrics.cumulative.accumulate(stats);
    }
}

fn adjust_counter(counter: &mut u64, old: bool, new: bool) {
    match (old, new) {
        (false, true) => *counter += 1,
        (true, false) => *counter -= 1,
        _ => {}
    }
}

fn geometry_world_bounds(
    geometry: Option<&Geometry>,
    appearance: Appearance,
    world: Affine2,
) -> Result<Option<Rect>, ()> {
    let Some(geometry) = geometry else {
        return Ok(None);
    };
    let size = geometry.size();
    let half_stroke = appearance.stroke.width * 0.5;
    let bounds = match geometry {
        Geometry::Ellipse { .. } => {
            let center = world.transform_point(size * 0.5);
            let radii = size * 0.5;
            let geometry_extent = Vec2::new(
                (world.m11 * radii.x).hypot(world.m12 * radii.y),
                (world.m21 * radii.x).hypot(world.m22 * radii.y),
            );
            let stroke_extent = Vec2::new(
                half_stroke * world.m11.hypot(world.m12),
                half_stroke * world.m21.hypot(world.m22),
            );
            let extent = geometry_extent + stroke_extent;
            if !center.is_finite() || !extent.is_finite() {
                return Err(());
            }
            Rect::from_min_max(center - extent, center + extent)
        }
        Geometry::Frame { .. } | Geometry::Rectangle { .. } => {
            world.transform_rect(Rect::from_min_max(
                Vec2::new(-half_stroke, -half_stroke),
                size + Vec2::new(half_stroke, half_stroke),
            ))
        }
        Geometry::Path(_) => {
            let local = geometry.local_bounds();
            world.transform_rect(Rect::from_min_max(
                local.min - Vec2::new(half_stroke, half_stroke),
                local.max + Vec2::new(half_stroke, half_stroke),
            ))
        }
    };
    bounds.is_finite().then_some(Some(bounds)).ok_or(())
}

fn ellipse_distance(local: Vec2, size: Vec2) -> f64 {
    let radii = size * 0.5;
    let normalized = Vec2::new(
        (local.x - size.x * 0.5) / radii.x,
        (local.y - size.y * 0.5) / radii.y,
    );
    let normalized_length = normalized.x.hypot(normalized.y);
    if normalized_length < 1.0e-12 {
        return -radii.x.min(radii.y);
    }
    let gradient_length =
        (normalized.x / radii.x).hypot(normalized.y / radii.y) / normalized_length;
    (normalized_length - 1.0) / gradient_length.max(1.0e-12)
}
