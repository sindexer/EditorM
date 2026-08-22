#![recursion_limit = "256"]

//! Versioned WASM EngineHost boundary. The browser Worker owns this type; JavaScript receives
//! typed responses plus compact transferable instance and visibility buffers.

use std::collections::BTreeSet;

use js_sys::{Uint32Array, Uint8Array};
use serde::Deserialize;
use serde_json::{json, Value};
use visual_authoring_core_math::{Affine2, Rect, Vec2};
use visual_authoring_document::{
    Appearance, ColorRgba, Command, Document, DocumentChange, DocumentChangeSet, Geometry, NodeId,
    NodeKind, NodeSpec, StructuralGroupChange,
};
use visual_authoring_render_model::{
    CullingResult, PrimitiveKind, RenderDelta, RenderEncodingDiagnosticKind, RenderItem,
};
use visual_authoring_runtime::arrange::{translated, world_delta_to_local};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{
    AlignMode, BatchOutcome, Camera, CameraError, DistributeAxis, EngineRuntime, RenderSyncStatus,
    RuntimeCommandOutcome, RuntimeError, RuntimeSceneOutcome, SceneSyncStatus, ViewportPoint,
    WorldPoint,
};
use wasm_bindgen::prelude::*;

pub const PROTOCOL_VERSION: u32 = 1;
pub const RENDER_BINARY_SCHEMA_VERSION: u32 = 2;
const INSTANCE_STRIDE_BYTES: usize = 112;
const DIRTY_RECORD_STRIDE_BYTES: usize = 4 + INSTANCE_STRIDE_BYTES;

#[derive(Debug, Deserialize)]
struct RequestEnvelope {
    protocol_version: u32,
    request_id: String,
    #[serde(flatten)]
    operation: HostRequest,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostRequest {
    Initialize,
    LoadFixture {
        fixture: String,
    },
    Command {
        command: CommandRequest,
    },
    CommandBatch {
        commands: Vec<CommandRequest>,
    },
    BeginTransaction,
    UpdateTransaction {
        command: CommandRequest,
    },
    CommitTransaction,
    RollbackTransaction,
    Undo,
    Redo,
    Camera {
        camera: CameraRequest,
    },
    HitTest {
        x: f64,
        y: f64,
        #[serde(default)]
        root_id: Option<String>,
        #[serde(default)]
        selectable_only: bool,
    },
    Selection {
        target: Option<String>,
        #[serde(default)]
        targets: Option<Vec<String>>,
        mode: String,
    },
    SetActiveRoot {
        node_id: String,
    },
    MarqueeSelect {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        #[serde(default)]
        additive: bool,
        #[serde(default)]
        root_id: Option<String>,
        #[serde(default)]
        exclude_ids: Option<Vec<String>>,
    },
    TranslateSelection {
        dx: f64,
        dy: f64,
        #[serde(default)]
        snap: bool,
        #[serde(default)]
        snap_threshold_px: Option<f64>,
    },
    TransformSelection {
        matrix: [f64; 6],
    },
    Arrange {
        operation: String,
    },
    GetUiSnapshot,
    SaveDocument,
    LoadDocument {
        document_json: String,
    },
    Heartbeat,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CommandRequest {
    MoveNode {
        node_index: u64,
        x: f64,
        y: f64,
    },
    ReorderNode {
        node_index: u64,
        index: usize,
    },
    SetVisibility {
        node_index: u64,
        visible: bool,
    },
    SetTransform {
        node_id: String,
        matrix: [f64; 6],
    },
    SetName {
        node_id: String,
        name: String,
    },
    SetVisible {
        node_id: String,
        visible: bool,
    },
    SetLocked {
        node_id: String,
        locked: bool,
    },
    SetGeometry {
        node_id: String,
        shape: String,
        width: f64,
        height: f64,
    },
    SetOpacity {
        node_id: String,
        opacity: f64,
    },
    SetFill {
        node_id: String,
        color: [f64; 4],
    },
    SetCornerRadii {
        node_id: String,
        radii: [f64; 4],
    },
    SetStroke {
        node_id: String,
        color: [f64; 4],
        width: f64,
    },
    CreateShape {
        node_id: String,
        parent_id: String,
        index: usize,
        shape: String,
        name: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Group {
        group_id: String,
        name: String,
        targets: Vec<String>,
    },
    Ungroup {
        node_id: String,
    },
    DeleteNode {
        node_id: String,
    },
    Reparent {
        node_id: String,
        parent_id: String,
        index: usize,
        preserve_world: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CameraRequest {
    Resize { width: f64, height: f64, dpr: f64 },
    Pan { dx: f64, dy: f64 },
    Zoom { x: f64, y: f64, zoom: f64 },
    Reset,
    Fit,
    FitSelection { node_id: String },
}

/// One rubber-band request in viewport coordinates.
#[derive(Clone, Copy, Debug)]
struct MarqueeRequest {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    additive: bool,
}

/// Selection state captured when a transaction opens, so every drag frame is computed from the
/// same starting geometry instead of from the previously previewed position.
#[derive(Clone, Debug, Default)]
struct DragBase {
    targets: Vec<DragTarget>,
    union_bounds: Option<Rect>,
    skipped_locked: usize,
}

#[derive(Clone, Copy, Debug)]
struct DragTarget {
    id: NodeId,
    local_transform: Affine2,
    parent_world: Affine2,
}

#[derive(Clone, Debug, Default)]
struct DeltaReport {
    full: bool,
    dirty_slots: usize,
    dirty_ranges: usize,
    removed_slots: usize,
    upload_bytes: usize,
    scene_full_rebuilds: u64,
    render_full_rebuilds: u64,
}

#[derive(Clone, Debug, Default)]
struct RequestWorkMetrics {
    document_nodes_scanned: u64,
    document_sequence_entries_examined: u64,
    document_sequence_entries_copied: u64,
    document_sequence_entries_moved_or_shifted: u64,
    document_sequence_rank_order_comparisons: u64,
    document_sequence_nodes_allocated: u64,
    document_sequence_allocated_bytes: u64,
    document_sequence_tree_rebalances: u64,
    document_sequence_maximum_depth: u64,
    document_sequence_fallback_or_rebuild_count: u64,
    document_sequence_full_scans: u64,
    document_sequence_full_copies: u64,
    document_sequence_dense_index_rewrites: u64,
    scene_sequence_entries_examined: u64,
    scene_sequence_entries_copied: u64,
    scene_sequence_entries_moved_or_shifted: u64,
    scene_sequence_rank_order_comparisons: u64,
    scene_sequence_nodes_allocated: u64,
    scene_sequence_allocated_bytes: u64,
    scene_sequence_tree_rebalances: u64,
    scene_sequence_maximum_depth: u64,
    scene_sequence_fallback_or_rebuild_count: u64,
    scene_sequence_full_scans: u64,
    scene_sequence_full_copies: u64,
    scene_sequence_dense_index_rewrites: u64,
    scene_nodes_visited: u64,
    render_items_planned: u64,
    render_items_read: u64,
    render_items_written: u64,
    render_items_cloned: u64,
    order_nodes_visited: u64,
    sibling_search_steps: u64,
    full_render_model_scans: u64,
    culling_candidates: u64,
    visible_items: u64,
    gpu_encode_attempted: u64,
    gpu_encode_omitted: u64,
    instance_upload_bytes: u64,
    visible_slot_upload_bytes: u64,
    allocation_growth_count: u64,
    document_full_clones: u64,
    document_nodes_touched: u64,
    ui_full_snapshots: u64,
    ui_nodes_serialized: u64,
    ui_child_ids_serialized: u64,
    ui_projection_payload_bytes: u64,
    ui_structural_operations: u64,
    ui_delta_nodes: u64,
}

#[derive(Debug, Default)]
struct PendingProjection {
    full: bool,
    upserts: Vec<Value>,
    removed: Vec<String>,
    structural_ops: Vec<Value>,
}

#[derive(Debug, Default)]
struct PendingBinary {
    full_instances: Vec<u8>,
    dirty_instances: Vec<u8>,
    removed_slots: Vec<u32>,
    visible_slots: Vec<u32>,
    has_full_instances: bool,
    has_dirty_instances: bool,
    has_removed_slots: bool,
    has_visible_slots: bool,
}

#[derive(Clone, Debug)]
struct HostFailure {
    code: &'static str,
    message: String,
}

impl HostFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<RuntimeError> for HostFailure {
    fn from(error: RuntimeError) -> Self {
        Self::new("runtime_error", error.to_string())
    }
}

impl From<CameraError> for HostFailure {
    fn from(error: CameraError) -> Self {
        Self::new("camera_error", error.to_string())
    }
}

/// Worker-owned runtime facade. No Document mutator crosses this boundary.
#[wasm_bindgen]
pub struct EngineHost {
    runtime: EngineRuntime,
    fixture: FixtureKind,
    active_root: NodeId,
    drag_base: Option<DragBase>,
    pending: PendingBinary,
    projection: PendingProjection,
    last_culling: CullingResult,
    last_delta: DeltaReport,
    fallback_rebuild_count: u64,
    last_error: Option<HostFailure>,
    engine_sequence: u64,
    request_metrics: RequestWorkMetrics,
}

#[wasm_bindgen]
impl EngineHost {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<EngineHost, JsValue> {
        Self::new_inner().map_err(|error| JsValue::from_str(&error.message))
    }

    #[wasm_bindgen(js_name = protocolVersion)]
    pub fn protocol_version() -> u32 {
        PROTOCOL_VERSION
    }

    #[wasm_bindgen(js_name = renderBinarySchemaVersion)]
    pub fn render_binary_schema_version() -> u32 {
        RENDER_BINARY_SCHEMA_VERSION
    }

    /// Accepts one versioned request and returns one versioned, correlated JSON response.
    #[wasm_bindgen(js_name = handleJson)]
    pub fn handle_json(&mut self, request_json: &str) -> String {
        self.pending = PendingBinary::default();
        self.projection = PendingProjection::default();
        self.last_delta = DeltaReport::default();
        self.request_metrics = RequestWorkMetrics::default();
        self.engine_sequence = self.engine_sequence.saturating_add(1);
        let request_id = serde_json::from_str::<Value>(request_json)
            .ok()
            .and_then(|value| {
                value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unparseable".to_owned());
        let request = match serde_json::from_str::<RequestEnvelope>(request_json) {
            Ok(request) => request,
            Err(error) => {
                let failure = HostFailure::new("invalid_request", error.to_string());
                self.last_error = Some(failure.clone());
                return self.response_json(&request_id, false, Value::Null, Some(&failure), 0.0);
            }
        };
        if request.protocol_version != PROTOCOL_VERSION {
            let failure = HostFailure::new(
                "protocol_version_mismatch",
                format!(
                    "protocol version {} is unsupported; expected {PROTOCOL_VERSION}",
                    request.protocol_version
                ),
            );
            self.last_error = Some(failure.clone());
            return self.response_json(
                &request.request_id,
                false,
                Value::Null,
                Some(&failure),
                0.0,
            );
        }

        let started = now_ms();
        match self.execute(request.operation) {
            Ok(result) => {
                self.last_error = None;
                self.response_json(&request.request_id, true, result, None, now_ms() - started)
            }
            Err(failure) => {
                self.last_error = Some(failure.clone());
                self.response_json(
                    &request.request_id,
                    false,
                    Value::Null,
                    Some(&failure),
                    now_ms() - started,
                )
            }
        }
    }

    #[wasm_bindgen(js_name = takeFullInstances)]
    pub fn take_full_instances(&mut self) -> Uint8Array {
        Uint8Array::from(std::mem::take(&mut self.pending.full_instances).as_slice())
    }

    #[wasm_bindgen(js_name = takeDirtyInstances)]
    pub fn take_dirty_instances(&mut self) -> Uint8Array {
        Uint8Array::from(std::mem::take(&mut self.pending.dirty_instances).as_slice())
    }

    #[wasm_bindgen(js_name = takeRemovedSlots)]
    pub fn take_removed_slots(&mut self) -> Uint32Array {
        Uint32Array::from(std::mem::take(&mut self.pending.removed_slots).as_slice())
    }

    #[wasm_bindgen(js_name = takeVisibleSlots)]
    pub fn take_visible_slots(&mut self) -> Uint32Array {
        Uint32Array::from(std::mem::take(&mut self.pending.visible_slots).as_slice())
    }
}

impl EngineHost {
    fn new_inner() -> Result<Self, HostFailure> {
        let fixture = FixtureKind::Preview;
        let runtime = EngineRuntime::new(
            build_fixture(fixture).map_err(document_failure)?,
            Camera::default(),
        )?;
        let active_root = default_active_root(runtime.document());
        let mut host = Self {
            runtime,
            fixture,
            active_root,
            drag_base: None,
            pending: PendingBinary::default(),
            projection: PendingProjection::default(),
            last_culling: CullingResult::default(),
            last_delta: DeltaReport::default(),
            fallback_rebuild_count: 0,
            last_error: None,
            engine_sequence: 0,
            request_metrics: RequestWorkMetrics::default(),
        };
        host.prepare_full_frame()?;
        Ok(host)
    }

    fn execute(&mut self, request: HostRequest) -> Result<Value, HostFailure> {
        match request {
            HostRequest::Initialize => {
                self.prepare_full_frame()?;
                self.prepare_full_projection()?;
                Ok(json!({ "initialized": true, "owner": "dedicated-worker" }))
            }
            HostRequest::LoadFixture { fixture } => {
                let kind = parse_fixture(&fixture)?;
                let document = build_fixture(kind).map_err(document_failure)?;
                let outcome = self.runtime.replace_document(document)?;
                self.fixture = kind;
                self.active_root = default_active_root(self.runtime.document());
                self.track_scene_outcome(&outcome);
                self.reset_camera()?;
                self.prepare_full_frame_with_delta(&outcome.render)?;
                self.prepare_full_projection()?;
                Ok(json!({ "fixture": kind.label() }))
            }
            HostRequest::Command { command } => {
                let structural = command.structural_nodes_touched().is_some();
                let command = command.into_command(self.runtime.document())?;
                let outcome = self.runtime.dispatch(command)?;
                self.ensure_active_root();
                if structural {
                    self.request_metrics.document_nodes_touched =
                        outcome.command.affected().len() as u64;
                }
                self.track_command_outcome(&outcome);
                self.prepare_incremental_frame(&outcome.render)?;
                self.prepare_projection_delta(outcome.command.change_set())?;
                Ok(json!({ "changed": outcome.command.changed() }))
            }
            HostRequest::CommandBatch { commands } => {
                let commands = commands
                    .into_iter()
                    .map(|command| command.into_command(self.runtime.document()))
                    .collect::<Result<Vec<_>, _>>()?;
                let outcome = self.runtime.apply_transactional_batch(commands)?;
                self.ensure_active_root();
                self.consume_batch(&outcome)?;
                Ok(json!({
                    "changed": outcome.changed(),
                    "applied": outcome.applied(),
                    "committed": outcome.committed,
                }))
            }
            HostRequest::BeginTransaction => {
                self.runtime.begin_transaction()?;
                self.drag_base = Some(self.capture_drag_base());
                Ok(json!({
                    "transaction_active": true,
                    "drag_targets": self
                        .drag_base
                        .as_ref()
                        .map_or(0, |base| base.targets.len()),
                }))
            }
            HostRequest::UpdateTransaction { command } => {
                let structural = command.structural_nodes_touched().is_some();
                let command = command.into_command(self.runtime.document())?;
                let outcome = self.runtime.update_transaction(command)?;
                if structural {
                    self.request_metrics.document_nodes_touched =
                        outcome.command.affected().len() as u64;
                }
                self.track_command_outcome(&outcome);
                self.prepare_incremental_frame(&outcome.render)?;
                self.prepare_projection_delta(outcome.command.change_set())?;
                Ok(json!({ "changed": outcome.command.changed(), "preview": true }))
            }
            HostRequest::CommitTransaction => {
                let committed = self.runtime.commit_transaction()?;
                self.ensure_active_root();
                self.drag_base = None;
                Ok(json!({ "committed": committed }))
            }
            HostRequest::RollbackTransaction => {
                let outcome = self.runtime.rollback_transaction()?;
                self.ensure_active_root();
                self.drag_base = None;
                self.track_scene_outcome(&outcome);
                self.prepare_incremental_frame(&outcome.render)?;
                self.prepare_projection_delta(&outcome.change_set)?;
                Ok(json!({ "rolled_back": true }))
            }
            HostRequest::Undo => {
                if let Some(outcome) = self.runtime.undo()? {
                    self.ensure_active_root();
                    self.track_scene_outcome(&outcome);
                    self.prepare_incremental_frame(&outcome.render)?;
                    self.prepare_projection_delta(&outcome.change_set)?;
                    Ok(json!({ "changed": true }))
                } else {
                    Ok(json!({ "changed": false }))
                }
            }
            HostRequest::Redo => {
                if let Some(outcome) = self.runtime.redo()? {
                    self.ensure_active_root();
                    self.track_scene_outcome(&outcome);
                    self.prepare_incremental_frame(&outcome.render)?;
                    self.prepare_projection_delta(&outcome.change_set)?;
                    Ok(json!({ "changed": true }))
                } else {
                    Ok(json!({ "changed": false }))
                }
            }
            HostRequest::Camera { camera } => {
                self.update_camera(camera)?;
                self.prepare_visibility_frame()?;
                Ok(json!({ "camera_changed": true }))
            }
            HostRequest::HitTest {
                x,
                y,
                root_id,
                selectable_only,
            } => {
                let hit = self
                    .runtime
                    .hit_test_viewport(ViewportPoint(Vec2::new(x, y)))?;
                let root = root_id.as_deref().map(parse_node_id).transpose()?;
                let all = hit
                    .all()
                    .iter()
                    .copied()
                    .filter(|id| {
                        !selectable_only
                            || match root {
                                Some(root) => self.selectable_in_root(*id, root),
                                None => true,
                            }
                    })
                    .collect::<Vec<_>>();
                Ok(json!({
                    "topmost": all.first().map(ToString::to_string),
                    "all": all.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "candidates": hit.candidate_count(),
                    "exact_geometry_tests": hit.exact_geometry_test_count(),
                }))
            }
            HostRequest::Selection {
                target,
                targets,
                mode,
            } => {
                self.update_selection(target.as_deref(), targets.as_deref(), &mode)?;
                Ok(json!({
                    "selection_changed": true,
                    "selection_count": self.runtime.selection().len(),
                }))
            }
            HostRequest::SetActiveRoot { node_id } => {
                let node_id = parse_node_id(&node_id)?;
                let node = self.runtime.document().node(node_id).ok_or_else(|| {
                    HostFailure::new("invalid_active_root", "active slide does not exist")
                })?;
                if node.kind() != NodeKind::Frame
                    || node.parent() != Some(self.runtime.document().root_id())
                {
                    return Err(HostFailure::new(
                        "invalid_active_root",
                        "active slide must be a top-level Frame",
                    ));
                }
                self.active_root = node_id;
                self.runtime.clear_selection();
                Ok(json!({ "active_root": node_id.to_string() }))
            }
            HostRequest::MarqueeSelect {
                x0,
                y0,
                x1,
                y1,
                additive,
                root_id,
                exclude_ids,
            } => self.marquee_select(
                MarqueeRequest {
                    x0,
                    y0,
                    x1,
                    y1,
                    additive,
                },
                root_id.as_deref(),
                exclude_ids.as_deref(),
            ),
            HostRequest::TranslateSelection {
                dx,
                dy,
                snap,
                snap_threshold_px,
            } => self.translate_selection(dx, dy, snap, snap_threshold_px),
            HostRequest::TransformSelection { matrix } => {
                self.transform_selection(matrix)
            }
            HostRequest::Arrange { operation } => self.arrange_selection(&operation),
            HostRequest::GetUiSnapshot => {
                self.prepare_full_projection()?;
                Ok(json!({ "ui_snapshot": true }))
            }
            HostRequest::SaveDocument => {
                let document_json =
                    visual_authoring_serialization::to_json_pretty(self.runtime.document())
                        .map_err(|error| {
                            HostFailure::new("document_save_failed", error.to_string())
                        })?;
                Ok(json!({ "document_bytes": document_json.len(), "document_json": document_json }))
            }
            HostRequest::LoadDocument { document_json } => {
                let document = visual_authoring_serialization::from_json(&document_json)
                    .map_err(|error| HostFailure::new("document_load_failed", error.to_string()))?;
                let outcome = self.runtime.replace_document(document)?;
                self.active_root = default_active_root(self.runtime.document());
                self.track_scene_outcome(&outcome);
                self.prepare_full_frame_with_delta(&outcome.render)?;
                self.prepare_full_projection()?;
                Ok(json!({ "document_bytes": document_json.len(), "loaded": true }))
            }
            HostRequest::Heartbeat => Ok(json!({ "heartbeat": true })),
        }
    }

    fn update_selection(
        &mut self,
        target: Option<&str>,
        targets: Option<&[String]>,
        mode: &str,
    ) -> Result<(), HostFailure> {
        let parsed = target.map(parse_node_id).transpose()?;
        if let Some(id) = parsed {
            if !self.is_descendant_of_root(id, self.active_root) {
                return Err(HostFailure::new(
                    "selection_outside_active_root",
                    "selection target is not an editable object in the active slide",
                ));
            }
        }
        match mode {
            "set" | "extend" => {
                let ids = parse_node_ids(targets.ok_or_else(|| {
                    HostFailure::new(
                        "invalid_selection",
                        format!("{mode} selection requires a targets array"),
                    )
                })?)?;
                if ids
                    .iter()
                    .any(|id| !self.is_descendant_of_root(*id, self.active_root))
                {
                    return Err(HostFailure::new(
                        "selection_outside_active_root",
                        "every selection target must be editable inside the active slide",
                    ));
                }
                if mode == "set" {
                    self.runtime.select_many(&ids)?;
                } else {
                    self.runtime.extend_selection(&ids)?;
                }
            }
            "clear" => self.runtime.clear_selection(),
            "replace" => self.runtime.select_only(parsed.ok_or_else(|| {
                HostFailure::new("invalid_selection", "replace selection requires a target")
            })?)?,
            "add" => {
                self.runtime.add_to_selection(parsed.ok_or_else(|| {
                    HostFailure::new("invalid_selection", "add selection requires a target")
                })?)?;
            }
            "toggle" => {
                self.runtime.toggle_selection(parsed.ok_or_else(|| {
                    HostFailure::new("invalid_selection", "toggle selection requires a target")
                })?)?;
            }
            _ => {
                return Err(HostFailure::new(
                    "invalid_selection",
                    format!("unknown selection mode {mode}"),
                ));
            }
        }
        Ok(())
    }

    /// Default snap radius in device-independent viewport pixels.
    const DEFAULT_SNAP_THRESHOLD_PX: f64 = 8.0;

    /// Captures the geometry a drag starts from: local transforms, parent world transforms, and
    /// the union of the selection's world bounds used for snapping.
    fn capture_drag_base(&self) -> DragBase {
        let mut base = DragBase::default();
        for id in self.runtime.selection().ordered() {
            let id = *id;
            let Some(node) = self.runtime.document().node(id) else {
                continue;
            };
            let mut ancestor = node.parent();
            let mut selected_ancestor = false;
            while let Some(candidate) = ancestor {
                if self.runtime.selection().contains(candidate) {
                    selected_ancestor = true;
                    break;
                }
                ancestor = self
                    .runtime
                    .document()
                    .node(candidate)
                    .and_then(|candidate_node| candidate_node.parent());
            }
            if selected_ancestor {
                continue;
            }
            if self.locked_for_edit(id) {
                base.skipped_locked += 1;
                continue;
            }
            let Some(parent) = node.parent() else {
                continue;
            };
            let Some(parent_world) = self
                .runtime
                .scene()
                .node(parent)
                .and_then(visual_authoring_scene::SceneNode::world_transform)
            else {
                continue;
            };
            if let Some(bounds) = self
                .runtime
                .scene()
                .node(id)
                .and_then(visual_authoring_scene::SceneNode::subtree_world_bounds)
            {
                base.union_bounds = Some(match base.union_bounds {
                    Some(current) => union_rect(current, bounds),
                    None => bounds,
                });
            }
            base.targets.push(DragTarget {
                id,
                local_transform: node.local_transform(),
                parent_world,
            });
        }
        base
    }

    /// Mirrors the command layer's rule that a node inside a locked container cannot be edited.
    fn locked_for_edit(&self, id: NodeId) -> bool {
        let mut current = Some(id);
        let mut depth = 0_usize;
        while let Some(candidate) = current {
            let Some(node) = self.runtime.document().node(candidate) else {
                return true;
            };
            if node.locked() {
                return true;
            }
            depth += 1;
            if depth > self.runtime.document().len() {
                return true;
            }
            current = node.parent();
        }
        false
    }

    fn ensure_active_root(&mut self) {
        let valid = self.runtime.document().node(self.active_root).is_some_and(|node| {
            node.kind() == NodeKind::Frame
                && node.parent() == Some(self.runtime.document().root_id())
        });
        if !valid {
            self.active_root = default_active_root(self.runtime.document());
        }
    }

    /// Canvas selection is restricted to visible, unlocked descendants of the active slide.
    /// The root itself is a slide backdrop, not a movable object.
    fn selectable_in_root(&self, id: NodeId, root: NodeId) -> bool {
        if !self.is_descendant_of_root(id, root) || self.locked_for_edit(id) {
            return false;
        }
        let mut current = Some(id);
        let mut depth = 0_usize;
        while let Some(candidate) = current {
            let Some(node) = self.runtime.document().node(candidate) else {
                return false;
            };
            if !node.visible() {
                return false;
            }
            current = node.parent();
            depth += 1;
            if depth > self.runtime.document().len() {
                return false;
            }
        }
        true
    }

    fn is_descendant_of_root(&self, id: NodeId, root: NodeId) -> bool {
        if id == root {
            return false;
        }
        let mut current = self.runtime.document().node(id).and_then(|node| node.parent());
        let mut depth = 0_usize;
        while let Some(candidate) = current {
            if candidate == root {
                return true;
            }
            current = self
                .runtime
                .document()
                .node(candidate)
                .and_then(|node| node.parent());
            depth += 1;
            if depth > self.runtime.document().len() {
                return false;
            }
        }
        false
    }

    fn marquee_select(
        &mut self,
        request: MarqueeRequest,
        root_id: Option<&str>,
        exclude_ids: Option<&[String]>,
    ) -> Result<Value, HostFailure> {
        let MarqueeRequest {
            x0,
            y0,
            x1,
            y1,
            additive,
        } = request;
        let first = self
            .runtime
            .camera()
            .viewport_to_world(ViewportPoint(Vec2::new(x0, y0)))?;
        let second = self
            .runtime
            .camera()
            .viewport_to_world(ViewportPoint(Vec2::new(x1, y1)))?;
        let bounds = Rect::from_min_max(
            Vec2::new(first.0.x.min(second.0.x), first.0.y.min(second.0.y)),
            Vec2::new(first.0.x.max(second.0.x), first.0.y.max(second.0.y)),
        );
        let root = match root_id {
            Some(value) => parse_node_id(value)?,
            None => self.runtime.document().root_id(),
        };
        let excluded = match exclude_ids {
            Some(values) => parse_node_ids(values)?,
            None => Vec::new(),
        };
        let result = self.runtime.marquee_candidates(bounds, root, &excluded)?;
        let selected = result
            .selected()
            .iter()
            .copied()
            .filter(|id| self.selectable_in_root(*id, root))
            .collect::<Vec<_>>();
        if additive {
            self.runtime.extend_selection(&selected)?;
        } else {
            self.runtime.select_many(&selected)?;
        }
        Ok(json!({
            "selected": selected.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "selection_count": self.runtime.selection().len(),
            "candidates": result.candidate_count(),
            "candidates_examined": result.candidates_examined(),
            "world_bounds": [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y],
        }))
    }

    /// Moves every dragged node by one world delta, optionally corrected by snapping.
    ///
    /// The delta is always measured from the transaction's captured base, so repeating or
    /// coalescing pointer frames can never accumulate drift.
    fn translate_selection(
        &mut self,
        dx: f64,
        dy: f64,
        snap: bool,
        snap_threshold_px: Option<f64>,
    ) -> Result<Value, HostFailure> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(HostFailure::new(
                "invalid_translation",
                "translation delta must be finite",
            ));
        }
        if !self.runtime.transaction_active() {
            return Err(HostFailure::new(
                "no_transaction",
                "translate_selection requires an active transaction",
            ));
        }
        let base = self.drag_base.clone().ok_or_else(|| {
            HostFailure::new(
                "no_drag_base",
                "the active transaction captured no draggable selection",
            )
        })?;
        let requested = Vec2::new(dx, dy);
        if base.targets.is_empty() {
            return Ok(json!({
                "moved": 0,
                "skipped_locked": base.skipped_locked,
                "requested_delta": [requested.x, requested.y],
                "applied_delta": [requested.x, requested.y],
                "snapped": false,
                "guides": Vec::<Value>::new(),
            }));
        }

        let mut applied = requested;
        let mut guides = Vec::new();
        let mut snapped = false;
        let mut candidates_examined = 0_u64;
        if snap {
            if let Some(bounds) = base.union_bounds {
                let threshold_px = snap_threshold_px.unwrap_or(Self::DEFAULT_SNAP_THRESHOLD_PX);
                if !threshold_px.is_finite() || threshold_px < 0.0 {
                    return Err(HostFailure::new(
                        "invalid_snap_threshold",
                        "snap threshold must be a finite, non-negative pixel distance",
                    ));
                }
                let threshold_world = threshold_px / self.runtime.camera().zoom();
                let proposed = Rect::from_min_max(
                    Vec2::new(bounds.min.x + requested.x, bounds.min.y + requested.y),
                    Vec2::new(bounds.max.x + requested.x, bounds.max.y + requested.y),
                );
                let moving = base
                    .targets
                    .iter()
                    .map(|target| target.id)
                    .collect::<Vec<_>>();
                let resolution = self
                    .runtime
                    .resolve_snap(proposed, &moving, threshold_world)?;
                applied = requested + resolution.correction();
                snapped = resolution.snapped();
                candidates_examined = resolution.candidates_examined();
                guides = resolution
                    .guides()
                    .iter()
                    .map(|guide| {
                        json!({
                            "axis": guide.axis.label(),
                            "position": guide.position,
                            "start": guide.start,
                            "end": guide.end,
                            "target": guide.target.to_string(),
                        })
                    })
                    .collect();
            }
        }

        let mut commands = Vec::with_capacity(base.targets.len());
        for target in &base.targets {
            let local_delta =
                world_delta_to_local(target.parent_world, applied).ok_or_else(|| {
                    HostFailure::new(
                        "invalid_translation",
                        format!("node {} has no invertible parent transform", target.id),
                    )
                })?;
            commands.push(Command::SetLocalTransform {
                target: target.id,
                transform: translated(target.local_transform, local_delta),
            });
        }
        let moved = commands.len();
        let outcome = match self.runtime.apply_batch_in_transaction(commands) {
            Ok(outcome) => outcome,
            Err(error) => {
                // Failure atomicity: a drag frame never leaves part of the selection moved.
                self.drag_base = None;
                let rolled_back = self.runtime.rollback_transaction()?;
                self.track_scene_outcome(&rolled_back);
                self.prepare_incremental_frame(&rolled_back.render)?;
                self.prepare_projection_delta(&rolled_back.change_set)?;
                return Err(error.into());
            }
        };
        self.consume_batch(&outcome)?;
        Ok(json!({
            "moved": moved,
            "skipped_locked": base.skipped_locked,
            "requested_delta": [requested.x, requested.y],
            "applied_delta": [applied.x, applied.y],
            "snapped": snapped,
            "snap_candidates_examined": candidates_examined,
            "guides": guides,
            "preview": true,
        }))
    }

    /// Applies one viewport-authored world transform to every editable selected node.
    ///
    /// Every pointer frame starts from the transaction capture, so resize and rotation do not
    /// accumulate floating-point drift. Converting back through each parent keeps nested and
    /// grouped selections in their own local coordinate systems.
    fn transform_selection(&mut self, matrix: [f64; 6]) -> Result<Value, HostFailure> {
        require_finite(&matrix, "selection transform")?;
        if !self.runtime.transaction_active() {
            return Err(HostFailure::new(
                "no_transaction",
                "transform_selection requires an active transaction",
            ));
        }
        let world_delta = Affine2::from_components(
            matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5],
        );
        let determinant = world_delta.determinant();
        if !determinant.is_finite() || determinant <= f64::EPSILON * 16.0 {
            return Err(HostFailure::new(
                "invalid_selection_transform",
                "selection transform must be finite, non-mirrored, and invertible",
            ));
        }
        let base = self.drag_base.clone().ok_or_else(|| {
            HostFailure::new(
                "no_drag_base",
                "the active transaction captured no transformable selection",
            )
        })?;
        let mut commands = Vec::with_capacity(base.targets.len());
        for target in &base.targets {
            let parent_inverse = target.parent_world.inverse().ok_or_else(|| {
                HostFailure::new(
                    "invalid_selection_transform",
                    format!("node {} has no invertible parent transform", target.id),
                )
            })?;
            let transform = parent_inverse
                * world_delta
                * target.parent_world
                * target.local_transform;
            if !transform.is_finite() || transform.inverse().is_none() {
                return Err(HostFailure::new(
                    "invalid_selection_transform",
                    format!("node {} would receive an invalid transform", target.id),
                ));
            }
            commands.push(Command::SetLocalTransform {
                target: target.id,
                transform,
            });
        }
        let transformed = commands.len();
        let outcome = match self.runtime.apply_batch_in_transaction(commands) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.drag_base = None;
                let rolled_back = self.runtime.rollback_transaction()?;
                self.track_scene_outcome(&rolled_back);
                self.prepare_incremental_frame(&rolled_back.render)?;
                self.prepare_projection_delta(&rolled_back.change_set)?;
                return Err(error.into());
            }
        };
        self.consume_batch(&outcome)?;
        Ok(json!({
            "transformed": transformed,
            "skipped_locked": base.skipped_locked,
            "preview": true,
        }))
    }

    /// Aligns or distributes the current selection as one atomic, undoable operation.
    fn arrange_selection(&mut self, operation: &str) -> Result<Value, HostFailure> {
        let targets = self.runtime.selection().ordered().to_vec();
        let plan = match operation {
            "align_left" => self.runtime.plan_align(&targets, AlignMode::Left),
            "align_horizontal_center" => self
                .runtime
                .plan_align(&targets, AlignMode::HorizontalCenter),
            "align_right" => self.runtime.plan_align(&targets, AlignMode::Right),
            "align_top" => self.runtime.plan_align(&targets, AlignMode::Top),
            "align_vertical_center" => self.runtime.plan_align(&targets, AlignMode::VerticalCenter),
            "align_bottom" => self.runtime.plan_align(&targets, AlignMode::Bottom),
            "distribute_horizontal" => self
                .runtime
                .plan_distribute(&targets, DistributeAxis::Horizontal),
            "distribute_vertical" => self
                .runtime
                .plan_distribute(&targets, DistributeAxis::Vertical),
            _ => {
                return Err(HostFailure::new(
                    "invalid_arrange",
                    format!("unknown arrange operation {operation}"),
                ))
            }
        }?;
        let reference = plan.reference();
        let unchanged = plan.unchanged_targets();
        let outcome = self.runtime.apply_arrange(&plan)?;
        self.consume_batch(&outcome)?;
        Ok(json!({
            "operation": operation,
            "moved": outcome.applied(),
            "unchanged": unchanged,
            "changed": outcome.changed(),
            "reference": [reference.min.x, reference.min.y, reference.max.x, reference.max.y],
        }))
    }

    /// Reports one batched edit as a single frame and a single projection delta.
    fn consume_batch(&mut self, outcome: &BatchOutcome) -> Result<(), HostFailure> {
        for command_outcome in &outcome.outcomes {
            self.track_command_outcome(command_outcome);
        }
        self.prepare_incremental_frame(&outcome.render)?;
        self.prepare_projection_delta(&outcome.change_set)?;
        Ok(())
    }

    fn prepare_full_projection(&mut self) -> Result<(), HostFailure> {
        let root = self.runtime.document().root_id();
        let mut stack = vec![root];
        let mut upserts = Vec::with_capacity(self.runtime.document().len());
        while let Some(id) = stack.pop() {
            let node = self.runtime.document().node(id).ok_or_else(|| {
                HostFailure::new("projection_node_missing", format!("node {id} is missing"))
            })?;
            for child in node.children().iter().rev() {
                stack.push(*child);
            }
            upserts.push(self.project_node(id)?);
        }
        self.request_metrics.ui_full_snapshots += 1;
        self.request_metrics.ui_nodes_serialized += upserts.len() as u64;
        self.request_metrics.ui_child_ids_serialized += upserts
            .iter()
            .filter_map(|node| node.get("children").and_then(Value::as_array))
            .map(|children| children.len() as u64)
            .sum::<u64>();
        self.projection = PendingProjection {
            full: true,
            upserts,
            removed: Vec::new(),
            structural_ops: Vec::new(),
        };
        self.finish_projection_metrics();
        Ok(())
    }

    fn prepare_projection_delta(
        &mut self,
        change_set: &DocumentChangeSet,
    ) -> Result<(), HostFailure> {
        let mut upsert_ids = BTreeSet::new();
        let mut removed = BTreeSet::new();
        let mut structural_ops = Vec::new();
        let mut compact_delta_nodes = 0_u64;
        for change in change_set.changes() {
            match change {
                DocumentChange::NodesInserted {
                    root,
                    nodes,
                    placement,
                } => {
                    upsert_ids.extend(nodes.iter().copied());
                    if let Some(placement) = placement {
                        structural_ops.push(json!({
                            "kind": "attach",
                            "node_id": root.to_string(),
                            "parent_id": placement.parent.to_string(),
                            "index": placement.index,
                        }));
                    }
                }
                DocumentChange::NodesRemoved {
                    root,
                    nodes,
                    placement,
                } => {
                    removed.extend(nodes.iter().map(ToString::to_string));
                    if let Some(placement) = placement {
                        structural_ops.push(json!({
                            "kind": "detach",
                            "node_id": root.to_string(),
                            "parent_id": placement.parent.to_string(),
                            "index": placement.index,
                        }));
                    }
                }
                DocumentChange::PlacementChanged {
                    root,
                    before,
                    after,
                    ..
                } => {
                    upsert_ids.insert(*root);
                    structural_ops.push(json!({
                        "kind": "move",
                        "node_id": root.to_string(),
                        "before_parent_id": before.map(|placement| placement.parent.to_string()),
                        "before_index": before.map(|placement| placement.index),
                        "parent_id": after.map(|placement| placement.parent.to_string()),
                        "index": after.map(|placement| placement.index),
                    }));
                }
                DocumentChange::StructuralGroupChanged {
                    direction,
                    group,
                    parent,
                    index,
                    children,
                    positions,
                } => {
                    let child_local_transforms = children
                        .iter()
                        .map(|child| {
                            let local = self
                                .runtime
                                .document()
                                .node(*child)
                                .ok_or_else(|| {
                                    HostFailure::new(
                                        "projection_node_missing",
                                        format!("node {child} is missing"),
                                    )
                                })?
                                .local_transform();
                            Ok([
                                local.m11, local.m12, local.m21, local.m22, local.tx, local.ty,
                            ])
                        })
                        .collect::<Result<Vec<_>, HostFailure>>()?;
                    compact_delta_nodes += children.len() as u64;
                    match direction {
                        StructuralGroupChange::Grouped => {
                            upsert_ids.insert(*group);
                        }
                        StructuralGroupChange::Ungrouped => {
                            removed.insert(group.to_string());
                        }
                    }
                    structural_ops.push(json!({
                        "kind": match direction {
                            StructuralGroupChange::Grouped => "group",
                            StructuralGroupChange::Ungrouped => "ungroup",
                        },
                        "group_id": group.to_string(),
                        "parent_id": parent.to_string(),
                        "index": index,
                        "children": children.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "child_local_transforms": child_local_transforms,
                        "positions": positions,
                    }));
                }
                DocumentChange::LocalTransformChanged { node }
                | DocumentChange::GeometryChanged { node }
                | DocumentChange::VisibilityChanged { node }
                | DocumentChange::AppearanceChanged { node, .. }
                | DocumentChange::PersistentPropertyChanged { node, .. } => {
                    upsert_ids.insert(*node);
                }
                DocumentChange::FullDocumentReset => return self.prepare_full_projection(),
            }
        }
        let upserts = upsert_ids
            .into_iter()
            .filter(|id| self.runtime.document().node(*id).is_some())
            .map(|id| self.project_node(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.request_metrics.ui_delta_nodes += upserts.len() as u64 + compact_delta_nodes;
        self.request_metrics.ui_nodes_serialized += upserts.len() as u64;
        self.request_metrics.ui_child_ids_serialized += upserts
            .iter()
            .filter_map(|node| node.get("children").and_then(Value::as_array))
            .map(|children| children.len() as u64)
            .sum::<u64>();
        self.request_metrics.ui_child_ids_serialized += structural_ops
            .iter()
            .filter_map(|operation| operation.get("children").and_then(Value::as_array))
            .map(|children| children.len() as u64)
            .sum::<u64>();
        self.request_metrics.ui_structural_operations += structural_ops.len() as u64;
        self.projection = PendingProjection {
            full: false,
            upserts,
            removed: removed.into_iter().collect(),
            structural_ops,
        };
        self.finish_projection_metrics();
        Ok(())
    }

    fn finish_projection_metrics(&mut self) {
        self.request_metrics.ui_projection_payload_bytes = serde_json::to_vec(&json!({
            "schema_version": 2,
            "full": self.projection.full,
            "upserts": self.projection.upserts,
            "removed": self.projection.removed,
            "structural_ops": self.projection.structural_ops,
        }))
        .map(|payload| payload.len() as u64)
        .unwrap_or(0);
    }

    fn project_node(&self, id: NodeId) -> Result<Value, HostFailure> {
        let node = self.runtime.document().node(id).ok_or_else(|| {
            HostFailure::new("projection_node_missing", format!("node {id} is missing"))
        })?;
        let local = node.local_transform();
        let scene = self.runtime.scene().node(id);
        let world = scene.and_then(visual_authoring_scene::SceneNode::world_transform);
        let bounds = scene.and_then(visual_authoring_scene::SceneNode::own_world_bounds);
        let size = node.geometry().map(Geometry::size);
        Ok(json!({
            "id": id.to_string(),
            "parent_id": node.parent().map(|parent| parent.to_string()),
            "children": node.children().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "name": node.name(),
            "kind": node_kind_label(node.kind()),
            "visible": node.visible(),
            "locked": node.locked(),
            "opacity": node.appearance().opacity,
            "appearance": {
                "fill": [
                    node.appearance().fill.r,
                    node.appearance().fill.g,
                    node.appearance().fill.b,
                    node.appearance().fill.a
                ],
                "corner_radii": [
                    node.appearance().corner_radii.top_left,
                    node.appearance().corner_radii.top_right,
                    node.appearance().corner_radii.bottom_right,
                    node.appearance().corner_radii.bottom_left
                ],
                "stroke": {
                    "color": [
                        node.appearance().stroke.color.r,
                        node.appearance().stroke.color.g,
                        node.appearance().stroke.color.b,
                        node.appearance().stroke.color.a
                    ],
                    "width": node.appearance().stroke.width
                }
            },
            "local_transform": [local.m11, local.m12, local.m21, local.m22, local.tx, local.ty],
            "world_transform": world.map(|matrix| [
                matrix.m11, matrix.m12, matrix.m21, matrix.m22, matrix.tx, matrix.ty
            ]),
            "geometry": size.map(|size| json!({ "width": size.x, "height": size.y })),
            "world_bounds": bounds.map(|bounds| json!({
                "min": [bounds.min.x, bounds.min.y],
                "max": [bounds.max.x, bounds.max.y],
            })),
        }))
    }

    fn track_command_outcome(&mut self, outcome: &RuntimeCommandOutcome) {
        self.track_sync(&outcome.sync, &outcome.render_sync);
        self.track_work(
            outcome.command.sequence_work(),
            &outcome.scene,
            &outcome.render,
        );
    }

    fn track_scene_outcome(&mut self, outcome: &RuntimeSceneOutcome) {
        self.track_sync(&outcome.sync, &outcome.render_sync);
        self.track_work(outcome.sequence_work, &outcome.scene, &outcome.render);
    }

    fn track_work(
        &mut self,
        document: visual_authoring_document::SequenceWork,
        scene: &visual_authoring_scene::SceneUpdateStats,
        render: &RenderDelta,
    ) {
        let stats = &render.stats;
        self.request_metrics.document_sequence_entries_examined += document.entries_examined;
        self.request_metrics.document_sequence_entries_copied += document.entries_copied;
        self.request_metrics
            .document_sequence_entries_moved_or_shifted += document.entries_moved_or_shifted;
        self.request_metrics
            .document_sequence_rank_order_comparisons += document.rank_order_comparisons;
        self.request_metrics.document_sequence_nodes_allocated += document.sequence_nodes_allocated;
        self.request_metrics.document_sequence_allocated_bytes += document.allocated_bytes;
        self.request_metrics.document_sequence_tree_rebalances += document.tree_rebalances;
        self.request_metrics.document_sequence_maximum_depth = self
            .request_metrics
            .document_sequence_maximum_depth
            .max(document.maximum_sequence_depth);
        self.request_metrics
            .document_sequence_fallback_or_rebuild_count += document.fallback_or_rebuild_count;
        self.request_metrics.document_sequence_full_scans += document.full_sequence_scans;
        self.request_metrics.document_sequence_full_copies += document.full_sequence_copies;
        self.request_metrics.document_sequence_dense_index_rewrites +=
            document.dense_index_rewrites;
        self.request_metrics.scene_sequence_entries_examined +=
            scene.sequence_work.entries_examined;
        self.request_metrics.scene_sequence_entries_copied += scene.sequence_work.entries_copied;
        self.request_metrics.scene_sequence_entries_moved_or_shifted +=
            scene.sequence_work.entries_moved_or_shifted;
        self.request_metrics.scene_sequence_rank_order_comparisons +=
            scene.sequence_work.rank_order_comparisons;
        self.request_metrics.scene_sequence_nodes_allocated +=
            scene.sequence_work.sequence_nodes_allocated;
        self.request_metrics.scene_sequence_allocated_bytes += scene.sequence_work.allocated_bytes;
        self.request_metrics.scene_sequence_tree_rebalances += scene.sequence_work.tree_rebalances;
        self.request_metrics.scene_sequence_maximum_depth = self
            .request_metrics
            .scene_sequence_maximum_depth
            .max(scene.sequence_work.maximum_sequence_depth);
        self.request_metrics
            .scene_sequence_fallback_or_rebuild_count +=
            scene.sequence_work.fallback_or_rebuild_count;
        self.request_metrics.scene_sequence_full_scans += scene.sequence_work.full_sequence_scans;
        self.request_metrics.scene_sequence_full_copies += scene.sequence_work.full_sequence_copies;
        self.request_metrics.scene_sequence_dense_index_rewrites +=
            scene.sequence_work.dense_index_rewrites;
        self.request_metrics.document_nodes_scanned += stats.document_nodes_scanned;
        self.request_metrics.scene_nodes_visited += scene.visited_scene_nodes;
        self.request_metrics.render_items_planned += stats.render_items_planned;
        self.request_metrics.render_items_read += stats.render_items_read;
        self.request_metrics.render_items_written += stats.render_items_written;
        self.request_metrics.render_items_cloned += stats.render_items_cloned;
        self.request_metrics.order_nodes_visited +=
            scene.order_nodes_visited + stats.order_nodes_visited;
        self.request_metrics.sibling_search_steps +=
            scene.sibling_search_steps + stats.sibling_search_steps;
        self.request_metrics.full_render_model_scans += stats.full_render_model_scans;
        self.request_metrics.allocation_growth_count += stats.allocation_growth_count;
    }

    fn track_sync(&mut self, scene: &SceneSyncStatus, render: &RenderSyncStatus) {
        if matches!(scene, SceneSyncStatus::FallbackRebuild { .. }) {
            self.fallback_rebuild_count += 1;
        }
        if matches!(render, RenderSyncStatus::FallbackRebuild { .. }) {
            self.fallback_rebuild_count += 1;
        }
    }

    fn reset_camera(&mut self) -> Result<(), HostFailure> {
        let viewport = self.runtime.camera().viewport_size();
        let dpr = self.runtime.camera().device_pixel_ratio();
        *self.runtime.camera_mut() = Camera::new(WorldPoint(Vec2::ZERO), 1.0, viewport, dpr)?;
        Ok(())
    }

    fn update_camera(&mut self, request: CameraRequest) -> Result<(), HostFailure> {
        match request {
            CameraRequest::Resize { width, height, dpr } => {
                self.runtime
                    .camera_mut()
                    .resize_viewport(Vec2::new(width, height))?;
                self.runtime.camera_mut().set_device_pixel_ratio(dpr)?;
            }
            CameraRequest::Pan { dx, dy } => {
                self.runtime.camera_mut().pan(Vec2::new(dx, dy))?;
            }
            CameraRequest::Zoom { x, y, zoom } => {
                self.runtime
                    .camera_mut()
                    .zoom_around(ViewportPoint(Vec2::new(x, y)), zoom)?;
            }
            CameraRequest::Reset => self.reset_camera()?,
            CameraRequest::Fit => {
                let root = self.runtime.scene().root_id();
                let bounds = self
                    .runtime
                    .scene()
                    .node(root)
                    .and_then(|node| node.subtree_world_bounds())
                    .ok_or_else(|| {
                        HostFailure::new("fit_unavailable", "fixture has no finite bounds")
                    })?;
                self.runtime.camera_mut().fit_world_bounds(bounds, 40.0)?;
            }
            CameraRequest::FitSelection { node_id } => {
                let target = parse_node_id(&node_id)?;
                let bounds = self
                    .runtime
                    .scene()
                    .node(target)
                    .and_then(|node| {
                        node.subtree_world_bounds()
                            .or_else(|| node.own_world_bounds())
                    })
                    .ok_or_else(|| {
                        HostFailure::new("fit_unavailable", "selection has no finite bounds")
                    })?;
                self.runtime.camera_mut().fit_world_bounds(bounds, 48.0)?;
            }
        }
        Ok(())
    }

    fn prepare_full_frame(&mut self) -> Result<(), HostFailure> {
        self.prepare_full_frame_with_delta(&RenderDelta::default())
    }

    fn prepare_full_frame_with_delta(&mut self, delta: &RenderDelta) -> Result<(), HostFailure> {
        let encoded = encode_full_model(self.runtime.render_model());
        self.pending.full_instances = encoded.bytes;
        self.pending.has_full_instances = true;
        self.pending.dirty_instances.clear();
        self.pending.removed_slots.clear();
        self.pending.has_dirty_instances = false;
        self.pending.has_removed_slots = false;
        self.request_metrics.full_render_model_scans += 1;
        self.request_metrics.gpu_encode_attempted += encoded.attempted;
        self.request_metrics.gpu_encode_omitted += encoded.omitted;
        self.request_metrics.instance_upload_bytes += self.pending.full_instances.len() as u64;
        self.refresh_visibility()?;
        self.last_delta = DeltaReport {
            full: true,
            dirty_slots: self.runtime.render_model().item_count(),
            dirty_ranges: usize::from(self.runtime.render_model().item_count() > 0),
            removed_slots: 0,
            upload_bytes: self.pending.full_instances.len(),
            scene_full_rebuilds: 0,
            render_full_rebuilds: delta.stats.full_render_rebuild_count,
        };
        Ok(())
    }

    fn prepare_incremental_frame(&mut self, delta: &RenderDelta) -> Result<(), HostFailure> {
        let encoded = encode_dirty_model(self.runtime.render_model(), &delta.dirty_slots);
        self.pending.dirty_instances = encoded.bytes;
        self.pending.removed_slots.clone_from(&delta.removed_slots);
        self.pending.has_dirty_instances = !delta.dirty_slots.is_empty();
        self.pending.has_removed_slots = !delta.removed_slots.is_empty();
        self.request_metrics.gpu_encode_attempted += encoded.attempted;
        self.request_metrics.gpu_encode_omitted += encoded.omitted;
        self.request_metrics.instance_upload_bytes +=
            (delta.dirty_slots.len() * INSTANCE_STRIDE_BYTES) as u64;
        self.refresh_visibility()?;
        self.last_delta = DeltaReport {
            full: false,
            dirty_slots: delta.dirty_slots.len(),
            dirty_ranges: delta.dirty_ranges.len(),
            removed_slots: delta.removed_slots.len(),
            upload_bytes: self.pending.dirty_instances.len(),
            scene_full_rebuilds: 0,
            render_full_rebuilds: delta.stats.full_render_rebuild_count,
        };
        Ok(())
    }

    fn prepare_visibility_frame(&mut self) -> Result<(), HostFailure> {
        self.refresh_visibility()?;
        self.last_delta = DeltaReport::default();
        Ok(())
    }

    fn refresh_visibility(&mut self) -> Result<(), HostFailure> {
        let culling = self.runtime.cull_viewport()?;
        self.pending
            .visible_slots
            .clone_from(&culling.slots_bottom_to_top);
        self.pending.has_visible_slots = true;
        self.request_metrics.render_items_read += culling.render_items_read as u64;
        self.request_metrics.order_nodes_visited += culling.order_nodes_visited;
        self.request_metrics.sibling_search_steps += culling.sibling_search_steps;
        self.request_metrics.full_render_model_scans += culling.full_render_model_scans;
        self.request_metrics.culling_candidates += culling.spatial_candidates as u64;
        self.request_metrics.visible_items += culling.exact_visible as u64;

        self.request_metrics.visible_slot_upload_bytes +=
            (culling.slots_bottom_to_top.len() * std::mem::size_of::<u32>()) as u64;
        self.last_culling = culling;
        Ok(())
    }
    fn response_json(
        &self,
        request_id: &str,
        ok: bool,
        result: Value,
        error: Option<&HostFailure>,
        cpu_update_ms: f64,
    ) -> String {
        let camera = self.runtime.camera();
        let scene_metrics = self.runtime.scene().metrics();
        let history = self.runtime.history_state();
        let counters = self.runtime.render_model().counters();
        let diagnostics = self
            .runtime
            .render_model()
            .encoding_diagnostics()
            .map(|diagnostic| {
                json!({
                    "node_id": diagnostic.node_id.to_string(),
                    "reason": encoding_diagnostic_label(diagnostic.kind),
                })
            })
            .collect::<Vec<_>>();
        let work = &self.request_metrics;
        let last_error = error.or(self.last_error.as_ref());
        let value = json!({
            "type": "engine_response",
            "protocol_version": PROTOCOL_VERSION,
            "render_binary_schema_version": RENDER_BINARY_SCHEMA_VERSION,
            "engine_sequence": self.engine_sequence,
            "request_id": request_id,
            "ok": ok,
            "result": result,
            "error": last_error.map(|failure| json!({
                "code": failure.code,
                "message": failure.message,
            })),
            "runtime_owner": "dedicated-worker",
            "fixture": self.fixture.label(),
            "revisions": {
                "document": self.runtime.document_revision(),
                "scene": self.runtime.scene().revision(),
                "render": self.runtime.render_model().revision(),
            },
            "projection": {
                "schema_version": 2,
                "full": self.projection.full,
                "upserts": self.projection.upserts,
                "removed": self.projection.removed,
                "structural_ops": self.projection.structural_ops,
            },
            "selection": {
                "ordered": self.runtime.selection().ordered().iter().map(ToString::to_string).collect::<Vec<_>>(),
                "primary": self.runtime.selection().primary().map(|id| id.to_string()),
            },
            "active_root": self.active_root.to_string(),
            "history": {
                "undo_depth": history.undo_depth,
                "redo_depth": history.redo_depth,
                "transaction_active": self.runtime.transaction_active(),
            },
            "camera": {
                "center": [camera.center().0.x, camera.center().0.y],
                "zoom": camera.zoom(),
                "viewport": [camera.viewport_size().x, camera.viewport_size().y],
                "dpr": camera.device_pixel_ratio(),
            },
            "culling": {
                "total": counters.renderable_items,
                "spatial_candidates": self.last_culling.spatial_candidates,
                "visible": self.last_culling.exact_visible,
                "culled": self.last_culling.culled,
                "submitted_instances": self.last_culling.submitted_instances,
            },
            "render_delta": {
                "full": self.last_delta.full,
                "dirty_slots": self.last_delta.dirty_slots,
                "dirty_ranges": self.last_delta.dirty_ranges,
                "removed_slots": self.last_delta.removed_slots,
                "upload_bytes": self.last_delta.upload_bytes,
                "scene_full_rebuilds": self.last_delta.scene_full_rebuilds,
                "render_full_rebuilds": self.last_delta.render_full_rebuilds,
            },
            "resources": {
                "instance_stride_bytes": INSTANCE_STRIDE_BYTES,
                "dirty_record_stride_bytes": DIRTY_RECORD_STRIDE_BYTES,
                "instance_capacity": counters.allocated_slots,
                "total_render_items": counters.total_render_items,
                "renderable_items": counters.renderable_items,
                "gpu_encodable_items": counters.gpu_encodable_items,
                "gpu_omitted_items": counters.gpu_omitted_items,
                "allocated_slots": counters.allocated_slots,
                "free_slots": counters.free_slots,
            },
            "render_encoding": {
                "gpu_omitted_items": counters.gpu_omitted_items,
                "diagnostics": diagnostics,
            },
            "binary": {
                "full_instances": self.pending.has_full_instances,
                "dirty_instances": self.pending.has_dirty_instances,
                "removed_slots": self.pending.has_removed_slots,
                "visible_slots": self.pending.has_visible_slots,
            },
            "metrics": {
                "cpu_update_ms": cpu_update_ms.max(0.0),
                "document_nodes": scene_metrics.total_document_nodes,
                "indexed_nodes": scene_metrics.indexed_nodes,
                "invalid_derived_nodes": scene_metrics.invalid_derived_nodes,
                "fallback_rebuild_count": self.fallback_rebuild_count,
                "document_nodes_scanned": work.document_nodes_scanned,
                "document_sequence_entries_examined": work.document_sequence_entries_examined,
                "document_sequence_entries_copied": work.document_sequence_entries_copied,
                "document_sequence_entries_moved_or_shifted": work.document_sequence_entries_moved_or_shifted,
                "document_sequence_rank_order_comparisons": work.document_sequence_rank_order_comparisons,
                "document_sequence_nodes_allocated": work.document_sequence_nodes_allocated,
                "document_sequence_allocated_bytes": work.document_sequence_allocated_bytes,
                "document_sequence_tree_rebalances": work.document_sequence_tree_rebalances,
                "document_sequence_maximum_depth": work.document_sequence_maximum_depth,
                "document_sequence_fallback_or_rebuild_count": work.document_sequence_fallback_or_rebuild_count,
                "document_sequence_full_scans": work.document_sequence_full_scans,
                "document_sequence_full_copies": work.document_sequence_full_copies,
                "document_sequence_dense_index_rewrites": work.document_sequence_dense_index_rewrites,
                "scene_sequence_entries_examined": work.scene_sequence_entries_examined,
                "scene_sequence_entries_copied": work.scene_sequence_entries_copied,
                "scene_sequence_entries_moved_or_shifted": work.scene_sequence_entries_moved_or_shifted,
                "scene_sequence_rank_order_comparisons": work.scene_sequence_rank_order_comparisons,
                "scene_sequence_nodes_allocated": work.scene_sequence_nodes_allocated,
                "scene_sequence_allocated_bytes": work.scene_sequence_allocated_bytes,
                "scene_sequence_tree_rebalances": work.scene_sequence_tree_rebalances,
                "scene_sequence_maximum_depth": work.scene_sequence_maximum_depth,
                "scene_sequence_fallback_or_rebuild_count": work.scene_sequence_fallback_or_rebuild_count,
                "scene_sequence_full_scans": work.scene_sequence_full_scans,
                "scene_sequence_full_copies": work.scene_sequence_full_copies,
                "scene_sequence_dense_index_rewrites": work.scene_sequence_dense_index_rewrites,
                "scene_nodes_visited": work.scene_nodes_visited,
                "render_items_planned": work.render_items_planned,
                "render_items_read": work.render_items_read,
                "render_items_written": work.render_items_written,
                "render_items_cloned": work.render_items_cloned,
                "order_nodes_visited": work.order_nodes_visited,
                "sibling_search_steps": work.sibling_search_steps,
                "full_render_model_scans": work.full_render_model_scans,
                "culling_candidates": work.culling_candidates,
                "visible_items": work.visible_items,
                "gpu_encode_attempted": work.gpu_encode_attempted,
                "gpu_encode_omitted": work.gpu_encode_omitted,
                "instance_upload_bytes": work.instance_upload_bytes,
                "visible_slot_upload_bytes": work.visible_slot_upload_bytes,
                "allocation_growth_count": work.allocation_growth_count,
                "document_full_clones": work.document_full_clones,
                "document_nodes_touched": work.document_nodes_touched,
                "ui_full_snapshots": work.ui_full_snapshots,
                "ui_nodes_serialized": work.ui_nodes_serialized,
                "ui_child_ids_serialized": work.ui_child_ids_serialized,
                "ui_projection_payload_bytes": work.ui_projection_payload_bytes,
                "ui_structural_operations": work.ui_structural_operations,
                "ui_delta_nodes": work.ui_delta_nodes,
            },
        });
        serde_json::to_string(&value).unwrap_or_else(|serialization_error| {
            format!(
                "{{\"type\":\"engine_response\",\"protocol_version\":{PROTOCOL_VERSION},\"request_id\":\"serialization_failure\",\"ok\":false,\"error\":{{\"code\":\"serialization_error\",\"message\":{}}}}}",
                serde_json::to_string(&serialization_error.to_string())
                    .unwrap_or_else(|_| "\"unknown\"".to_owned())
            )
        })
    }
}

impl CommandRequest {
    fn structural_nodes_touched(&self) -> Option<u64> {
        match self {
            Self::Group { targets, .. } => Some(targets.len() as u64 + 2),
            Self::Ungroup { .. } => Some(4),
            _ => None,
        }
    }

    fn into_command(self, document: &Document) -> Result<Command, HostFailure> {
        match self {
            Self::MoveNode { node_index, x, y } => {
                require_finite(&[x, y], "node translation")?;
                Ok(Command::SetLocalTransform {
                    target: fixture_node_id(u128::from(node_index)),
                    transform: Affine2::translation(Vec2::new(x, y)),
                })
            }
            Self::ReorderNode { node_index, index } => Ok(Command::Reparent {
                child: fixture_node_id(u128::from(node_index)),
                new_parent: fixture_node_id(0),
                index,
            }),
            Self::SetVisibility {
                node_index,
                visible,
            } => Ok(Command::SetVisible {
                target: fixture_node_id(u128::from(node_index)),
                visible,
            }),
            Self::SetTransform { node_id, matrix } => {
                require_finite(&matrix, "transform matrix")?;
                Ok(Command::SetLocalTransform {
                    target: parse_node_id(&node_id)?,
                    transform: Affine2::from_components(
                        matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5],
                    ),
                })
            }
            Self::SetName { node_id, name } => Ok(Command::SetName {
                target: parse_node_id(&node_id)?,
                name,
            }),
            Self::SetVisible { node_id, visible } => Ok(Command::SetVisible {
                target: parse_node_id(&node_id)?,
                visible,
            }),
            Self::SetLocked { node_id, locked } => Ok(Command::SetLocked {
                target: parse_node_id(&node_id)?,
                locked,
            }),
            Self::SetGeometry {
                node_id,
                shape,
                width,
                height,
            } => {
                require_positive_size(width, height)?;
                Ok(Command::SetGeometry {
                    target: parse_node_id(&node_id)?,
                    geometry: geometry_for(&shape, width, height)?,
                })
            }
            Self::SetOpacity { node_id, opacity } => {
                require_finite(&[opacity], "opacity")?;
                if !(0.0..=1.0).contains(&opacity) {
                    return Err(HostFailure::new(
                        "invalid_command",
                        "opacity must be between 0 and 1",
                    ));
                }
                let target = parse_node_id(&node_id)?;
                let mut appearance = current_appearance(document, target)?;
                appearance.opacity = opacity;
                Ok(Command::SetAppearance { target, appearance })
            }
            Self::SetFill { node_id, color } => {
                let target = parse_node_id(&node_id)?;
                let mut appearance = current_appearance(document, target)?;
                appearance.fill = parse_color(color)?;
                Ok(Command::SetAppearance { target, appearance })
            }
            Self::SetCornerRadii { node_id, radii } => {
                require_non_negative(&radii, "corner radii")?;
                let target = parse_node_id(&node_id)?;
                let mut appearance = current_appearance(document, target)?;
                appearance.corner_radii = visual_authoring_document::CornerRadii {
                    top_left: radii[0],
                    top_right: radii[1],
                    bottom_right: radii[2],
                    bottom_left: radii[3],
                };
                Ok(Command::SetAppearance { target, appearance })
            }
            Self::SetStroke {
                node_id,
                color,
                width,
            } => {
                require_non_negative(&[width], "stroke width")?;
                let target = parse_node_id(&node_id)?;
                let mut appearance = current_appearance(document, target)?;
                appearance.stroke = visual_authoring_document::Stroke {
                    color: parse_color(color)?,
                    width,
                };
                Ok(Command::SetAppearance { target, appearance })
            }
            Self::CreateShape {
                node_id,
                parent_id,
                index,
                shape,
                name,
                x,
                y,
                width,
                height,
            } => {
                require_finite(&[x, y], "shape translation")?;
                require_positive_size(width, height)?;
                let id = parse_node_id(&node_id)?;
                let mut spec = match shape.as_str() {
                    "rectangle" => NodeSpec::rectangle(id, name, Vec2::new(width, height)),
                    "ellipse" => NodeSpec::ellipse(id, name, Vec2::new(width, height)),
                    "frame" => NodeSpec::frame(id, name, Vec2::new(width, height)),
                    _ => {
                        return Err(HostFailure::new(
                            "invalid_command",
                            format!("unknown shape {shape}"),
                        ))
                    }
                };
                spec.local_transform = Affine2::translation(Vec2::new(x, y));
                Ok(Command::CreateNode {
                    spec,
                    parent: parse_node_id(&parent_id)?,
                    index,
                })
            }
            Self::Group {
                group_id,
                name,
                targets,
            } => Ok(Command::Group {
                group: NodeSpec::group(parse_node_id(&group_id)?, name),
                targets: targets
                    .iter()
                    .map(|target| parse_node_id(target))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            Self::Ungroup { node_id } => Ok(Command::Ungroup {
                target: parse_node_id(&node_id)?,
            }),
            Self::DeleteNode { node_id } => Ok(Command::DeleteSubtree {
                target: parse_node_id(&node_id)?,
            }),
            Self::Reparent {
                node_id,
                parent_id,
                index,
                preserve_world,
            } => {
                let child = parse_node_id(&node_id)?;
                let new_parent = parse_node_id(&parent_id)?;
                if preserve_world {
                    Ok(Command::ReparentPreservingWorld {
                        child,
                        new_parent,
                        index,
                    })
                } else {
                    Ok(Command::Reparent {
                        child,
                        new_parent,
                        index,
                    })
                }
            }
        }
    }
}

fn current_appearance(document: &Document, target: NodeId) -> Result<Appearance, HostFailure> {
    document
        .node(target)
        .map(visual_authoring_document::Node::appearance)
        .ok_or_else(|| HostFailure::new("node_not_found", format!("node {target} was not found")))
}

fn parse_color(color: [f64; 4]) -> Result<ColorRgba, HostFailure> {
    require_finite(&color, "color")?;
    if color
        .iter()
        .any(|component| !(0.0..=1.0).contains(component))
    {
        return Err(HostFailure::new(
            "invalid_command",
            "color components must be between 0 and 1",
        ));
    }
    Ok(ColorRgba::new(color[0], color[1], color[2], color[3]))
}

fn require_non_negative(values: &[f64], label: &str) -> Result<(), HostFailure> {
    require_finite(values, label)?;
    if values.iter().all(|value| *value >= 0.0) {
        Ok(())
    } else {
        Err(HostFailure::new(
            "invalid_command",
            format!("{label} must be non-negative"),
        ))
    }
}

fn parse_node_id(value: &str) -> Result<NodeId, HostFailure> {
    serde_json::from_value(json!(value)).map_err(|_| {
        HostFailure::new(
            "invalid_node_id",
            format!("{value} is not a valid stable NodeId"),
        )
    })
}

fn require_finite(values: &[f64], label: &str) -> Result<(), HostFailure> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(HostFailure::new(
            "invalid_command",
            format!("{label} must be finite"),
        ))
    }
}

fn require_positive_size(width: f64, height: f64) -> Result<(), HostFailure> {
    require_finite(&[width, height], "geometry size")?;
    if width > 0.0 && height > 0.0 {
        Ok(())
    } else {
        Err(HostFailure::new(
            "invalid_command",
            "geometry size must be positive",
        ))
    }
}

fn geometry_for(shape: &str, width: f64, height: f64) -> Result<Geometry, HostFailure> {
    let size = Vec2::new(width, height);
    match shape {
        "rectangle" => Ok(Geometry::Rectangle { size }),
        "ellipse" => Ok(Geometry::Ellipse { size }),
        "frame" => Ok(Geometry::Frame { size }),
        _ => Err(HostFailure::new(
            "invalid_command",
            format!("unknown geometry {shape}"),
        )),
    }
}

fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Document => "document",
        NodeKind::Frame => "frame",
        NodeKind::Group => "group",
        NodeKind::Rectangle => "rectangle",
        NodeKind::Ellipse => "ellipse",
    }
}

fn parse_node_ids(values: &[String]) -> Result<Vec<NodeId>, HostFailure> {
    values.iter().map(|value| parse_node_id(value)).collect()
}

fn default_active_root(document: &Document) -> NodeId {
    let root = document.root_id();
    document
        .node(root)
        .and_then(|node| {
            node.children().iter().copied().find(|id| {
                document
                    .node(*id)
                    .is_some_and(|candidate| candidate.kind() == NodeKind::Frame)
            })
        })
        .unwrap_or(root)
}

const fn union_rect(left: Rect, right: Rect) -> Rect {
    Rect {
        min: Vec2::new(
            if left.min.x < right.min.x {
                left.min.x
            } else {
                right.min.x
            },
            if left.min.y < right.min.y {
                left.min.y
            } else {
                right.min.y
            },
        ),
        max: Vec2::new(
            if left.max.x > right.max.x {
                left.max.x
            } else {
                right.max.x
            },
            if left.max.y > right.max.y {
                left.max.y
            } else {
                right.max.y
            },
        ),
    }
}

fn parse_fixture(value: &str) -> Result<FixtureKind, HostFailure> {
    match value.to_ascii_lowercase().as_str() {
        "preview" | "demo" => Ok(FixtureKind::Preview),
        "editor" | "phase-1a" => Ok(FixtureKind::Editor),
        "bench-a" | "a" | "1k" => Ok(FixtureKind::BenchA),
        "bench-b" | "b" | "10k" => Ok(FixtureKind::BenchB),
        "bench-c" | "c" | "100k" => Ok(FixtureKind::BenchC),
        "bench-d" | "d" | "deep" => Ok(FixtureKind::BenchD),
        _ => Err(HostFailure::new(
            "unknown_fixture",
            format!("unknown fixture {value}"),
        )),
    }
}

#[derive(Debug, Default)]
struct EncodedModel {
    bytes: Vec<u8>,
    attempted: u64,
    omitted: u64,
}

fn encode_full_model(model: &visual_authoring_render_model::RenderModel) -> EncodedModel {
    let mut encoded_model = EncodedModel {
        bytes: vec![0; model.allocated_slot_count() * INSTANCE_STRIDE_BYTES],
        ..EncodedModel::default()
    };
    for item in model.items() {
        if !item.renderable {
            continue;
        }
        encoded_model.attempted += 1;
        let Some(encoded) = encode_item(item) else {
            encoded_model.omitted += 1;
            continue;
        };
        let offset = item.slot as usize * INSTANCE_STRIDE_BYTES;
        encoded_model.bytes[offset..offset + INSTANCE_STRIDE_BYTES].copy_from_slice(&encoded);
    }
    encoded_model
}

fn encode_dirty_model(
    model: &visual_authoring_render_model::RenderModel,
    dirty_slots: &[u32],
) -> EncodedModel {
    let mut encoded_model = EncodedModel {
        bytes: Vec::with_capacity(dirty_slots.len() * DIRTY_RECORD_STRIDE_BYTES),
        ..EncodedModel::default()
    };
    for &slot in dirty_slots {
        encoded_model.bytes.extend_from_slice(&slot.to_le_bytes());
        let encoded = model.item_by_slot(slot).and_then(|item| {
            if item.renderable {
                encoded_model.attempted += 1;
            }
            let encoded = encode_item(item);
            if item.renderable && encoded.is_none() {
                encoded_model.omitted += 1;
            }
            encoded
        });
        if let Some(encoded) = encoded {
            encoded_model.bytes.extend_from_slice(&encoded);
        } else {
            encoded_model
                .bytes
                .resize(encoded_model.bytes.len() + INSTANCE_STRIDE_BYTES, 0);
        }
    }
    encoded_model
}

fn encode_item(item: &RenderItem) -> Option<[u8; INSTANCE_STRIDE_BYTES]> {
    if !item.gpu_encodable {
        return None;
    }
    let world = item.world_transform?;
    let values = [
        world.m11,
        world.m12,
        world.m21,
        world.m22,
        item.size.x,
        item.size.y,
        item.fill_linear[0],
        item.fill_linear[1],
        item.fill_linear[2],
        item.fill_linear[3],
        item.opacity,
        item.corner_radii[0],
        item.corner_radii[1],
        item.corner_radii[2],
        item.corner_radii[3],
        item.stroke_linear[0],
        item.stroke_linear[1],
        item.stroke_linear[2],
        item.stroke_linear[3],
        item.stroke_width,
    ];
    if values.iter().any(|value| !finite_f32(*value)) {
        return None;
    }
    let (tx_hi, tx_lo) = split_f64(world.tx)?;
    let (ty_hi, ty_lo) = split_f64(world.ty)?;
    let fields = [
        world.m11 as f32,
        world.m12 as f32,
        world.m21 as f32,
        world.m22 as f32,
        tx_hi,
        ty_hi,
        tx_lo,
        ty_lo,
        item.size.x as f32,
        item.size.y as f32,
        item.opacity as f32,
    ];
    let mut bytes = [0_u8; INSTANCE_STRIDE_BYTES];
    for (index, value) in fields.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    let primitive = u32::from(item.primitive == PrimitiveKind::Ellipse);
    bytes[44..48].copy_from_slice(&primitive.to_le_bytes());
    for (index, value) in item.fill_linear.into_iter().enumerate() {
        bytes[48 + index * 4..52 + index * 4].copy_from_slice(&(value as f32).to_le_bytes());
    }
    for (index, value) in item.corner_radii.into_iter().enumerate() {
        bytes[64 + index * 4..68 + index * 4].copy_from_slice(&(value as f32).to_le_bytes());
    }
    for (index, value) in item.stroke_linear.into_iter().enumerate() {
        bytes[80 + index * 4..84 + index * 4].copy_from_slice(&(value as f32).to_le_bytes());
    }
    bytes[96..100].copy_from_slice(&(item.stroke_width as f32).to_le_bytes());
    Some(bytes)
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

const fn encoding_diagnostic_label(kind: RenderEncodingDiagnosticKind) -> &'static str {
    match kind {
        RenderEncodingDiagnosticKind::LinearOrGeometryOutsideF32 => {
            "linear_or_geometry_outside_f32"
        }
        RenderEncodingDiagnosticKind::TranslationOutsideF32 => "translation_outside_f32",
    }
}
fn document_failure(error: visual_authoring_document::DocumentError) -> HostFailure {
    HostFailure::new("document_error", error.to_string())
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(request_id: &str, body: Value) -> String {
        let mut object = body.as_object().cloned().expect("request is an object");
        object.insert("protocol_version".to_owned(), json!(PROTOCOL_VERSION));
        object.insert("request_id".to_owned(), json!(request_id));
        serde_json::to_string(&object).unwrap()
    }

    fn response(host: &mut EngineHost, request: String) -> Value {
        serde_json::from_str(&host.handle_json(&request)).unwrap()
    }

    #[test]
    fn initialization_returns_correlated_worker_state_and_binary_frame() {
        let mut host = EngineHost::new_inner().unwrap();
        let response = response(
            &mut host,
            request("init-1", json!({ "type": "initialize" })),
        );
        assert_eq!(response["request_id"], "init-1");
        assert_eq!(response["runtime_owner"], "dedicated-worker");
        assert_eq!(
            response["revisions"]["document"],
            response["revisions"]["scene"]
        );
        assert_eq!(
            response["revisions"]["scene"],
            response["revisions"]["render"]
        );
        assert_eq!(response["culling"]["visible"], 2);
        assert!(response["binary"]["full_instances"].as_bool().unwrap());
        assert_eq!(host.pending.full_instances.len(), 2 * INSTANCE_STRIDE_BYTES);
    }

    #[test]
    fn protocol_mismatch_is_a_typed_error_without_mutation() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = host.runtime.document_revision();
        let response: Value = serde_json::from_str(
            &host.handle_json(
                &json!({
                    "protocol_version": 999,
                    "request_id": "bad-version",
                    "type": "heartbeat"
                })
                .to_string(),
            ),
        )
        .unwrap();
        assert!(!response["ok"].as_bool().unwrap());
        assert_eq!(response["error"]["code"], "protocol_version_mismatch");
        assert_eq!(host.runtime.document_revision(), before);
    }

    #[test]
    fn command_updates_one_instance_and_undo_restores_through_runtime_history() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = host.runtime.document_revision();
        let moved = response(
            &mut host,
            request(
                "move-1",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 1, "x": 25.0, "y": 30.0 }
                }),
            ),
        );
        assert!(moved["ok"].as_bool().unwrap());
        assert_eq!(moved["render_delta"]["dirty_slots"], 1);
        assert_eq!(
            host.pending.dirty_instances.len(),
            DIRTY_RECORD_STRIDE_BYTES
        );
        assert_eq!(host.runtime.document_revision(), before + 1);

        let undone = response(&mut host, request("undo-1", json!({ "type": "undo" })));
        assert!(undone["result"]["changed"].as_bool().unwrap());
        assert_eq!(undone["render_delta"]["dirty_slots"], 1);
        assert_eq!(host.runtime.document_revision(), before + 2);
    }

    #[test]
    fn camera_changes_visibility_without_changing_any_revision() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = (
            host.runtime.document_revision(),
            host.runtime.scene().revision(),
            host.runtime.render_model().revision(),
        );
        let response = response(
            &mut host,
            request(
                "pan-1",
                json!({ "type": "camera", "camera": { "kind": "pan", "dx": 5000.0, "dy": 5000.0 } }),
            ),
        );
        assert!(response["ok"].as_bool().unwrap());
        assert_eq!(response["render_delta"]["dirty_slots"], 0);
        assert_eq!(
            (
                host.runtime.document_revision(),
                host.runtime.scene().revision(),
                host.runtime.render_model().revision(),
            ),
            before
        );
    }

    #[test]
    fn failed_transaction_rollback_returns_preview_to_original_state() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = host.runtime.document_revision();
        response(
            &mut host,
            request("tx-begin", json!({ "type": "begin_transaction" })),
        );
        response(
            &mut host,
            request(
                "tx-update",
                json!({
                    "type": "update_transaction",
                    "command": { "kind": "move_node", "node_index": 2, "x": 400.0, "y": 300.0 }
                }),
            ),
        );
        let rolled_back = response(
            &mut host,
            request("tx-rollback", json!({ "type": "rollback_transaction" })),
        );
        assert!(rolled_back["ok"].as_bool().unwrap());
        assert!(!host.runtime.transaction_active());
        assert_eq!(host.runtime.document_revision(), before + 2);
        assert_eq!(rolled_back["render_delta"]["dirty_slots"], 1);
    }

    #[test]
    fn unencodable_f64_command_succeeds_with_typed_omission_and_same_slot_recovery() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = host.runtime.document_revision();
        let slot = host
            .runtime
            .render_model()
            .item(fixture_node_id(1))
            .unwrap()
            .slot;

        let omitted = response(
            &mut host,
            request(
                "f32-omission",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 1, "x": 1.0e100, "y": 0.0 }
                }),
            ),
        );
        assert!(omitted["ok"].as_bool().unwrap());
        assert_eq!(omitted["revisions"]["document"], before + 1);
        assert_eq!(
            omitted["revisions"]["document"],
            omitted["revisions"]["scene"]
        );
        assert_eq!(
            omitted["revisions"]["scene"],
            omitted["revisions"]["render"]
        );
        assert_eq!(omitted["render_encoding"]["gpu_omitted_items"], 1);
        assert_eq!(
            omitted["render_encoding"]["diagnostics"][0]["node_id"],
            fixture_node_id(1).to_string()
        );
        assert_eq!(
            omitted["render_encoding"]["diagnostics"][0]["reason"],
            "translation_outside_f32"
        );
        assert_eq!(omitted["render_delta"]["dirty_slots"], 1);
        assert!(omitted["binary"]["dirty_instances"].as_bool().unwrap());
        assert!(omitted["binary"]["visible_slots"].as_bool().unwrap());
        assert_eq!(
            host.pending.dirty_instances.len(),
            DIRTY_RECORD_STRIDE_BYTES
        );
        assert!(host.pending.dirty_instances[4..]
            .iter()
            .all(|byte| *byte == 0));
        assert_eq!(host.pending.visible_slots.len(), 1);
        assert_eq!(
            host.runtime
                .render_model()
                .item(fixture_node_id(1))
                .unwrap()
                .slot,
            slot
        );

        let recovered = response(
            &mut host,
            request(
                "f32-recovery",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 1, "x": 25.0, "y": 30.0 }
                }),
            ),
        );
        assert!(recovered["ok"].as_bool().unwrap());
        assert_eq!(recovered["render_encoding"]["gpu_omitted_items"], 0);
        assert!(host.pending.dirty_instances[4..]
            .iter()
            .any(|byte| *byte != 0));
        assert_eq!(host.pending.visible_slots.len(), 2);
        assert_eq!(
            host.runtime
                .render_model()
                .item(fixture_node_id(1))
                .unwrap()
                .slot,
            slot
        );
    }

    #[test]
    fn omission_round_trips_through_undo_redo_and_transaction_rollback() {
        let mut host = EngineHost::new_inner().unwrap();
        let omitted = response(
            &mut host,
            request(
                "omit",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 2, "x": 1.0e100, "y": 0.0 }
                }),
            ),
        );
        assert!(omitted["ok"].as_bool().unwrap());
        assert_eq!(omitted["render_encoding"]["gpu_omitted_items"], 1);

        let undo = response(&mut host, request("undo-omit", json!({ "type": "undo" })));
        assert!(undo["ok"].as_bool().unwrap());
        assert_eq!(undo["render_encoding"]["gpu_omitted_items"], 0);
        let redo = response(&mut host, request("redo-omit", json!({ "type": "redo" })));
        assert!(redo["ok"].as_bool().unwrap());
        assert_eq!(redo["render_encoding"]["gpu_omitted_items"], 1);

        response(
            &mut host,
            request("begin-omit", json!({ "type": "begin_transaction" })),
        );
        let preview = response(
            &mut host,
            request(
                "preview-recover",
                json!({
                    "type": "update_transaction",
                    "command": { "kind": "move_node", "node_index": 2, "x": 20.0, "y": 20.0 }
                }),
            ),
        );
        assert!(preview["ok"].as_bool().unwrap());
        assert_eq!(preview["render_encoding"]["gpu_omitted_items"], 0);
        let rollback = response(
            &mut host,
            request("rollback-omit", json!({ "type": "rollback_transaction" })),
        );
        assert!(rollback["ok"].as_bool().unwrap());
        assert_eq!(rollback["render_encoding"]["gpu_omitted_items"], 1);
    }

    #[test]
    fn one_unencodable_item_in_multi_dirty_update_zeroes_only_its_record() {
        let mut host = EngineHost::new_inner().unwrap();
        response(
            &mut host,
            request(
                "make-one-omitted",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 1, "x": 1.0e100, "y": 0.0 }
                }),
            ),
        );
        let root = host.runtime.scene().root_id();
        let hidden = host
            .runtime
            .dispatch(Command::SetVisible {
                target: root,
                visible: false,
            })
            .unwrap();
        let shown = host
            .runtime
            .dispatch(Command::SetVisible {
                target: root,
                visible: true,
            })
            .unwrap();
        assert_eq!(hidden.render.dirty_slots.len(), 2);
        assert_eq!(shown.render.dirty_slots.len(), 2);
        let encoded = encode_dirty_model(host.runtime.render_model(), &shown.render.dirty_slots);
        assert_eq!(encoded.attempted, 2);
        assert_eq!(encoded.omitted, 1);
        assert_eq!(encoded.bytes.len(), 2 * DIRTY_RECORD_STRIDE_BYTES);
        let zero_records = encoded
            .bytes
            .chunks_exact(DIRTY_RECORD_STRIDE_BYTES)
            .filter(|record| record[4..].iter().all(|byte| *byte == 0))
            .count();
        assert_eq!(zero_records, 1);
    }

    #[test]
    fn failed_request_keeps_all_revisions_and_binary_state_unchanged() {
        let mut host = EngineHost::new_inner().unwrap();
        let before = (
            host.runtime.document_revision(),
            host.runtime.scene().revision(),
            host.runtime.render_model().revision(),
            host.runtime.history_state(),
        );
        let failed = response(
            &mut host,
            request(
                "missing-node",
                json!({
                    "type": "command",
                    "command": { "kind": "move_node", "node_index": 999999, "x": 10.0, "y": 20.0 }
                }),
            ),
        );
        assert!(!failed["ok"].as_bool().unwrap());
        assert_eq!(
            (
                host.runtime.document_revision(),
                host.runtime.scene().revision(),
                host.runtime.render_model().revision(),
                host.runtime.history_state(),
            ),
            before
        );
        assert!(!failed["binary"]["full_instances"].as_bool().unwrap());
        assert!(!failed["binary"]["dirty_instances"].as_bool().unwrap());
        assert!(!failed["binary"]["removed_slots"].as_bool().unwrap());
        assert!(!failed["binary"]["visible_slots"].as_bool().unwrap());
        assert!(host.pending.full_instances.is_empty());
        assert!(host.pending.dirty_instances.is_empty());
        assert!(host.pending.removed_slots.is_empty());
        assert!(host.pending.visible_slots.is_empty());
    }

    #[test]
    fn selection_transform_updates_multiple_nodes_as_one_undo_step() {
        let mut host = EngineHost::new_inner().unwrap();
        let first = fixture_node_id(1);
        let second = fixture_node_id(2);
        let before_first = host.runtime.document().node(first).unwrap().local_transform();
        let before_second = host.runtime.document().node(second).unwrap().local_transform();
        response(
            &mut host,
            request(
                "select-two",
                json!({
                    "type": "selection",
                    "mode": "set",
                    "targets": [first.to_string(), second.to_string()]
                }),
            ),
        );
        response(
            &mut host,
            request("transform-begin", json!({ "type": "begin_transaction" })),
        );
        let transformed = response(
            &mut host,
            request(
                "transform-preview",
                json!({
                    "type": "transform_selection",
                    "matrix": [2.0, 0.0, 0.0, 2.0, 10.0, 20.0]
                }),
            ),
        );
        assert!(transformed["ok"].as_bool().unwrap());
        assert_eq!(transformed["result"]["transformed"], 2);
        response(
            &mut host,
            request("transform-commit", json!({ "type": "commit_transaction" })),
        );
        assert_eq!(host.runtime.history_state().undo_depth, 1);
        response(&mut host, request("transform-undo", json!({ "type": "undo" })));
        assert_eq!(
            host.runtime.document().node(first).unwrap().local_transform(),
            before_first
        );
        assert_eq!(
            host.runtime.document().node(second).unwrap().local_transform(),
            before_second
        );
    }

    #[test]
    fn command_batch_failure_rolls_back_every_prior_command() {
        let mut host = EngineHost::new_inner().unwrap();
        let first = fixture_node_id(1);
        let before = host.runtime.document().node(first).unwrap().local_transform();
        let failed = response(
            &mut host,
            request(
                "batch-failure",
                json!({
                    "type": "command_batch",
                    "commands": [
                        { "kind": "set_transform", "node_id": first.to_string(), "matrix": [1.0, 0.0, 0.0, 1.0, 50.0, 60.0] },
                        { "kind": "set_transform", "node_id": fixture_node_id(999_999).to_string(), "matrix": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] }
                    ]
                }),
            ),
        );
        assert!(!failed["ok"].as_bool().unwrap());
        assert_eq!(host.runtime.document().node(first).unwrap().local_transform(), before);
        assert_eq!(host.runtime.history_state().undo_depth, 0);
        assert!(!host.runtime.transaction_active());
    }

    #[test]
    fn active_slide_root_is_not_selectable_and_switching_isolates_its_descendants() {
        let mut host = EngineHost::new_inner().unwrap();
        let loaded = response(
            &mut host,
            request(
                "load-editor",
                json!({ "type": "load_fixture", "fixture": "editor" }),
            ),
        );
        let first_slide = fixture_node_id(1);
        assert_eq!(loaded["active_root"], first_slide.to_string());
        let root_rejected = response(
            &mut host,
            request(
                "select-slide-root",
                json!({
                    "type": "selection",
                    "mode": "replace",
                    "target": first_slide.to_string()
                }),
            ),
        );
        assert!(!root_rejected["ok"].as_bool().unwrap());
        assert_eq!(
            root_rejected["error"]["code"],
            "selection_outside_active_root"
        );

        let rectangle = fixture_node_id(100);
        let created = response(
            &mut host,
            request(
                "create-in-slide",
                json!({
                    "type": "command",
                    "command": {
                        "kind": "create_shape",
                        "node_id": rectangle.to_string(),
                        "parent_id": first_slide.to_string(),
                        "index": 0,
                        "shape": "rectangle",
                        "name": "Inside first slide",
                        "x": 10.0,
                        "y": 20.0,
                        "width": 40.0,
                        "height": 30.0
                    }
                }),
            ),
        );
        assert!(created["ok"].as_bool().unwrap());
        let selected = response(
            &mut host,
            request(
                "select-in-slide",
                json!({
                    "type": "selection",
                    "mode": "replace",
                    "target": rectangle.to_string()
                }),
            ),
        );
        assert!(selected["ok"].as_bool().unwrap());

        let second_slide = fixture_node_id(101);
        response(
            &mut host,
            request(
                "create-second-slide",
                json!({
                    "type": "command",
                    "command": {
                        "kind": "create_shape",
                        "node_id": second_slide.to_string(),
                        "parent_id": fixture_node_id(0).to_string(),
                        "index": 1,
                        "shape": "frame",
                        "name": "Second slide",
                        "x": 2000.0,
                        "y": 0.0,
                        "width": 1920.0,
                        "height": 1080.0
                    }
                }),
            ),
        );
        let switched = response(
            &mut host,
            request(
                "switch-slide",
                json!({
                    "type": "set_active_root",
                    "node_id": second_slide.to_string()
                }),
            ),
        );
        assert!(switched["ok"].as_bool().unwrap());
        assert_eq!(switched["active_root"], second_slide.to_string());
        assert!(host.runtime.selection().is_empty());
        let old_slide_rejected = response(
            &mut host,
            request(
                "select-old-slide-child",
                json!({
                    "type": "selection",
                    "mode": "replace",
                    "target": rectangle.to_string()
                }),
            ),
        );
        assert!(!old_slide_rejected["ok"].as_bool().unwrap());
    }

    #[test]
    fn heartbeat_has_monotonic_sequence_and_zero_render_work() {
        let mut host = EngineHost::new_inner().unwrap();
        let first = response(
            &mut host,
            request("heartbeat-one", json!({ "type": "heartbeat" })),
        );
        let second = response(
            &mut host,
            request("heartbeat-two", json!({ "type": "heartbeat" })),
        );
        assert!(
            second["engine_sequence"].as_u64().unwrap()
                > first["engine_sequence"].as_u64().unwrap()
        );
        assert_eq!(
            second["render_binary_schema_version"],
            RENDER_BINARY_SCHEMA_VERSION
        );
        for field in [
            "document_nodes_scanned",
            "scene_nodes_visited",
            "render_items_planned",
            "render_items_read",
            "render_items_written",
            "render_items_cloned",
            "order_nodes_visited",
            "sibling_search_steps",
            "full_render_model_scans",
            "culling_candidates",
            "visible_items",
            "gpu_encode_attempted",
            "gpu_encode_omitted",
            "instance_upload_bytes",
            "visible_slot_upload_bytes",
            "allocation_growth_count",
        ] {
            assert_eq!(second["metrics"][field], 0, "{field}");
        }
        assert!(!second["binary"]["full_instances"].as_bool().unwrap());
        assert!(!second["binary"]["dirty_instances"].as_bool().unwrap());
        assert!(!second["binary"]["visible_slots"].as_bool().unwrap());
    }

    #[derive(Default)]
    struct R2HostMeasurements {
        times_ms: Vec<f64>,
        work: Vec<u64>,
    }

    impl R2HostMeasurements {
        fn push(&mut self, elapsed_ms: f64, value: &Value) {
            self.times_ms.push(elapsed_ms);
            self.work.push(r2_host_work(value));
        }

        fn median(&self) -> f64 {
            r2_percentile(&self.times_ms[10..], 0.50)
        }

        fn max_work(&self) -> u64 {
            self.work[10..].iter().copied().max().unwrap_or(0)
        }

        fn json(&self) -> Value {
            json!({
                "samples_ms": &self.times_ms[10..],
                "median_ms": r2_percentile(&self.times_ms[10..], 0.50),
                "p95_ms": r2_percentile(&self.times_ms[10..], 0.95),
                "work_samples": &self.work[10..],
                "max_work": self.max_work(),
            })
        }
    }

    struct R2HostFixture {
        count: usize,
        targets: Vec<String>,
        group: R2HostMeasurements,
        undo: R2HostMeasurements,
        redo: R2HostMeasurements,
        ungroup: R2HostMeasurements,
        edited_ungroup: R2HostMeasurements,
        persistence_bytes: u64,
    }

    fn r2_host_call(host: &mut EngineHost, id: &str, body: Value) -> Value {
        response(host, request(id, body))
    }

    fn r2_assert_host_bounded(value: &Value, gpu_zero: bool) {
        assert_eq!(value["ok"], true, "{value}");
        for prefix in ["document", "scene"] {
            for suffix in ["full_scans", "full_copies", "dense_index_rewrites"] {
                let key = format!("{prefix}_sequence_{suffix}");
                assert_eq!(value["metrics"][&key], 0, "{key}");
            }
        }
        assert_eq!(value["metrics"]["fallback_rebuild_count"], 0);
        assert_eq!(value["metrics"]["full_render_model_scans"], 0);
        assert_eq!(value["metrics"]["render_items_cloned"], 0);
        assert_eq!(value["render_delta"]["scene_full_rebuilds"], 0);
        assert_eq!(value["render_delta"]["render_full_rebuilds"], 0);
        assert_eq!(value["metrics"]["ui_full_snapshots"], 0);
        if gpu_zero {
            assert_eq!(value["render_delta"]["dirty_slots"], 0);
            assert_eq!(value["metrics"]["instance_upload_bytes"], 0);
        }
    }

    fn r2_host_work(value: &Value) -> u64 {
        ["document", "scene"]
            .iter()
            .map(|prefix| {
                let metric = |suffix: &str| {
                    value["metrics"][format!("{prefix}_sequence_{suffix}")]
                        .as_u64()
                        .unwrap_or(0)
                };
                metric("entries_examined").max(metric("rank_order_comparisons"))
                    + metric("entries_copied")
                    + metric("entries_moved_or_shifted")
                    + metric("nodes_allocated")
                    + metric("allocated_bytes").div_ceil(8)
                    + metric("tree_rebalances")
                    + metric("fallback_or_rebuild_count")
                    + metric("full_scans")
                    + metric("full_copies")
                    + metric("dense_index_rewrites")
            })
            .sum()
    }

    fn r2_run_host_fixture(fixture: &str, count: usize) -> R2HostFixture {
        let mut host = EngineHost::new_inner().unwrap();
        let loaded = r2_host_call(
            &mut host,
            "r2-load-fixture",
            json!({ "type": "load_fixture", "fixture": fixture }),
        );
        assert_eq!(loaded["ok"], true);
        let targets = vec![
            fixture_node_id(1).to_string(),
            fixture_node_id((count / 2 + 1) as u128).to_string(),
            fixture_node_id(count as u128).to_string(),
        ];
        let group_id = fixture_node_id(count as u128 + 1_000_000).to_string();
        let mut group = R2HostMeasurements::default();
        let mut undo = R2HostMeasurements::default();
        let mut redo = R2HostMeasurements::default();
        let mut ungroup = R2HostMeasurements::default();
        for iteration in 0..40 {
            let started = std::time::Instant::now();
            let grouped = r2_host_call(
                &mut host,
                &format!("r2-group-{iteration}"),
                json!({
                    "type": "command",
                    "command": { "kind": "group", "group_id": group_id, "name": "R2 host group", "targets": targets }
                }),
            );
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            r2_assert_host_bounded(&grouped, true);
            group.push(elapsed, &grouped);

            let started = std::time::Instant::now();
            let undone = r2_host_call(
                &mut host,
                &format!("r2-undo-{iteration}"),
                json!({ "type": "undo" }),
            );
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            r2_assert_host_bounded(&undone, true);
            undo.push(elapsed, &undone);

            let started = std::time::Instant::now();
            let redone = r2_host_call(
                &mut host,
                &format!("r2-redo-{iteration}"),
                json!({ "type": "redo" }),
            );
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            r2_assert_host_bounded(&redone, true);
            redo.push(elapsed, &redone);

            let started = std::time::Instant::now();
            let ungrouped = r2_host_call(
                &mut host,
                &format!("r2-ungroup-{iteration}"),
                json!({
                    "type": "command", "command": { "kind": "ungroup", "node_id": group_id }
                }),
            );
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            r2_assert_host_bounded(&ungrouped, true);
            ungroup.push(elapsed, &ungrouped);
        }

        let workflow_group_id = fixture_node_id(count as u128 + 8_000_000).to_string();
        let inserted_id = fixture_node_id(count as u128 + 8_000_001).to_string();
        let deleted_id = fixture_node_id(2).to_string();
        let reordered_id = fixture_node_id((count - 1) as u128).to_string();
        let root_id = fixture_node_id(0).to_string();
        let mut edited_ungroup = R2HostMeasurements::default();
        for iteration in 0..40 {
            let steps = [
                json!({ "type": "command", "command": { "kind": "group", "group_id": workflow_group_id, "name": "R2 edited group", "targets": targets } }),
                json!({ "type": "command", "command": { "kind": "create_shape", "node_id": inserted_id, "parent_id": root_id, "index": 0, "shape": "rectangle", "name": "Inserted", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 } }),
                json!({ "type": "command", "command": { "kind": "delete_node", "node_id": deleted_id } }),
                json!({ "type": "command", "command": { "kind": "reparent", "node_id": reordered_id, "parent_id": root_id, "index": 1, "preserve_world": false } }),
                json!({ "type": "command", "command": { "kind": "ungroup", "node_id": workflow_group_id } }),
            ];
            let mut last = Value::Null;
            for (step, body) in steps.into_iter().enumerate() {
                let started = std::time::Instant::now();
                let value =
                    r2_host_call(&mut host, &format!("r2-workflow-{iteration}-{step}"), body);
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                r2_assert_host_bounded(&value, matches!(step, 0 | 4));
                if step == 4 {
                    edited_ungroup.times_ms.push(elapsed);
                    edited_ungroup.work.push(r2_host_work(&value));
                }
                last = value;
            }
            assert_eq!(last["ok"], true);
            for reset in 0..5 {
                let value = r2_host_call(
                    &mut host,
                    &format!("r2-workflow-reset-{iteration}-{reset}"),
                    json!({ "type": "undo" }),
                );
                assert_eq!(value["ok"], true);
            }
        }

        let persistence_group_id = fixture_node_id(count as u128 + 4_000_000).to_string();
        let grouped = r2_host_call(
            &mut host,
            "r2-persistence-group",
            json!({
                "type": "command", "command": { "kind": "group", "group_id": persistence_group_id, "name": "Persisted", "targets": targets }
            }),
        );
        r2_assert_host_bounded(&grouped, true);
        let saved = r2_host_call(&mut host, "r2-save", json!({ "type": "save_document" }));
        let document_json = saved["result"]["document_json"]
            .as_str()
            .unwrap()
            .to_owned();
        let persistence_bytes = saved["result"]["document_bytes"].as_u64().unwrap();
        let loaded = r2_host_call(
            &mut host,
            "r2-load",
            json!({ "type": "load_document", "document_json": document_json }),
        );
        assert_eq!(loaded["ok"], true);
        let ungrouped = r2_host_call(
            &mut host,
            "r2-loaded-ungroup",
            json!({
                "type": "command", "command": { "kind": "ungroup", "node_id": persistence_group_id }
            }),
        );
        r2_assert_host_bounded(&ungrouped, true);

        R2HostFixture {
            count,
            targets,
            group,
            undo,
            redo,
            ungroup,
            edited_ungroup,
            persistence_bytes,
        }
    }

    fn r2_percentile(values: &[f64], fraction: f64) -> f64 {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize]
    }

    fn r2_host_ratio(small: &R2HostMeasurements, large: &R2HostMeasurements) -> Value {
        json!({
            "work_count": large.max_work() as f64 / small.max_work().max(1) as f64,
            "median_time": large.median() / small.median().max(f64::EPSILON),
        })
    }

    #[test]
    fn phase0e_r2_direct_engine_host_is_bounded_at_10k_and_100k() {
        let ten_k = r2_run_host_fixture("bench-b", 10_000);
        let hundred_k = r2_run_host_fixture("bench-c", 100_000);
        for (name, small, large) in [
            ("group", &ten_k.group, &hundred_k.group),
            ("undo", &ten_k.undo, &hundred_k.undo),
            ("redo", &ten_k.redo, &hundred_k.redo),
            ("ungroup", &ten_k.ungroup, &hundred_k.ungroup),
            (
                "edited_ungroup",
                &ten_k.edited_ungroup,
                &hundred_k.edited_ungroup,
            ),
        ] {
            let work_ratio = large.max_work() as f64 / small.max_work().max(1) as f64;
            let time_ratio = large.median() / small.median().max(f64::EPSILON);
            assert!(work_ratio <= 1.35, "{name} work ratio {work_ratio}");
            assert!(time_ratio <= 3.0, "{name} time ratio {time_ratio}");
        }
        let fixture_json = |fixture: &R2HostFixture| {
            json!({
                "node_count": fixture.count,
                "target_ids": fixture.targets,
                "warmups": 10,
                "measured_iterations": 30,
                "persistence_bytes": fixture.persistence_bytes,
                "operations": {
                    "group": fixture.group.json(),
                    "undo": fixture.undo.json(),
                    "redo": fixture.redo.json(),
                    "ungroup": fixture.ungroup.json(),
                    "sibling_insert_delete_reorder_then_ungroup": fixture.edited_ungroup.json(),
                }
            })
        };
        let report = json!({
            "phase": "0E-R2",
            "layer": "direct-engine-host",
            "fixtures": [fixture_json(&ten_k), fixture_json(&hundred_k)],
            "ratios_100k_over_10k": {
                "group": r2_host_ratio(&ten_k.group, &hundred_k.group),
                "undo": r2_host_ratio(&ten_k.undo, &hundred_k.undo),
                "redo": r2_host_ratio(&ten_k.redo, &hundred_k.redo),
                "ungroup": r2_host_ratio(&ten_k.ungroup, &hundred_k.ungroup),
                "sibling_insert_delete_reorder_then_ungroup": r2_host_ratio(&ten_k.edited_ungroup, &hundred_k.edited_ungroup),
            },
            "all_passed": true,
        });
        println!("PHASE0E_R2_WASM_JSON={report}");
    }

    #[test]
    fn phase0e_r3_failed_public_requests_preserve_all_runtime_planes_and_diagnostics() {
        let mut host = EngineHost::new_inner().unwrap();
        let root_id = fixture_node_id(0).to_string();
        let first = fixture_node_id(1).to_string();
        let second = fixture_node_id(2).to_string();
        let group_id = fixture_node_id(9_000_000).to_string();
        let grouped = r2_host_call(
            &mut host,
            "r3-seed-group",
            json!({
                "type": "command",
                "command": {
                    "kind": "group",
                    "group_id": group_id,
                    "name": "Atomic seed",
                    "targets": [first, second]
                }
            }),
        );
        assert_eq!(grouped["ok"], true);
        let selected = r2_host_call(
            &mut host,
            "r3-seed-selection",
            json!({ "type": "selection", "target": group_id, "mode": "replace" }),
        );
        assert_eq!(selected["ok"], true);
        let saved = r2_host_call(
            &mut host,
            "r3-seed-save",
            json!({ "type": "save_document" }),
        );
        let stored_document = saved["result"]["document_json"]
            .as_str()
            .expect("saved document")
            .to_owned();

        let document_before = host.runtime.document().snapshot();
        let sequence_before = host.runtime.document().sequence_work();
        let scene_before = host.runtime.scene().semantic_snapshot();
        let render_revision_before = host.runtime.render_model().revision();
        let render_counters_before = host.runtime.render_model().counters();
        let revision_before = host.runtime.document_revision();
        let history_before = host.runtime.history_state();
        let selection_before = host.runtime.selection().clone();
        let fallback_before = host.fallback_rebuild_count;

        let assert_unchanged = |host: &EngineHost| {
            assert_eq!(host.runtime.document().snapshot(), document_before);
            assert_eq!(host.runtime.document().sequence_work(), sequence_before);
            assert_eq!(host.runtime.scene().semantic_snapshot(), scene_before);
            assert_eq!(
                host.runtime.render_model().revision(),
                render_revision_before
            );
            assert_eq!(
                host.runtime.render_model().counters(),
                render_counters_before
            );
            assert_eq!(host.runtime.document_revision(), revision_before);
            assert_eq!(host.runtime.history_state(), history_before);
            assert_eq!(host.runtime.selection(), &selection_before);
            assert_eq!(host.fallback_rebuild_count, fallback_before);
            assert!(host.pending.full_instances.is_empty());
            assert!(host.pending.dirty_instances.is_empty());
            assert!(host.pending.removed_slots.is_empty());
            assert!(host.pending.visible_slots.is_empty());
        };

        let invalid_spec = r2_host_call(
            &mut host,
            "r3-invalid-public-spec",
            json!({
                "type": "command",
                "command": {
                    "kind": "create_shape",
                    "node_id": fixture_node_id(9_000_001).to_string(),
                    "parent_id": root_id,
                    "index": 0,
                    "shape": "unsupported-shape",
                    "name": "Invalid",
                    "x": 0.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0
                }
            }),
        );
        assert_eq!(invalid_spec["ok"], false);
        assert_unchanged(&host);

        let duplicate = r2_host_call(
            &mut host,
            "r3-duplicate-id",
            json!({
                "type": "command",
                "command": {
                    "kind": "group",
                    "group_id": group_id,
                    "name": "Duplicate",
                    "targets": [first, second]
                }
            }),
        );
        assert_eq!(duplicate["ok"], false);
        assert_unchanged(&host);

        let mutate_restoration = |version: Option<u32>, bad_anchor: bool| {
            let mut stored: Value = serde_json::from_str(&stored_document).unwrap();
            let group = stored["document"]["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|node| node["id"] == group_id)
                .unwrap();
            let restoration = group["internal_group_restoration"].as_object_mut().unwrap();
            if let Some(version) = version {
                restoration.insert("version".to_owned(), json!(version));
            }
            if bad_anchor {
                restoration["runs"].as_array_mut().unwrap()[0]["before_anchor"] = json!(first);
            }
            serde_json::to_string(&stored).unwrap()
        };

        let unsupported = r2_host_call(
            &mut host,
            "r3-restoration-version",
            json!({ "type": "load_document", "document_json": mutate_restoration(Some(999), false) }),
        );
        assert_eq!(unsupported["ok"], false);
        assert!(unsupported["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unsupported group restoration version"));
        assert_unchanged(&host);

        let bad_anchor = r2_host_call(
            &mut host,
            "r3-restoration-anchor",
            json!({ "type": "load_document", "document_json": mutate_restoration(None, true) }),
        );
        assert_eq!(bad_anchor["ok"], false);
        assert!(bad_anchor["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid group restoration"));
        assert_unchanged(&host);

        println!(
            "PHASE0E_R3_HOST_ATOMICITY_JSON={}",
            json!({
                "invalid_public_spec": invalid_spec["error"]["code"],
                "duplicate_id": duplicate["error"]["code"],
                "unsupported_restoration_version": unsupported["error"]["code"],
                "invalid_anchor": bad_anchor["error"]["code"],
                "document_scene_render_history_selection_unchanged": true,
                "sequence_diagnostics_unchanged": true,
                "gpu_payloads_unchanged": true,
                "all_passed": true
            })
        );
    }

    #[derive(Default)]
    struct R3HostSamples {
        times: Vec<f64>,
        work: Vec<u64>,
        depth: Vec<u64>,
        rebalances: Vec<u64>,
    }

    impl R3HostSamples {
        fn push(&mut self, elapsed: f64, response: &Value) {
            self.times.push(elapsed);
            self.work.push(r2_host_work(response));
            self.depth.push(
                response["metrics"]["document_sequence_maximum_depth"]
                    .as_u64()
                    .unwrap_or(0)
                    .max(
                        response["metrics"]["scene_sequence_maximum_depth"]
                            .as_u64()
                            .unwrap_or(0),
                    ),
            );
            self.rebalances.push(
                response["metrics"]["document_sequence_tree_rebalances"]
                    .as_u64()
                    .unwrap_or(0)
                    + response["metrics"]["scene_sequence_tree_rebalances"]
                        .as_u64()
                        .unwrap_or(0),
            );
        }

        fn json(&self) -> Value {
            json!({
                "raw_samples_ms": self.times,
                "raw_work_samples": self.work,
                "median_ms": r2_percentile(&self.times, 0.5),
                "p95_ms": r2_percentile(&self.times, 0.95),
                "maximum_work": self.work.iter().copied().max().unwrap_or(0),
                "maximum_sequence_depth": self.depth.iter().copied().max().unwrap_or(0),
                "tree_rebalances": self.rebalances.iter().copied().max().unwrap_or(0),
            })
        }

        fn max_work(&self) -> u64 {
            self.work.iter().copied().max().unwrap_or(0)
        }
    }

    struct R3HostEntry {
        count: usize,
        k: usize,
        pattern: &'static str,
        group: R3HostSamples,
        undo: R3HostSamples,
        redo: R3HostSamples,
        ungroup: R3HostSamples,
    }

    fn r3_target_indexes(count: usize, k: usize, pattern: &str) -> Vec<usize> {
        match pattern {
            "contiguous_front" => (1..=k).collect(),
            "contiguous_middle" => {
                let start = (count - k) / 2 + 1;
                (start..start + k).collect()
            }
            "uniform" => (0..k)
                .map(|index| 1 + index * (count - 1) / (k - 1))
                .collect(),
            "random_seed_0x5eed" => {
                let mut state = 0x5eed_u32 ^ count as u32 ^ k as u32;
                let mut selected = std::collections::BTreeSet::new();
                while selected.len() < k {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    selected.insert(1 + state as usize % count);
                }
                selected.into_iter().collect()
            }
            "multiple_runs" => {
                if k == count {
                    return (1..=count).collect();
                }
                let run_count = k.min(7).min(count - k + 1);
                let base_length = k / run_count;
                let mut remainder = k % run_count;
                let gap = (count - k) / (run_count + 1);
                let mut cursor = 1 + gap;
                let mut result = Vec::with_capacity(k);
                for _ in 0..run_count {
                    let length = base_length
                        + usize::from({
                            let extra = remainder > 0;
                            remainder = remainder.saturating_sub(1);
                            extra
                        });
                    result.extend(cursor..cursor + length);
                    cursor += length + gap;
                }
                result
            }
            _ => unreachable!(),
        }
    }

    fn r3_host_entry<'a>(
        entries: &'a [R3HostEntry],
        count: usize,
        k: usize,
        pattern: &str,
    ) -> &'a R3HostEntry {
        entries
            .iter()
            .find(|entry| entry.count == count && entry.k == k && entry.pattern == pattern)
            .expect("host matrix entry")
    }

    #[test]
    fn phase0e_r3_native_engine_host_complexity_matrix() {
        if cfg!(debug_assertions) {
            println!(
                "Phase 0E-R3 EngineHost matrix is exercised by the required release test command"
            );
            return;
        }
        const KS: [usize; 7] = [3, 30, 100, 300, 1_000, 3_000, 10_000];
        const PATTERNS: [&str; 5] = [
            "contiguous_front",
            "contiguous_middle",
            "uniform",
            "random_seed_0x5eed",
            "multiple_runs",
        ];
        let started = format!("{:?}", std::time::SystemTime::now());
        let mut entries = Vec::new();
        for (fixture, count) in [("bench-b", 10_000_usize), ("bench-c", 100_000_usize)] {
            let mut host = EngineHost::new_inner().unwrap();
            let loaded = r2_host_call(
                &mut host,
                "r3-host-load",
                json!({ "type": "load_fixture", "fixture": fixture }),
            );
            assert_eq!(loaded["ok"], true);
            for k in KS {
                for (pattern_index, pattern) in PATTERNS.into_iter().enumerate() {
                    let loaded = r2_host_call(
                        &mut host,
                        "r3-host-reset-baseline",
                        json!({ "type": "load_fixture", "fixture": fixture }),
                    );
                    assert_eq!(loaded["ok"], true);
                    let targets = r3_target_indexes(count, k, pattern)
                        .into_iter()
                        .map(|index| fixture_node_id(index as u128).to_string())
                        .collect::<Vec<_>>();
                    assert_eq!(targets.len(), k);
                    let group_id = fixture_node_id(
                        count as u128 * 1_000 + k as u128 * 10 + pattern_index as u128 + 1,
                    )
                    .to_string();
                    let mut entry = R3HostEntry {
                        count,
                        k,
                        pattern,
                        group: R3HostSamples::default(),
                        undo: R3HostSamples::default(),
                        redo: R3HostSamples::default(),
                        ungroup: R3HostSamples::default(),
                    };
                    for iteration in 0..40 {
                        let started = std::time::Instant::now();
                        let grouped = r2_host_call(
                            &mut host,
                            "r3-host-group",
                            json!({
                                "type": "command",
                                "command": { "kind": "group", "group_id": group_id, "name": "R3 host matrix", "targets": targets }
                            }),
                        );
                        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                        r2_assert_host_bounded(&grouped, true);
                        if iteration >= 10 {
                            entry.group.push(elapsed, &grouped);
                        }

                        let started = std::time::Instant::now();
                        let undone =
                            r2_host_call(&mut host, "r3-host-undo", json!({ "type": "undo" }));
                        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                        r2_assert_host_bounded(&undone, true);
                        if iteration >= 10 {
                            entry.undo.push(elapsed, &undone);
                        }

                        let started = std::time::Instant::now();
                        let redone =
                            r2_host_call(&mut host, "r3-host-redo", json!({ "type": "redo" }));
                        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                        r2_assert_host_bounded(&redone, true);
                        if iteration >= 10 {
                            entry.redo.push(elapsed, &redone);
                        }

                        let started = std::time::Instant::now();
                        let ungrouped = r2_host_call(
                            &mut host,
                            "r3-host-ungroup",
                            json!({ "type": "command", "command": { "kind": "ungroup", "node_id": group_id } }),
                        );
                        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                        r2_assert_host_bounded(&ungrouped, true);
                        if iteration >= 10 {
                            entry.ungroup.push(elapsed, &ungrouped);
                        }
                    }
                    let saved = r2_host_call(
                        &mut host,
                        "r3-host-save",
                        json!({ "type": "save_document" }),
                    );
                    let document: Value =
                        serde_json::from_str(saved["result"]["document_json"].as_str().unwrap())
                            .unwrap();
                    let root = document["document"]["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|node| node["id"] == fixture_node_id(0).to_string())
                        .unwrap();
                    let children = root["children"].as_array().unwrap();
                    assert_eq!(children.len(), count);
                    assert!(children.iter().enumerate().all(|(index, child)| child
                        == &json!(fixture_node_id((index + 1) as u128).to_string())));
                    entries.push(entry);
                }
            }
        }

        let mut thresholds = Vec::new();
        for count in [10_000, 100_000] {
            for pattern in PATTERNS {
                let work_30 =
                    r3_host_entry(&entries, count, 30, pattern).group.max_work() as f64 / 30.0;
                let work_3000 = r3_host_entry(&entries, count, 3_000, pattern)
                    .group
                    .max_work() as f64
                    / 3_000.0;
                let per_k = work_3000 / work_30;
                assert!(per_k <= 2.0, "{count}/{pattern}: work/k {per_k}");
                thresholds.push(json!({ "name": format!("work_per_k_{count}_{pattern}"), "value": per_k, "limit": 2.0, "passed": true }));
                let x10 = r3_host_entry(&entries, count, 3_000, pattern)
                    .group
                    .max_work() as f64
                    / r3_host_entry(&entries, count, 300, pattern)
                        .group
                        .max_work()
                        .max(1) as f64;
                assert!(x10 <= 15.0, "{count}/{pattern}: x10 {x10}");
                thresholds.push(json!({ "name": format!("x10_work_{count}_{pattern}"), "value": x10, "limit": 15.0, "passed": true }));
            }
        }
        for k in KS {
            for pattern in PATTERNS {
                let ratio = r3_host_entry(&entries, 100_000, k, pattern)
                    .group
                    .max_work() as f64
                    / r3_host_entry(&entries, 10_000, k, pattern)
                        .group
                        .max_work()
                        .max(1) as f64;
                assert!(ratio <= 1.35, "{k}/{pattern}: N ratio {ratio}");
                thresholds.push(json!({ "name": format!("n_ratio_{k}_{pattern}"), "value": ratio, "limit": 1.35, "passed": true }));
            }
        }
        let matrix = entries
            .iter()
            .map(|entry| {
                json!({
                    "node_count": entry.count,
                    "selection_count": entry.k,
                    "pattern": entry.pattern,
                    "warmups": 10,
                    "measured_iterations": 30,
                    "operations": {
                        "group": entry.group.json(),
                        "ungroup": entry.ungroup.json(),
                        "undo": entry.undo.json(),
                        "redo": entry.redo.json(),
                    },
                    "exact_sibling_order_restored": true,
                })
            })
            .collect::<Vec<_>>();
        let report = json!({
            "phase": "0E-R3",
            "layer": "native-engine-host",
            "command": "cargo test --release -p visual_authoring_wasm_bridge phase0e_r3_native_engine_host_complexity_matrix -- --nocapture",
            "started_at_utc": started,
            "finished_at_utc": format!("{:?}", std::time::SystemTime::now()),
            "fixture_creation_and_load_excluded": true,
            "matrix": matrix,
            "thresholds": thresholds,
            "all_passed": true,
        });
        println!("PHASE0E_R3_ENGINE_HOST_MATRIX_JSON={report}");
    }

    #[test]
    fn phase1a_frame_appearance_is_typed_incremental_and_undoable() {
        let mut host = EngineHost::new_inner().unwrap();
        let frame_id = fixture_node_id(1);
        let loaded = response(
            &mut host,
            request(
                "load-editor",
                json!({ "type": "load_fixture", "fixture": "editor" }),
            ),
        );
        assert!(loaded["ok"].as_bool().unwrap());
        assert_eq!(loaded["fixture"], "EDITOR");
        assert_eq!(loaded["projection"]["upserts"][1]["kind"], "frame");
        assert_eq!(
            loaded["projection"]["upserts"][1]["name"],
            "Frame 1920×1080"
        );

        let before_revision = host.runtime.document_revision();
        let fill = response(
            &mut host,
            request(
                "set-fill",
                json!({
                    "type": "command",
                    "command": {
                        "kind": "set_fill",
                        "node_id": frame_id.to_string(),
                        "color": [1.0, 0.0, 0.0, 0.75]
                    }
                }),
            ),
        );
        assert!(fill["ok"].as_bool().unwrap());
        assert_eq!(fill["render_delta"]["dirty_slots"], 1);
        assert_eq!(
            fill["projection"]["upserts"][0]["appearance"]["fill"],
            json!([1.0, 0.0, 0.0, 0.75])
        );
        assert_eq!(
            host.pending.dirty_instances.len(),
            DIRTY_RECORD_STRIDE_BYTES
        );
        let linear_red =
            f32::from_le_bytes(host.pending.dirty_instances[52..56].try_into().unwrap());
        assert_eq!(linear_red, 1.0);
        assert_eq!(host.runtime.document_revision(), before_revision + 1);

        let stroke = response(
            &mut host,
            request(
                "set-stroke",
                json!({
                    "type": "command",
                    "command": {
                        "kind": "set_stroke",
                        "node_id": frame_id.to_string(),
                        "color": [0.0, 0.0, 0.0, 1.0],
                        "width": 8.0
                    }
                }),
            ),
        );
        assert!(stroke["ok"].as_bool().unwrap());
        assert_eq!(stroke["render_delta"]["dirty_slots"], 1);
        assert_eq!(
            stroke["projection"]["upserts"][0]["appearance"]["stroke"]["width"],
            8.0
        );

        let before_invalid = host.runtime.document().snapshot();
        let invalid = response(
            &mut host,
            request(
                "invalid-stroke",
                json!({
                    "type": "command",
                    "command": {
                        "kind": "set_stroke",
                        "node_id": frame_id.to_string(),
                        "color": [0.0, 0.0, 0.0, 1.0],
                        "width": -1.0
                    }
                }),
            ),
        );
        assert!(!invalid["ok"].as_bool().unwrap());
        assert_eq!(invalid["error"]["code"], "invalid_command");
        assert_eq!(host.runtime.document().snapshot(), before_invalid);

        let undo = response(&mut host, request("undo-stroke", json!({ "type": "undo" })));
        assert!(undo["ok"].as_bool().unwrap());
        assert_eq!(
            undo["projection"]["upserts"][0]["appearance"]["stroke"]["width"],
            1.0
        );
    }
}
