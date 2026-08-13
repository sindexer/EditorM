//! Scene-aware application runtime. Normal persistent edits automatically synchronize the
//! computed scene and spatial index before success is returned.

mod camera;
pub mod fixtures;

#[cfg(test)]
mod tests;

pub use camera::{Camera, CameraError, DevicePoint, ViewportPoint, WorldPoint};

use thiserror::Error;
use visual_authoring_core_math::Rect;
use visual_authoring_document::{
    Command, CommandOutcome, Document, DocumentChangeSet, EditorError, HeadlessEditorCore,
    HistoryState, NodeId, Selection, SelectionError, SequenceWork,
};
use visual_authoring_render_model::{
    CullingResult, DirtySlotRange, RenderDelta, RenderModel, RenderModelError,
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
