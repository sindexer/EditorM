//! Backend-neutral, rebuildable render representation with stable instance slots.

pub mod path_gpu;
pub mod path_tessellation;

pub use path_gpu::{
    encode_path_instance, encode_path_instances, encode_path_vertex, encode_path_vertices,
    PATH_INSTANCE_STRIDE_BYTES, PATH_VERTEX_STRIDE_BYTES,
};
pub use path_tessellation::{
    tessellate, PathTessellation, PathTessellationKey, PathVertex, PATH_VERTEX_KIND_FILL,
    PATH_VERTEX_KIND_STROKE,
};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use thiserror::Error;
use visual_authoring_core_math::{Affine2, Rect, Vec2};
use visual_authoring_document::{
    Document, DocumentChange, DocumentChangeSet, Geometry, NodeId, StructuralGroupChange,
};
use visual_authoring_scene::{ComputedScene, SceneError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveKind {
    Rectangle,
    Ellipse,
    /// A tessellated path. Path items carry triangles instead of an analytic primitive, so they
    /// are submitted through the path pipeline and never through the instanced quad draw.
    Path,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderEncodingDiagnosticKind {
    LinearOrGeometryOutsideF32,
    TranslationOutsideF32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderEncodingDiagnostic {
    pub node_id: NodeId,
    pub kind: RenderEncodingDiagnosticKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderItem {
    pub node_id: NodeId,
    pub slot: u32,
    pub primitive: PrimitiveKind,
    /// Derived triangles for a path item; `None` for analytic primitives.
    pub path: Option<Arc<PathTessellation>>,
    /// Compact index into the path instance array, assigned only to path items.
    pub path_index: Option<u32>,
    pub size: Vec2,
    pub world_transform: Option<Affine2>,
    pub world_bounds: Option<Rect>,
    pub fill_linear: [f64; 4],
    pub opacity: f64,
    pub corner_radii: [f64; 4],
    pub stroke_linear: [f64; 4],
    pub stroke_width: f64,
    pub renderable: bool,
    pub gpu_encodable: bool,
    pub encoding_diagnostic: Option<RenderEncodingDiagnosticKind>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderModelCounters {
    pub total_render_items: usize,
    pub renderable_items: usize,
    pub gpu_encodable_items: usize,
    pub gpu_omitted_items: usize,
    pub allocated_slots: usize,
    pub free_slots: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtySlotRange {
    pub first: u32,
    pub count: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RenderUpdateStats {
    /// Paths whose triangles were rebuilt in this update. Reusing a cached tessellation costs
    /// nothing here, so a camera move or a colour change reports zero.
    pub path_tessellations: u64,
    /// Vertices produced by those rebuilds.
    pub path_vertices_tessellated: u64,
    pub dirty_items: u64,
    pub inserted_items: u64,
    pub removed_items: u64,
    pub reused_slots: u64,
    pub dirty_slot_count: u64,
    pub dirty_range_count: u64,
    pub order_keys_updated: u64,
    pub full_render_rebuild_count: u64,
    pub render_revision: u64,
    pub document_nodes_scanned: u64,
    pub scene_nodes_visited: u64,
    pub render_items_planned: u64,
    pub render_items_read: u64,
    pub render_items_written: u64,
    pub render_items_cloned: u64,
    pub order_nodes_visited: u64,
    pub sibling_search_steps: u64,
    pub full_render_model_scans: u64,
    pub allocation_growth_count: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RenderDelta {
    pub dirty_slots: Vec<u32>,
    pub removed_slots: Vec<u32>,
    pub dirty_ranges: Vec<DirtySlotRange>,
    pub stats: RenderUpdateStats,
}

/// One ordered unit of GPU work for a frame.
///
/// Batches keep paths and analytic primitives in the same painter's order: the renderer walks
/// this list bottom to top and switches pipelines only where the scene order actually changes
/// kind, so a document without paths still submits exactly one instanced draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawBatch {
    /// A run of `count` entries of `slots_bottom_to_top`, starting at `first_visible`.
    Primitives { first_visible: u32, count: u32 },
    /// A run of path triangles, as a half-open vertex range of the path vertex buffer.
    Paths {
        first_vertex: u32,
        vertex_count: u32,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CullingResult {
    pub ids_top_to_bottom: Vec<NodeId>,
    /// Instanced primitive slots, bottom to top. Path items never appear here: they are drawn by
    /// the path pipeline from their own compact index space.
    pub slots_bottom_to_top: Vec<u32>,
    /// Compact path instance indices that survived culling, bottom to top.
    pub visible_path_indices: Vec<u32>,
    /// Pipeline-switching draw order, bottom to top.
    pub draw_batches: Vec<DrawBatch>,
    pub spatial_candidates: usize,
    pub exact_visible: usize,
    pub culled: usize,
    pub submitted_instances: usize,
    pub render_items_read: usize,
    pub gpu_encode_attempted: usize,
    pub gpu_encode_omitted: usize,
    pub order_nodes_visited: u64,
    pub sibling_search_steps: u64,
    pub full_render_model_scans: u64,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum RenderModelError {
    #[error("render revision {render} does not match change-set base {document}")]
    RevisionMismatch { render: u64, document: u64 },
    #[error("scene revision {scene} does not match requested render revision {render}")]
    SceneRevisionMismatch { scene: u64, render: u64 },
    #[error("document node {0} was not found")]
    MissingDocumentNode(NodeId),
    #[error("scene node {0} was not found")]
    MissingSceneNode(NodeId),
    #[error(transparent)]
    Scene(#[from] SceneError),
}

/// Derived render data. Document remains the only persistent truth.
#[derive(Clone, Debug)]
pub struct RenderModel {
    slots: Vec<Option<RenderItem>>,
    path_cache: PathCache,
    path_order: Vec<NodeId>,
    /// First vertex of each path in the path vertex buffer, parallel to `path_order`.
    path_vertex_offsets: Vec<u32>,
    path_vertex_total: u32,
    path_vertex_revision: u64,
    path_layout_dirty: bool,
    by_id: BTreeMap<NodeId, u32>,
    free_slots: Vec<u32>,
    render_revision: u64,
    renderable_items: usize,
    gpu_encodable_items: usize,
    diagnostics: BTreeMap<NodeId, RenderEncodingDiagnosticKind>,
    last_update: RenderUpdateStats,
    cumulative: RenderUpdateStats,
}

impl RenderModel {
    pub fn build(
        document: &Document,
        scene: &ComputedScene,
        revision: u64,
    ) -> Result<Self, RenderModelError> {
        if scene.revision() != revision {
            return Err(RenderModelError::SceneRevisionMismatch {
                scene: scene.revision(),
                render: revision,
            });
        }
        let mut model = Self {
            slots: Vec::new(),
            path_cache: PathCache::default(),
            path_order: Vec::new(),
            path_vertex_offsets: Vec::new(),
            path_vertex_total: 0,
            path_vertex_revision: 0,
            path_layout_dirty: false,
            by_id: BTreeMap::new(),
            free_slots: Vec::new(),
            render_revision: revision,
            renderable_items: 0,
            gpu_encodable_items: 0,
            diagnostics: BTreeMap::new(),
            last_update: RenderUpdateStats::default(),
            cumulative: RenderUpdateStats::default(),
        };
        let mut stats = RenderUpdateStats {
            full_render_rebuild_count: 1,
            render_revision: revision,
            document_nodes_scanned: document.len() as u64,
            full_render_model_scans: 1,
            ..RenderUpdateStats::default()
        };
        for node in document.nodes() {
            let prepared = prepare_item(
                document,
                scene,
                node.id(),
                &mut model.path_cache,
                &mut stats,
            )?;
            stats.scene_nodes_visited += 1;
            stats.render_items_planned += 1;
            model.commit_prepared(node.id(), prepared, &mut stats, &mut Vec::new());
        }
        stats.dirty_items = model.by_id.len() as u64;
        stats.dirty_slot_count = model.by_id.len() as u64;
        stats.dirty_range_count = u64::from(!model.by_id.is_empty());
        model.refresh_path_layout();
        model.finish_stats(stats);
        Ok(model)
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.render_revision
    }

    #[must_use]
    pub const fn last_update(&self) -> &RenderUpdateStats {
        &self.last_update
    }

    #[must_use]
    pub const fn cumulative(&self) -> &RenderUpdateStats {
        &self.cumulative
    }

    #[must_use]
    pub fn item(&self, id: NodeId) -> Option<&RenderItem> {
        let slot = *self.by_id.get(&id)?;
        self.slots.get(slot as usize)?.as_ref()
    }

    #[must_use]
    pub fn item_by_slot(&self, slot: u32) -> Option<&RenderItem> {
        self.slots.get(slot as usize)?.as_ref()
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn allocated_slot_count(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn free_slot_count(&self) -> usize {
        self.free_slots.len()
    }

    #[must_use]
    pub const fn renderable_count(&self) -> usize {
        self.renderable_items
    }

    #[must_use]
    pub const fn gpu_encodable_count(&self) -> usize {
        self.gpu_encodable_items
    }

    #[must_use]
    pub fn gpu_omitted_count(&self) -> usize {
        self.diagnostics.len()
    }

    #[must_use]
    pub fn counters(&self) -> RenderModelCounters {
        RenderModelCounters {
            total_render_items: self.by_id.len(),
            renderable_items: self.renderable_items,
            gpu_encodable_items: self.gpu_encodable_items,
            gpu_omitted_items: self.diagnostics.len(),
            allocated_slots: self.slots.len(),
            free_slots: self.free_slots.len(),
        }
    }

    pub fn encoding_diagnostics(&self) -> impl Iterator<Item = RenderEncodingDiagnostic> + '_ {
        self.diagnostics
            .iter()
            .map(|(node_id, kind)| RenderEncodingDiagnostic {
                node_id: *node_id,
                kind: *kind,
            })
    }

    pub fn items(&self) -> impl Iterator<Item = &RenderItem> {
        self.slots.iter().flatten()
    }

    pub fn apply_changes(
        &mut self,
        document: &Document,
        scene: &ComputedScene,
        change_set: &DocumentChangeSet,
    ) -> Result<RenderDelta, RenderModelError> {
        if self.render_revision != change_set.revision().before {
            return Err(RenderModelError::RevisionMismatch {
                render: self.render_revision,
                document: change_set.revision().before,
            });
        }
        if scene.revision() != change_set.revision().after {
            return Err(RenderModelError::SceneRevisionMismatch {
                scene: scene.revision(),
                render: change_set.revision().after,
            });
        }
        if !change_set.changed_document() {
            let stats = RenderUpdateStats {
                render_revision: self.render_revision,
                ..RenderUpdateStats::default()
            };
            self.finish_stats(stats.clone());
            return Ok(RenderDelta {
                stats,
                ..RenderDelta::default()
            });
        }
        if change_set
            .changes()
            .iter()
            .any(|change| matches!(change, DocumentChange::FullDocumentReset))
        {
            let cumulative = self.cumulative.clone();
            let mut rebuilt = Self::build(document, scene, change_set.revision().after)?;
            rebuilt.cumulative = cumulative;
            rebuilt.accumulate(rebuilt.last_update.clone());
            let dirty_slots = rebuilt.by_id.values().copied().collect::<Vec<_>>();
            let dirty_ranges = coalesce_ranges(&dirty_slots);
            let stats = rebuilt.last_update.clone();
            *self = rebuilt;
            return Ok(RenderDelta {
                dirty_slots,
                removed_slots: Vec::new(),
                dirty_ranges,
                stats,
            });
        }

        let mut dirty = BTreeSet::new();
        let mut removed = BTreeSet::new();
        for change in change_set.changes() {
            match change {
                DocumentChange::NodesInserted { nodes, .. } => {
                    dirty.extend(nodes.iter().copied());
                }
                DocumentChange::NodesRemoved { nodes, .. } => {
                    removed.extend(nodes.iter().copied());
                }
                DocumentChange::PlacementChanged { subtree, .. } => {
                    dirty.extend(subtree.iter().copied());
                }
                DocumentChange::StructuralGroupChanged {
                    direction,
                    group,
                    children,
                    ..
                } => {
                    dirty.extend(children.iter().copied());
                    if *direction == StructuralGroupChange::Grouped {
                        dirty.insert(*group);
                    } else {
                        removed.insert(*group);
                    }
                }
                DocumentChange::LocalTransformChanged { node }
                | DocumentChange::VisibilityChanged { node } => {
                    collect_scene_subtree(scene, *node, &mut dirty)?;
                }
                DocumentChange::GeometryChanged { node }
                | DocumentChange::AppearanceChanged { node, .. } => {
                    dirty.insert(*node);
                }
                DocumentChange::PersistentPropertyChanged { .. }
                | DocumentChange::FullDocumentReset => {}
            }
        }

        let mut stats = RenderUpdateStats {
            render_revision: change_set.revision().after,
            ..RenderUpdateStats::default()
        };
        let mut patches = Vec::with_capacity(dirty.len());
        for id in dirty {
            if document.node(id).is_none() {
                continue;
            }
            let prepared = prepare_item(document, scene, id, &mut self.path_cache, &mut stats)?;
            stats.document_nodes_scanned += 1;
            stats.scene_nodes_visited += 1;
            stats.render_items_planned += 1;
            patches.push((id, prepared));
        }

        // Every fallible source lookup has completed. The commit below cannot partially fail.
        let mut dirty_slots = Vec::new();
        let mut removed_slots = Vec::new();
        for id in removed {
            if self.by_id.contains_key(&id) {
                stats.render_items_read += 1;
            }
            if let Some(slot) = self.remove(id) {
                removed_slots.push(slot);
                stats.removed_items += 1;
                stats.render_items_written += 1;
            }
        }
        for (id, prepared) in patches {
            self.commit_prepared(id, prepared, &mut stats, &mut dirty_slots);
        }

        dirty_slots.sort_unstable();
        dirty_slots.dedup();
        removed_slots.sort_unstable();
        let dirty_ranges = coalesce_ranges(&dirty_slots);
        stats.dirty_items = dirty_slots.len() as u64;
        stats.dirty_slot_count = dirty_slots.len() as u64;
        stats.dirty_range_count = dirty_ranges.len() as u64;
        self.refresh_path_layout();
        self.render_revision = change_set.revision().after;
        self.finish_stats(stats.clone());
        Ok(RenderDelta {
            dirty_slots,
            removed_slots,
            dirty_ranges,
            stats,
        })
    }

    pub fn cull(
        &self,
        scene: &mut ComputedScene,
        world_viewport: Rect,
    ) -> Result<CullingResult, RenderModelError> {
        let query = scene.query_rect_candidates(world_viewport)?;
        let mut render_items_read = 0_usize;
        let mut gpu_encode_attempted = 0_usize;
        let mut gpu_encode_omitted = 0_usize;
        let ids_top_to_bottom = query
            .ids()
            .iter()
            .copied()
            .filter(|id| {
                render_items_read += 1;
                let Some(item) = self.item(*id) else {
                    return false;
                };
                if !item.renderable {
                    return false;
                }
                gpu_encode_attempted += 1;
                if !item.gpu_encodable {
                    gpu_encode_omitted += 1;
                    return false;
                }
                true
            })
            .collect::<Vec<_>>();
        let mut slots_bottom_to_top = Vec::with_capacity(ids_top_to_bottom.len());
        let mut visible_path_indices = Vec::new();
        let mut draw_batches: Vec<DrawBatch> = Vec::new();
        for id in ids_top_to_bottom.iter().rev() {
            let Some(item) = self.item(*id) else {
                continue;
            };
            match item.path_index {
                Some(path_index) => {
                    let Some((first_vertex, vertex_count)) = self.path_vertex_range(path_index)
                    else {
                        continue;
                    };
                    visible_path_indices.push(path_index);
                    if vertex_count == 0 {
                        continue;
                    }
                    match draw_batches.last_mut() {
                        // Paths that are adjacent in the buffer and adjacent in paint order
                        // collapse into a single draw call.
                        Some(DrawBatch::Paths {
                            first_vertex: batch_first,
                            vertex_count: batch_count,
                        }) if *batch_first + *batch_count == first_vertex => {
                            *batch_count += vertex_count;
                        }
                        _ => draw_batches.push(DrawBatch::Paths {
                            first_vertex,
                            vertex_count,
                        }),
                    }
                }
                None => {
                    let first_visible = slots_bottom_to_top.len() as u32;
                    slots_bottom_to_top.push(item.slot);
                    match draw_batches.last_mut() {
                        Some(DrawBatch::Primitives { count, .. }) => *count += 1,
                        _ => draw_batches.push(DrawBatch::Primitives {
                            first_visible,
                            count: 1,
                        }),
                    }
                }
            }
        }
        let exact_visible = ids_top_to_bottom.len();
        Ok(CullingResult {
            ids_top_to_bottom,
            slots_bottom_to_top,
            visible_path_indices,
            draw_batches,
            spatial_candidates: query.candidate_count(),
            exact_visible,
            culled: self.renderable_count().saturating_sub(exact_visible),
            submitted_instances: exact_visible,
            render_items_read,
            gpu_encode_attempted,
            gpu_encode_omitted,
            order_nodes_visited: query.order_nodes_visited(),
            sibling_search_steps: query.sibling_search_steps(),
            full_render_model_scans: 0,
        })
    }

    /// Compact, stable index of a path item inside the path instance array.
    ///
    /// Primitives never enter this list, so a document of rectangles pays nothing for path
    /// support.
    fn assign_path_index(&mut self, id: NodeId, is_path: bool) -> Option<u32> {
        let existing = self.path_order.iter().position(|entry| *entry == id);
        match (is_path, existing) {
            (true, Some(index)) => Some(index as u32),
            (true, None) => {
                self.path_order.push(id);
                self.path_layout_dirty = true;
                Some((self.path_order.len() - 1) as u32)
            }
            (false, Some(index)) => {
                self.forget_path_at(index);
                None
            }
            (false, None) => None,
        }
    }

    /// Removes a path from the compact order and renumbers the items after it.
    fn forget_path_at(&mut self, index: usize) {
        let id = self.path_order.remove(index);
        self.path_cache.forget(id);
        self.path_layout_dirty = true;
        for (position, node) in self.path_order.iter().enumerate().skip(index) {
            if let Some(slot) = self.by_id.get(node) {
                if let Some(Some(item)) = self.slots.get_mut(*slot as usize) {
                    item.path_index = Some(position as u32);
                }
            }
        }
    }

    /// Path items in their compact instance order.
    pub fn path_items(&self) -> impl Iterator<Item = &RenderItem> {
        self.path_order.iter().filter_map(|id| self.item(*id))
    }

    #[must_use]
    pub fn path_item_count(&self) -> usize {
        self.path_order.len()
    }

    /// Total tessellated vertices currently held by path items.
    #[must_use]
    pub fn path_vertex_count(&self) -> usize {
        self.path_vertex_total as usize
    }

    /// Monotonic revision of the path vertex buffer layout.
    ///
    /// The number changes only when a path's triangles change, so a consumer that uploads path
    /// vertices can skip the upload entirely while the camera moves or a colour changes.
    #[must_use]
    pub const fn path_vertex_revision(&self) -> u64 {
        self.path_vertex_revision
    }

    /// Half-open vertex range of one path inside the path vertex buffer.
    #[must_use]
    pub fn path_vertex_range(&self, path_index: u32) -> Option<(u32, u32)> {
        let first = *self.path_vertex_offsets.get(path_index as usize)?;
        let count = self
            .path_order
            .get(path_index as usize)
            .and_then(|id| self.item(*id))
            .and_then(|item| item.path.as_ref())
            .map_or(0, |tessellation| tessellation.vertex_count() as u32);
        Some((first, count))
    }

    /// Recomputes path vertex offsets after the compact path order or any tessellation changed.
    fn refresh_path_layout(&mut self) {
        if !self.path_layout_dirty {
            return;
        }
        self.path_layout_dirty = false;
        self.path_vertex_offsets.clear();
        self.path_vertex_offsets.reserve(self.path_order.len());
        let mut offset = 0_u32;
        for id in &self.path_order {
            self.path_vertex_offsets.push(offset);
            let vertices = self
                .by_id
                .get(id)
                .and_then(|slot| self.slots.get(*slot as usize))
                .and_then(|slot| slot.as_ref())
                .and_then(|item| item.path.as_ref())
                .map_or(0, |tessellation| tessellation.vertex_count() as u32);
            offset = offset.saturating_add(vertices);
        }
        self.path_vertex_total = offset;
        self.path_vertex_revision = self.path_vertex_revision.saturating_add(1);
    }

    fn commit_prepared(
        &mut self,
        id: NodeId,
        prepared: Option<PreparedItem>,
        stats: &mut RenderUpdateStats,
        dirty_slots: &mut Vec<u32>,
    ) {
        let Some(prepared) = prepared else {
            if self.by_id.contains_key(&id) {
                stats.render_items_read += 1;
            }
            if let Some(slot) = self.remove(id) {
                dirty_slots.push(slot);
                stats.removed_items += 1;
                stats.render_items_written += 1;
            }
            return;
        };
        let (slot, reused) = match self.by_id.get(&id).copied() {
            Some(slot) => {
                stats.render_items_read += 1;
                (slot, false)
            }
            None => {
                let reused = !self.free_slots.is_empty();
                let slot = if let Some(slot) = self.free_slots.pop() {
                    slot
                } else {
                    let previous_capacity = self.slots.capacity();
                    self.slots.push(None);
                    stats.allocation_growth_count +=
                        u64::from(self.slots.capacity() != previous_capacity);
                    (self.slots.len() - 1) as u32
                };
                self.by_id.insert(id, slot);
                stats.inserted_items += 1;
                (slot, reused)
            }
        };
        if reused {
            stats.reused_slots += 1;
        }
        let next = RenderItem {
            node_id: id,
            slot,
            primitive: prepared.primitive,
            path: prepared.path.clone(),
            // Assigned below, once the item is known to belong to the compact path order.
            path_index: None,
            size: prepared.size,
            world_transform: prepared.world_transform,
            world_bounds: prepared.world_bounds,
            fill_linear: prepared.fill_linear,
            opacity: prepared.opacity,
            corner_radii: prepared.corner_radii,
            stroke_linear: prepared.stroke_linear,
            stroke_width: prepared.stroke_width,
            renderable: prepared.renderable,
            gpu_encodable: prepared.gpu_encodable,
            encoding_diagnostic: prepared.encoding_diagnostic,
        };
        let mut next = next;
        next.path_index = self.assign_path_index(id, next.path.is_some());
        let changed = self.slots[slot as usize].as_ref() != Some(&next);
        if changed {
            let previous_path = self.slots[slot as usize]
                .as_ref()
                .and_then(|item| item.path.as_ref());
            self.path_layout_dirty |= match (previous_path, next.path.as_ref()) {
                (Some(before), Some(after)) => !Arc::ptr_eq(before, after),
                (None, None) => false,
                _ => true,
            };
            if let Some(old) = self.slots[slot as usize].take() {
                self.account_remove(&old);
            }
            self.account_insert(&next);
            self.slots[slot as usize] = Some(next);
            stats.render_items_written += 1;
            dirty_slots.push(slot);
        }
    }

    fn remove(&mut self, id: NodeId) -> Option<u32> {
        let slot = self.by_id.remove(&id)?;
        if let Some(index) = self.path_order.iter().position(|entry| *entry == id) {
            self.forget_path_at(index);
        }
        if let Some(old) = self.slots[slot as usize].take() {
            self.account_remove(&old);
        }
        self.free_slots.push(slot);
        Some(slot)
    }

    fn account_insert(&mut self, item: &RenderItem) {
        self.renderable_items += usize::from(item.renderable);
        self.gpu_encodable_items += usize::from(item.gpu_encodable);
        if let Some(kind) = item.encoding_diagnostic {
            self.diagnostics.insert(item.node_id, kind);
        }
    }

    fn account_remove(&mut self, item: &RenderItem) {
        self.renderable_items -= usize::from(item.renderable);
        self.gpu_encodable_items -= usize::from(item.gpu_encodable);
        self.diagnostics.remove(&item.node_id);
    }

    fn finish_stats(&mut self, stats: RenderUpdateStats) {
        self.last_update = stats.clone();
        self.accumulate(stats);
    }

    fn accumulate(&mut self, update: RenderUpdateStats) {
        self.cumulative.dirty_items += update.dirty_items;
        self.cumulative.inserted_items += update.inserted_items;
        self.cumulative.removed_items += update.removed_items;
        self.cumulative.reused_slots += update.reused_slots;
        self.cumulative.dirty_slot_count += update.dirty_slot_count;
        self.cumulative.dirty_range_count += update.dirty_range_count;
        self.cumulative.order_keys_updated += update.order_keys_updated;
        self.cumulative.full_render_rebuild_count += update.full_render_rebuild_count;
        self.cumulative.document_nodes_scanned += update.document_nodes_scanned;
        self.cumulative.scene_nodes_visited += update.scene_nodes_visited;
        self.cumulative.render_items_planned += update.render_items_planned;
        self.cumulative.render_items_read += update.render_items_read;
        self.cumulative.render_items_written += update.render_items_written;
        self.cumulative.render_items_cloned += update.render_items_cloned;
        self.cumulative.order_nodes_visited += update.order_nodes_visited;
        self.cumulative.sibling_search_steps += update.sibling_search_steps;
        self.cumulative.full_render_model_scans += update.full_render_model_scans;
        self.cumulative.allocation_growth_count += update.allocation_growth_count;
        self.cumulative.path_tessellations += update.path_tessellations;
        self.cumulative.path_vertices_tessellated += update.path_vertices_tessellated;
        self.cumulative.render_revision = update.render_revision;
    }
}

#[derive(Clone, Debug)]
struct PreparedItem {
    primitive: PrimitiveKind,
    path: Option<Arc<PathTessellation>>,
    size: Vec2,
    world_transform: Option<Affine2>,
    world_bounds: Option<Rect>,
    fill_linear: [f64; 4],
    opacity: f64,
    corner_radii: [f64; 4],
    stroke_linear: [f64; 4],
    stroke_width: f64,
    renderable: bool,
    gpu_encodable: bool,
    encoding_diagnostic: Option<RenderEncodingDiagnosticKind>,
}

fn prepare_item(
    document: &Document,
    scene: &ComputedScene,
    id: NodeId,
    cache: &mut PathCache,
    stats: &mut RenderUpdateStats,
) -> Result<Option<PreparedItem>, RenderModelError> {
    let node = document
        .node(id)
        .ok_or(RenderModelError::MissingDocumentNode(id))?;
    let Some((primitive, size)) = primitive(node.geometry()) else {
        return Ok(None);
    };
    let scene_node = scene
        .node(id)
        .ok_or(RenderModelError::MissingSceneNode(id))?;
    let world_transform = scene_node.world_transform();
    let appearance = node.appearance();
    // The triangles are needed before renderability is known, so they are produced once here and
    // handed to the item; the cache still counts one tessellation per changed path.
    let path = match node.geometry() {
        Some(Geometry::Path(path)) => {
            Some(cache.tessellation(id, PathTessellationKey::new(path, appearance), stats))
        }
        _ => None,
    };
    let fill_linear = linear_color(appearance.fill);
    let stroke_linear = linear_color(appearance.stroke.color);
    let corner_radii = [
        appearance.corner_radii.top_left,
        appearance.corner_radii.top_right,
        appearance.corner_radii.bottom_right,
        appearance.corner_radii.bottom_left,
    ];
    // A primitive with a degenerate box draws nothing, but a path is a stroked outline: a
    // straight horizontal line has zero height and is still drawn, so paths are judged by their
    // triangles instead of by their bounding box.
    let has_area = match primitive {
        PrimitiveKind::Path => path.as_ref().is_some_and(|path| !path.is_empty()),
        PrimitiveKind::Rectangle | PrimitiveKind::Ellipse => size.x > 0.0 && size.y > 0.0,
    };
    let renderable = scene_node.attached()
        && scene_node.effective_visible()
        && scene_node.invalid().is_none()
        && world_transform.is_some()
        && scene_node.own_world_bounds().is_some()
        && has_area;
    let encoding_diagnostic = if renderable {
        encoding_diagnostic(
            world_transform.expect("renderable item has a world transform"),
            size,
            fill_linear,
            appearance.opacity,
            corner_radii,
            stroke_linear,
            appearance.stroke.width,
        )
    } else {
        None
    };
    Ok(Some(PreparedItem {
        primitive,
        path,
        size,
        world_transform,
        world_bounds: scene_node.own_world_bounds(),
        fill_linear,
        opacity: appearance.opacity,
        corner_radii,
        stroke_linear,
        stroke_width: appearance.stroke.width,
        renderable,
        gpu_encodable: renderable && encoding_diagnostic.is_none(),
        encoding_diagnostic,
    }))
}

fn encoding_diagnostic(
    world: Affine2,
    size: Vec2,
    fill_linear: [f64; 4],
    opacity: f64,
    corner_radii: [f64; 4],
    stroke_linear: [f64; 4],
    stroke_width: f64,
) -> Option<RenderEncodingDiagnosticKind> {
    let linear_and_geometry = [
        world.m11,
        world.m12,
        world.m21,
        world.m22,
        size.x,
        size.y,
        fill_linear[0],
        fill_linear[1],
        fill_linear[2],
        fill_linear[3],
        opacity,
        corner_radii[0],
        corner_radii[1],
        corner_radii[2],
        corner_radii[3],
        stroke_linear[0],
        stroke_linear[1],
        stroke_linear[2],
        stroke_linear[3],
        stroke_width,
    ];
    if linear_and_geometry.iter().any(|value| !finite_f32(*value)) {
        return Some(RenderEncodingDiagnosticKind::LinearOrGeometryOutsideF32);
    }
    if split_f64(world.tx).is_none() || split_f64(world.ty).is_none() {
        return Some(RenderEncodingDiagnosticKind::TranslationOutsideF32);
    }
    None
}

fn linear_color(color: visual_authoring_document::ColorRgba) -> [f64; 4] {
    let channel = |value: f64| {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [
        channel(color.r),
        channel(color.g),
        channel(color.b),
        color.a,
    ]
}

fn finite_f32(value: f64) -> bool {
    value.is_finite() && (value as f32).is_finite()
}

fn split_f64(value: f64) -> Option<(f32, f32)> {
    if !value.is_finite() {
        return None;
    }
    let high = value as f32;
    if !high.is_finite() {
        return None;
    }
    let low = (value - f64::from(high)) as f32;
    low.is_finite().then_some((high, low))
}

fn primitive(geometry: Option<&Geometry>) -> Option<(PrimitiveKind, Vec2)> {
    match geometry? {
        Geometry::Frame { size } | Geometry::Rectangle { size } => {
            Some((PrimitiveKind::Rectangle, *size))
        }
        Geometry::Ellipse { size } => Some((PrimitiveKind::Ellipse, *size)),
        // A path reports the size of its exact bounds so culling, diagnostics, and renderability
        // read it the same way as a primitive; its silhouette comes from the tessellation.
        Geometry::Path(path) => {
            let bounds = path.bounds()?;
            Some((
                PrimitiveKind::Path,
                Vec2::new(bounds.width(), bounds.height()),
            ))
        }
    }
}

/// Derived tessellations, keyed by node and rebuilt only when a path's silhouette inputs change.
#[derive(Clone, Debug, Default)]
struct PathCache {
    entries: BTreeMap<NodeId, (PathTessellationKey, Arc<PathTessellation>)>,
}

impl PathCache {
    fn tessellation(
        &mut self,
        id: NodeId,
        key: PathTessellationKey,
        stats: &mut RenderUpdateStats,
    ) -> Arc<PathTessellation> {
        if let Some((cached_key, cached)) = self.entries.get(&id) {
            if *cached_key == key {
                return Arc::clone(cached);
            }
        }
        let tessellation = tessellate(&key);
        stats.path_tessellations += 1;
        stats.path_vertices_tessellated += tessellation.vertex_count() as u64;
        self.entries.insert(id, (key, Arc::clone(&tessellation)));
        tessellation
    }

    fn forget(&mut self, id: NodeId) {
        self.entries.remove(&id);
    }
}

fn collect_scene_subtree(
    scene: &ComputedScene,
    root: NodeId,
    output: &mut BTreeSet<NodeId>,
) -> Result<(), RenderModelError> {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = scene
            .node(id)
            .ok_or(RenderModelError::MissingSceneNode(id))?;
        output.insert(id);
        stack.extend(node.children().iter().rev().copied());
    }
    Ok(())
}

/// Coalesces an ascending, de-duplicated slot list into contiguous upload ranges.
///
/// Callers that merge several deltas into one batched frame reuse this so batched and
/// single-command uploads describe ranges identically.
#[must_use]
pub fn coalesce_ranges(slots: &[u32]) -> Vec<DirtySlotRange> {
    let mut ranges = Vec::new();
    let Some(&first) = slots.first() else {
        return ranges;
    };
    let mut start = first;
    let mut previous = first;
    for &slot in &slots[1..] {
        if slot == previous.saturating_add(1) {
            previous = slot;
        } else {
            ranges.push(DirtySlotRange {
                first: start,
                count: previous - start + 1,
            });
            start = slot;
            previous = slot;
        }
    }
    ranges.push(DirtySlotRange {
        first: start,
        count: previous - start + 1,
    });
    ranges
}
#[cfg(test)]
mod tests {
    use visual_authoring_document::{
        Appearance, ColorRgba, Command, CornerRadii, HeadlessEditorCore, NodeSpec, PathAnchor,
        PathAnchorId, PathGeometry, Stroke,
    };

    use super::*;

    fn setup() -> (
        HeadlessEditorCore,
        ComputedScene,
        RenderModel,
        NodeId,
        NodeId,
    ) {
        let mut editor = HeadlessEditorCore::blank("render");
        let root = editor.document().root_id();
        let rectangle = NodeId::new();
        let ellipse = NodeId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(rectangle, "rectangle", Vec2::new(20.0, 10.0)),
                parent: root,
                index: 0,
            })
            .unwrap();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::ellipse(ellipse, "ellipse", Vec2::new(12.0, 12.0)),
                parent: root,
                index: 1,
            })
            .unwrap();
        let scene = ComputedScene::build(editor.document(), editor.revision()).unwrap();
        let model = RenderModel::build(editor.document(), &scene, editor.revision()).unwrap();
        (editor, scene, model, rectangle, ellipse)
    }

    #[test]
    fn build_tracks_primitives_by_node_id_with_stable_slots() {
        let (_, _, model, rectangle, ellipse) = setup();
        assert_eq!(model.item_count(), 2);
        assert_eq!(
            model.item(rectangle).unwrap().primitive,
            PrimitiveKind::Rectangle
        );
        assert_eq!(
            model.item(ellipse).unwrap().primitive,
            PrimitiveKind::Ellipse
        );
        assert_ne!(
            model.item(rectangle).unwrap().slot,
            model.item(ellipse).unwrap().slot
        );
        assert_eq!(model.last_update().full_render_rebuild_count, 1);
    }

    #[test]
    fn a_path_becomes_a_tessellated_render_item() {
        let mut editor = HeadlessEditorCore::blank("render");
        let root = editor.document().root_id();
        let path_id = NodeId::new();
        let mut spec = NodeSpec::path(
            path_id,
            "path",
            PathGeometry {
                closed: false,
                anchors: vec![
                    PathAnchor::new(PathAnchorId::new(), Vec2::ZERO),
                    PathAnchor::new(PathAnchorId::new(), Vec2::new(20.0, 10.0)),
                ],
            },
        );
        // A path with no stroke and no enclosed area has nothing to draw, so give it a stroke.
        spec.appearance.stroke = Stroke {
            color: ColorRgba::new(0.0, 0.0, 0.0, 1.0),
            width: 2.0,
        };
        editor
            .dispatch(Command::CreateNode {
                spec,
                parent: root,
                index: 0,
            })
            .unwrap();
        let scene = ComputedScene::build(editor.document(), editor.revision()).unwrap();
        let model = RenderModel::build(editor.document(), &scene, editor.revision()).unwrap();

        let item = model.item(path_id).expect("the path is a render item");
        assert_eq!(item.primitive, PrimitiveKind::Path);
        assert_eq!(item.path_index, Some(0));
        assert!(item.renderable);
        assert!(item.gpu_encodable);
        let tessellation = item.path.as_ref().expect("a path item carries triangles");
        assert!(tessellation.vertex_count() > 0);
        assert_eq!(
            tessellation.fill_vertex_count, 0,
            "an open path is not filled"
        );
        assert_eq!(model.item_count(), 1);
        assert_eq!(model.path_item_count(), 1);
        assert_eq!(model.gpu_omitted_count(), 0);
        assert_eq!(
            model.last_update().path_tessellations,
            1,
            "building the model tessellates the path exactly once"
        );
    }

    #[test]
    fn one_transform_change_updates_one_slot_without_full_rebuild() {
        let (mut editor, mut scene, mut model, rectangle, _) = setup();
        let outcome = editor
            .dispatch(Command::SetLocalTransform {
                target: rectangle,
                transform: Affine2::translation(Vec2::new(50.0, 25.0)),
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        let delta = model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();
        assert_eq!(delta.dirty_slots.len(), 1);
        assert_eq!(delta.stats.full_render_rebuild_count, 0);
        assert_eq!(delta.stats.dirty_slot_count, 1);
    }

    #[test]
    fn deleted_slot_is_reused_without_duplicates() {
        let (mut editor, mut scene, mut model, rectangle, ellipse) = setup();
        let removed_slot = model.item(rectangle).unwrap().slot;
        let outcome = editor
            .dispatch(Command::DeleteSubtree { target: rectangle })
            .unwrap();
        scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();
        assert_eq!(model.free_slot_count(), 1);

        let replacement = NodeId::new();
        let outcome = editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(replacement, "replacement", Vec2::new(4.0, 4.0)),
                parent: editor.document().root_id(),
                index: 1,
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        let delta = model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();
        assert_eq!(model.item(replacement).unwrap().slot, removed_slot);
        assert_eq!(delta.stats.reused_slots, 1);
        assert_ne!(
            model.item(replacement).unwrap().slot,
            model.item(ellipse).unwrap().slot
        );
    }

    #[test]
    fn culling_excludes_offscreen_items_and_wide_view_returns_all() {
        let (mut editor, mut scene, mut model, rectangle, ellipse) = setup();
        let outcome = editor
            .dispatch(Command::SetLocalTransform {
                target: ellipse,
                transform: Affine2::translation(Vec2::new(10_000.0, 10_000.0)),
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();

        let narrow = model
            .cull(
                &mut scene,
                Rect::from_min_max(Vec2::new(-10.0, -10.0), Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        assert_eq!(narrow.ids_top_to_bottom, vec![rectangle]);
        assert_eq!(narrow.culled, 1);

        let wide = model
            .cull(
                &mut scene,
                Rect::from_min_max(Vec2::new(-1.0e12, -1.0e12), Vec2::new(1.0e12, 1.0e12)),
            )
            .unwrap();
        assert_eq!(wide.submitted_instances, 2);
    }

    #[test]
    fn unencodable_translation_is_omitted_and_recovers_same_slot_incrementally() {
        let (mut editor, mut scene, mut model, rectangle, _) = setup();
        let slot = model.item(rectangle).unwrap().slot;
        let outcome = editor
            .dispatch(Command::SetLocalTransform {
                target: rectangle,
                transform: Affine2::translation(Vec2::new(1.0e100, 0.0)),
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        let delta = model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();
        let item = model.item(rectangle).unwrap();
        assert!(item.renderable);
        assert!(!item.gpu_encodable);
        assert_eq!(item.slot, slot);
        assert_eq!(
            item.encoding_diagnostic,
            Some(RenderEncodingDiagnosticKind::TranslationOutsideF32)
        );
        assert_eq!(model.gpu_omitted_count(), 1);
        assert_eq!(delta.dirty_slots, vec![slot]);
        assert_eq!(delta.stats.render_items_cloned, 0);
        assert_eq!(delta.stats.full_render_model_scans, 0);
        assert_eq!(delta.stats.order_nodes_visited, 0);
        assert_eq!(delta.stats.document_nodes_scanned, 1);
        assert_eq!(delta.stats.render_items_planned, 1);
        assert_eq!(delta.stats.render_items_written, 1);

        let recovered = editor
            .dispatch(Command::SetLocalTransform {
                target: rectangle,
                transform: Affine2::translation(Vec2::new(25.0, 30.0)),
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), recovered.change_set())
            .unwrap();
        let recovered_delta = model
            .apply_changes(editor.document(), &scene, recovered.change_set())
            .unwrap();
        let item = model.item(rectangle).unwrap();
        assert!(item.gpu_encodable);
        assert_eq!(item.slot, slot);
        assert_eq!(item.encoding_diagnostic, None);
        assert_eq!(model.gpu_omitted_count(), 0);
        assert_eq!(recovered_delta.dirty_slots, vec![slot]);
    }

    #[test]
    fn f32_range_geometry_and_linear_transform_are_typed_omissions() {
        let (mut editor, mut scene, mut model, rectangle, ellipse) = setup();
        let geometry = editor
            .dispatch(Command::SetGeometry {
                target: rectangle,
                geometry: Geometry::Rectangle {
                    size: Vec2::new(1.0e100, 10.0),
                },
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), geometry.change_set())
            .unwrap();
        model
            .apply_changes(editor.document(), &scene, geometry.change_set())
            .unwrap();
        assert_eq!(
            model.item(rectangle).unwrap().encoding_diagnostic,
            Some(RenderEncodingDiagnosticKind::LinearOrGeometryOutsideF32)
        );

        let linear = editor
            .dispatch(Command::SetLocalTransform {
                target: ellipse,
                transform: Affine2 {
                    m11: 1.0e100,
                    m12: 0.0,
                    m21: 0.0,
                    m22: 1.0,
                    tx: 0.0,
                    ty: 0.0,
                },
            })
            .unwrap();
        scene
            .apply_changes(editor.document(), linear.change_set())
            .unwrap();
        model
            .apply_changes(editor.document(), &scene, linear.change_set())
            .unwrap();
        assert_eq!(
            model.item(ellipse).unwrap().encoding_diagnostic,
            Some(RenderEncodingDiagnosticKind::LinearOrGeometryOutsideF32)
        );
        assert_eq!(model.gpu_omitted_count(), 2);
        assert_eq!(model.gpu_encodable_count(), 0);
    }

    #[test]
    fn incremental_counters_match_a_scanning_oracle() {
        let (mut editor, mut scene, mut model, rectangle, ellipse) = setup();
        for step in 0..128 {
            let target = if step % 2 == 0 { rectangle } else { ellipse };
            let transform = if step % 11 == 0 {
                Affine2::translation(Vec2::new(1.0e100, step as f64))
            } else {
                Affine2::translation(Vec2::new(step as f64, -(step as f64)))
            };
            let outcome = editor
                .dispatch(Command::SetLocalTransform { target, transform })
                .unwrap();
            scene
                .apply_changes(editor.document(), outcome.change_set())
                .unwrap();
            model
                .apply_changes(editor.document(), &scene, outcome.change_set())
                .unwrap();

            let items = model.items().collect::<Vec<_>>();
            let oracle_renderable = items.iter().filter(|item| item.renderable).count();
            let oracle_encodable = items.iter().filter(|item| item.gpu_encodable).count();
            let oracle_omitted = items
                .iter()
                .filter(|item| item.encoding_diagnostic.is_some())
                .count();
            let counters = model.counters();
            assert_eq!(counters.total_render_items, items.len());
            assert_eq!(counters.renderable_items, oracle_renderable);
            assert_eq!(counters.gpu_encodable_items, oracle_encodable);
            assert_eq!(counters.gpu_omitted_items, oracle_omitted);
            assert_eq!(
                counters.allocated_slots,
                counters.total_render_items + counters.free_slots
            );
        }
    }

    #[test]
    fn appearance_change_updates_one_stable_slot_with_linear_color_contract() {
        let (mut editor, mut scene, mut model, rectangle, _) = setup();
        let slot = model.item(rectangle).unwrap().slot;
        let outcome = editor
            .dispatch(Command::SetAppearance {
                target: rectangle,
                appearance: Appearance {
                    fill: ColorRgba::new(1.0, 0.040_45, 0.0, 0.75),
                    opacity: 0.5,
                    corner_radii: CornerRadii {
                        top_left: 2.0,
                        top_right: 4.0,
                        bottom_right: 6.0,
                        bottom_left: 8.0,
                    },
                    stroke: Stroke {
                        color: ColorRgba::new(0.0, 1.0, 0.0, 0.5),
                        width: 3.0,
                    },
                },
            })
            .unwrap();
        let scene_stats = scene
            .apply_changes(editor.document(), outcome.change_set())
            .unwrap();
        let delta = model
            .apply_changes(editor.document(), &scene, outcome.change_set())
            .unwrap();
        let item = model.item(rectangle).unwrap();

        assert_eq!(item.slot, slot);
        assert_eq!(item.fill_linear, [1.0, 0.040_45 / 12.92, 0.0, 0.75]);
        assert_eq!(item.opacity, 0.5);
        assert_eq!(item.corner_radii, [2.0, 4.0, 6.0, 8.0]);
        assert_eq!(item.stroke_linear, [0.0, 1.0, 0.0, 0.5]);
        assert_eq!(item.stroke_width, 3.0);
        assert_eq!(delta.dirty_slots, vec![slot]);
        assert_eq!(delta.stats.full_render_rebuild_count, 0);
        assert_eq!(delta.stats.full_render_model_scans, 0);
        assert_eq!(scene_stats.bounds_recomputed, 1);
    }
}
