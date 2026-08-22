//! Scene-aware application runtime. Normal persistent edits automatically synchronize the
//! computed scene and spatial index before success is returned.

pub mod arrange;
mod camera;
pub mod fixtures;
pub mod snap;

#[cfg(test)]
mod tests;

pub use arrange::{AlignMode, ArrangeError, ArrangePlan, DistributeAxis};
pub use camera::{Camera, CameraError, DevicePoint, ViewportPoint, WorldPoint};
pub use snap::{SnapAxis, SnapError, SnapGuide, SnapResolution};

use std::collections::BTreeSet;

use thiserror::Error;
use visual_authoring_core_math::Rect;
use visual_authoring_document::{
    ChangeMergeError, Command, CommandOutcome, Document, DocumentChangeSet, EditorError,
    HeadlessEditorCore, HistoryState, NodeId, Selection, SelectionError, SequenceWork,
};
use visual_authoring_render_model::{
    coalesce_ranges, CullingResult, DirtySlotRange, RenderDelta, RenderModel, RenderModelError,
    RenderUpdateStats,
};
use visual_authoring_scene::{
    ComputedScene, HitTestResult, SceneError, SceneQueryResult, SceneUpdateStats,
};

#[derive(Clone, Debug, PartialEq)]
pub enum SceneSyncStatus {
    Incremental,
    FullDocumentReset,
    FallbackRebuild { cause: SceneError },
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderSyncStatus {
    Incremental,
    FullDocumentReset,
    FallbackRebuild { cause: RenderModelError },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeCommandOutcome {
    pub command: CommandOutcome,
    pub scene: SceneUpdateStats,
    pub sync: SceneSyncStatus,
    pub render: RenderDelta,
    pub render_sync: RenderSyncStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeSceneOutcome {
    pub change_set: DocumentChangeSet,
    pub sequence_work: SequenceWork,
    pub scene: SceneUpdateStats,
    pub sync: SceneSyncStatus,
    pub render: RenderDelta,
    pub render_sync: RenderSyncStatus,
}

type SyncOutcome = (
    SceneUpdateStats,
    SceneSyncStatus,
    RenderDelta,
    RenderSyncStatus,
);

#[derive(Clone, Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error(transparent)]
    Editor(#[from] EditorError),
    #[error(transparent)]
    Arrange(#[from] ArrangeError),
    #[error(transparent)]
    Snap(#[from] SnapError),
    #[error(transparent)]
    ChangeMerge(#[from] ChangeMergeError),
    #[error("world bounds {0:?} are not finite")]
    NonFiniteBounds(Rect),
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error(transparent)]
    Render(#[from] RenderModelError),
    #[error(transparent)]
    Camera(#[from] CameraError),
    #[error(transparent)]
    Selection(#[from] SelectionError),
}

/// Owns the headless editor core, computed scene/spatial cache, camera, revisions, and metrics.
pub struct EngineRuntime {
    core: HeadlessEditorCore,
    scene: ComputedScene,
    render: RenderModel,
    camera: Camera,
}

impl EngineRuntime {
    pub fn new(document: Document, camera: Camera) -> Result<Self, RuntimeError> {
        let core = HeadlessEditorCore::new(document)?;
        let scene = ComputedScene::build(core.document(), core.revision())?;
        let render = RenderModel::build(core.document(), &scene, core.revision())?;
        Ok(Self {
            core,
            scene,
            render,
            camera,
        })
    }

    pub fn blank(name: impl Into<String>) -> Result<Self, RuntimeError> {
        Self::new(Document::new(name), Camera::default())
    }

    #[must_use]
    pub const fn document(&self) -> &Document {
        self.core.document()
    }

    #[must_use]
    pub const fn scene(&self) -> &ComputedScene {
        &self.scene
    }

    #[must_use]
    pub const fn render_model(&self) -> &RenderModel {
        &self.render
    }

    #[must_use]
    pub const fn camera(&self) -> &Camera {
        &self.camera
    }

    pub const fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    #[must_use]
    pub const fn selection(&self) -> &Selection {
        self.core.selection()
    }

    #[must_use]
    pub const fn history_state(&self) -> HistoryState {
        self.core.history_state()
    }

    #[must_use]
    pub const fn transaction_active(&self) -> bool {
        self.core.transaction_active()
    }

    #[must_use]
    pub const fn document_revision(&self) -> u64 {
        self.core.revision()
    }

    pub fn dispatch(&mut self, command: Command) -> Result<RuntimeCommandOutcome, RuntimeError> {
        let outcome = self.core.dispatch(command)?;
        let change_set = outcome.change_set().clone();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(RuntimeCommandOutcome {
            command: outcome,
            scene,
            sync,
            render,
            render_sync,
        })
    }

    pub fn begin_transaction(&mut self) -> Result<(), RuntimeError> {
        self.core.begin_transaction()?;
        Ok(())
    }

    pub fn update_transaction(
        &mut self,
        command: Command,
    ) -> Result<RuntimeCommandOutcome, RuntimeError> {
        let outcome = self.core.update_transaction(command)?;
        let change_set = outcome.change_set().clone();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(RuntimeCommandOutcome {
            command: outcome,
            scene,
            sync,
            render,
            render_sync,
        })
    }

    /// Preview updates already synchronized scene state, so commit performs no recomputation.
    pub fn commit_transaction(&mut self) -> Result<bool, RuntimeError> {
        Ok(self.core.commit_transaction()?)
    }

    pub fn rollback_transaction(&mut self) -> Result<RuntimeSceneOutcome, RuntimeError> {
        let change_set = self.core.rollback_transaction()?;
        let sequence_work = self.core.document().sequence_work();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(RuntimeSceneOutcome {
            sequence_work,
            change_set,
            scene,
            sync,
            render,
            render_sync,
        })
    }

    pub fn undo(&mut self) -> Result<Option<RuntimeSceneOutcome>, RuntimeError> {
        let Some(change_set) = self.core.undo()? else {
            return Ok(None);
        };
        let sequence_work = self.core.document().sequence_work();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(Some(RuntimeSceneOutcome {
            sequence_work,
            change_set,
            scene,
            sync,
            render,
            render_sync,
        }))
    }

    pub fn redo(&mut self) -> Result<Option<RuntimeSceneOutcome>, RuntimeError> {
        let Some(change_set) = self.core.redo()? else {
            return Ok(None);
        };
        let sequence_work = self.core.document().sequence_work();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(Some(RuntimeSceneOutcome {
            sequence_work,
            change_set,
            scene,
            sync,
            render,
            render_sync,
        }))
    }

    pub fn replace_document(
        &mut self,
        document: Document,
    ) -> Result<RuntimeSceneOutcome, RuntimeError> {
        let change_set = self.core.replace_document(document)?;
        let sequence_work = self.core.document().sequence_work();
        let (scene, sync, render, render_sync) = self.synchronize(&change_set)?;
        Ok(RuntimeSceneOutcome {
            sequence_work,
            change_set,
            scene,
            sync,
            render,
            render_sync,
        })
    }

    pub fn hit_test_viewport(
        &mut self,
        point: ViewportPoint,
    ) -> Result<HitTestResult, RuntimeError> {
        let world = self.camera.viewport_to_world(point)?;
        Ok(self
            .scene
            .hit_test_world_point(self.core.document(), world.0)?)
    }

    pub fn query_world_rect(&mut self, bounds: Rect) -> Result<SceneQueryResult, RuntimeError> {
        Ok(self.scene.query_rect_candidates(bounds)?)
    }

    pub fn cull_viewport(&mut self) -> Result<CullingResult, RuntimeError> {
        let world_viewport = self.camera.world_viewport_bounds()?;
        Ok(self.render.cull(&mut self.scene, world_viewport)?)
    }

    pub fn select_only(&mut self, id: NodeId) -> Result<(), RuntimeError> {
        self.core.select_only(id)?;
        Ok(())
    }

    pub fn add_to_selection(&mut self, id: NodeId) -> Result<bool, RuntimeError> {
        Ok(self.core.add_to_selection(id)?)
    }

    pub fn remove_from_selection(&mut self, id: NodeId) -> bool {
        self.core.remove_from_selection(id)
    }

    pub fn toggle_selection(&mut self, id: NodeId) -> Result<bool, RuntimeError> {
        Ok(self.core.toggle_selection(id)?)
    }

    pub fn clear_selection(&mut self) {
        self.core.clear_selection();
    }

    /// Replaces the selection with a validated set of IDs, keeping request order.
    pub fn select_many(&mut self, ids: &[NodeId]) -> Result<usize, RuntimeError> {
        Ok(self.core.select_many(ids)?)
    }

    /// Adds IDs to the current selection without dropping what is already selected.
    pub fn extend_selection(&mut self, ids: &[NodeId]) -> Result<usize, RuntimeError> {
        Ok(self.core.extend_selection(ids)?)
    }

    /// Resolves the top-level nodes a rubber-band rectangle covers inside `root`.
    ///
    /// Only direct children of `root` are returned: dragging a band across a group selects the
    /// group, exactly as clicking one of its members would.
    pub fn marquee_candidates(
        &mut self,
        world_bounds: Rect,
        root: NodeId,
    ) -> Result<MarqueeResult, RuntimeError> {
        if !world_bounds.is_finite() {
            return Err(RuntimeError::NonFiniteBounds(world_bounds));
        }
        let query = self.scene.query_rect_candidates(world_bounds)?;
        let examined = query.ids().len() as u64;
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for id in query.ids() {
            let Some(top_level) = self.top_level_selectable(*id, root) else {
                continue;
            };
            if seen.insert(top_level) {
                selected.push(top_level);
            }
        }
        Ok(MarqueeResult {
            selected,
            candidates_examined: examined,
            candidate_count: query.candidate_count(),
        })
    }

    /// Walks up from `id` to the child of `root` that a marquee should select, or `None` when
    /// the node is outside `root`, hidden, or inside a locked container.
    fn top_level_selectable(&self, id: NodeId, root: NodeId) -> Option<NodeId> {
        if id == root || id == self.core.document().root_id() {
            return None;
        }
        if !self.scene.node(id)?.effective_visible() {
            return None;
        }
        let mut current = id;
        let mut depth = 0_usize;
        loop {
            let node = self.core.document().node(current)?;
            if node.locked() {
                return None;
            }
            let parent = node.parent()?;
            if parent == root {
                return Some(current);
            }
            depth += 1;
            if depth > self.core.document().len() {
                return None;
            }
            current = parent;
        }
    }

    /// Resolves the snap correction for the world bounds a dragged selection would occupy.
    pub fn resolve_snap(
        &mut self,
        proposed: Rect,
        moving: &[NodeId],
        threshold: f64,
    ) -> Result<SnapResolution, RuntimeError> {
        let viewport = self.camera.world_viewport_bounds()?;
        Ok(snap::resolve(
            self.core.document(),
            &mut self.scene,
            proposed,
            moving,
            threshold,
            viewport,
        )?)
    }

    /// Plans an alignment without touching document, scene, render, or history state.
    pub fn plan_align(
        &self,
        targets: &[NodeId],
        mode: AlignMode,
    ) -> Result<ArrangePlan, RuntimeError> {
        Ok(arrange::plan_align(
            self.core.document(),
            &self.scene,
            targets,
            mode,
        )?)
    }

    /// Plans equal spacing without touching document, scene, render, or history state.
    pub fn plan_distribute(
        &self,
        targets: &[NodeId],
        axis: DistributeAxis,
    ) -> Result<ArrangePlan, RuntimeError> {
        Ok(arrange::plan_distribute(
            self.core.document(),
            &self.scene,
            targets,
            axis,
        )?)
    }

    /// Applies a planned arrangement as one transaction, and therefore as one undo step.
    ///
    /// A failure inside the batch rolls the whole transaction back, so the document never keeps
    /// a partially aligned selection.
    pub fn apply_arrange(&mut self, plan: &ArrangePlan) -> Result<BatchOutcome, RuntimeError> {
        self.apply_transactional_batch(plan.commands())
    }

    /// Runs several commands as one transaction and reports one merged change set and delta.
    pub fn apply_transactional_batch(
        &mut self,
        commands: Vec<Command>,
    ) -> Result<BatchOutcome, RuntimeError> {
        if commands.is_empty() {
            return Ok(BatchOutcome::unchanged(self.core.revision()));
        }
        self.begin_transaction()?;
        match self.apply_batch_in_transaction(commands) {
            Ok(mut outcome) => {
                outcome.committed = self.commit_transaction()?;
                Ok(outcome)
            }
            Err(error) => {
                self.rollback_transaction()?;
                Err(error)
            }
        }
    }

    /// Runs several commands inside an already active transaction, merging their reports.
    pub fn apply_batch_in_transaction(
        &mut self,
        commands: Vec<Command>,
    ) -> Result<BatchOutcome, RuntimeError> {
        let base_revision = self.core.revision();
        let mut outcomes = Vec::with_capacity(commands.len());
        let mut change_sets = Vec::with_capacity(commands.len());
        for command in commands {
            let outcome = self.update_transaction(command)?;
            change_sets.push(outcome.command.change_set().clone());
            outcomes.push(outcome);
        }
        let change_set = DocumentChangeSet::merge_consecutive(&change_sets, base_revision)?;
        let render = merge_render_deltas(outcomes.iter().map(|outcome| &outcome.render));
        Ok(BatchOutcome {
            outcomes,
            change_set,
            render,
            committed: false,
        })
    }

    fn synchronize(&mut self, change_set: &DocumentChangeSet) -> Result<SyncOutcome, RuntimeError> {
        let full_reset = change_set.changes().iter().any(|change| {
            matches!(
                change,
                visual_authoring_document::DocumentChange::FullDocumentReset
            )
        });
        let (scene, sync) = match self.scene.apply_changes(self.core.document(), change_set) {
            Ok(stats) => {
                let status = if full_reset {
                    SceneSyncStatus::FullDocumentReset
                } else {
                    SceneSyncStatus::Incremental
                };
                (stats, status)
            }
            Err(cause) => {
                let stats = self
                    .scene
                    .rebuild_after_failure(self.core.document(), self.core.revision())?;
                (stats, SceneSyncStatus::FallbackRebuild { cause })
            }
        };
        let (render, render_sync) =
            match self
                .render
                .apply_changes(self.core.document(), &self.scene, change_set)
            {
                Ok(delta) => {
                    let status = if full_reset {
                        RenderSyncStatus::FullDocumentReset
                    } else {
                        RenderSyncStatus::Incremental
                    };
                    (delta, status)
                }
                Err(cause) => {
                    let rebuilt = RenderModel::build(
                        self.core.document(),
                        &self.scene,
                        self.core.revision(),
                    )?;
                    let mut dirty_slots = rebuilt.items().map(|item| item.slot).collect::<Vec<_>>();
                    dirty_slots.sort_unstable();
                    let dirty_ranges = if dirty_slots.is_empty() {
                        Vec::new()
                    } else {
                        vec![DirtySlotRange {
                            first: 0,
                            count: dirty_slots.len() as u32,
                        }]
                    };
                    let stats = rebuilt.last_update().clone();
                    self.render = rebuilt;
                    (
                        RenderDelta {
                            dirty_slots,
                            removed_slots: Vec::new(),
                            dirty_ranges,
                            stats,
                        },
                        RenderSyncStatus::FallbackRebuild { cause },
                    )
                }
            };
        Ok((scene, sync, render, render_sync))
    }

    #[cfg(test)]
    fn set_revision_for_test(&mut self, revision: u64) -> Result<(), RuntimeError> {
        self.core.set_revision_for_test(revision);
        self.scene = ComputedScene::build(self.core.document(), revision)?;
        self.render = RenderModel::build(self.core.document(), &self.scene, revision)?;
        Ok(())
    }
}

/// Top-level nodes one rubber-band rectangle resolves to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarqueeResult {
    selected: Vec<NodeId>,
    candidates_examined: u64,
    candidate_count: usize,
}

impl MarqueeResult {
    #[must_use]
    pub fn selected(&self) -> &[NodeId] {
        &self.selected
    }

    #[must_use]
    pub const fn candidates_examined(&self) -> u64 {
        self.candidates_examined
    }

    #[must_use]
    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }
}

/// Merged report for several commands applied as one atomic batch.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchOutcome {
    pub outcomes: Vec<RuntimeCommandOutcome>,
    pub change_set: DocumentChangeSet,
    pub render: RenderDelta,
    pub committed: bool,
}

impl BatchOutcome {
    fn unchanged(revision: u64) -> Self {
        Self {
            outcomes: Vec::new(),
            change_set: DocumentChangeSet::unchanged(revision),
            render: RenderDelta::default(),
            committed: false,
        }
    }

    #[must_use]
    pub fn applied(&self) -> usize {
        self.outcomes.len()
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.change_set.changed_document()
    }
}

/// Unions per-command render deltas into the single upload a batched request performs.
fn merge_render_deltas<'a>(deltas: impl Iterator<Item = &'a RenderDelta>) -> RenderDelta {
    let mut dirty = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let mut stats = RenderUpdateStats::default();
    for delta in deltas {
        dirty.extend(delta.dirty_slots.iter().copied());
        removed.extend(delta.removed_slots.iter().copied());
        stats = accumulate_render_stats(stats, &delta.stats);
    }
    for slot in &removed {
        dirty.remove(slot);
    }
    let dirty_slots = dirty.into_iter().collect::<Vec<_>>();
    let removed_slots = removed.into_iter().collect::<Vec<_>>();
    let dirty_ranges = coalesce_ranges(&dirty_slots);
    stats.dirty_slot_count = dirty_slots.len() as u64;
    stats.dirty_range_count = dirty_ranges.len() as u64;
    RenderDelta {
        dirty_slots,
        removed_slots,
        dirty_ranges,
        stats,
    }
}

fn accumulate_render_stats(
    mut accumulated: RenderUpdateStats,
    next: &RenderUpdateStats,
) -> RenderUpdateStats {
    accumulated.dirty_items += next.dirty_items;
    accumulated.inserted_items += next.inserted_items;
    accumulated.removed_items += next.removed_items;
    accumulated.reused_slots += next.reused_slots;
    accumulated.order_keys_updated += next.order_keys_updated;
    accumulated.full_render_rebuild_count += next.full_render_rebuild_count;
    accumulated.render_revision = accumulated.render_revision.max(next.render_revision);
    accumulated.document_nodes_scanned += next.document_nodes_scanned;
    accumulated.scene_nodes_visited += next.scene_nodes_visited;
    accumulated.render_items_planned += next.render_items_planned;
    accumulated.render_items_read += next.render_items_read;
    accumulated.render_items_written += next.render_items_written;
    accumulated.render_items_cloned += next.render_items_cloned;
    accumulated.order_nodes_visited += next.order_nodes_visited;
    accumulated.sibling_search_steps += next.sibling_search_steps;
    accumulated.full_render_model_scans += next.full_render_model_scans;
    accumulated.allocation_growth_count += next.allocation_growth_count;
    accumulated
}
