use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use uuid::Uuid;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_render_model::{DirtySlotRange, RenderDelta, RenderModel};
use visual_authoring_renderer_wgpu::WgpuRenderer;
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime};

fn main() -> Result<(), Box<dyn Error>> {
    let metrics_path = metrics_path()?;
    let mut renderer = pollster::block_on(WgpuRenderer::new_headless())?;
    let capabilities = renderer.capabilities().clone();

    let ten_k_started = Instant::now();
    let mut ten_k = EngineRuntime::new(build_fixture(FixtureKind::BenchB)?, Camera::default())?;
    let ten_k_load_ms = ten_k_started.elapsed().as_secs_f64() * 1_000.0;
    let ten_k_initial =
        renderer.sync_model(ten_k.render_model(), &full_delta(ten_k.render_model()))?;
    let ten_k_edit_started = Instant::now();
    let ten_k_edit = ten_k.dispatch(Command::SetLocalTransform {
        target: fixture_node_id(1),
        transform: Affine2::translation(Vec2::new(1.0, 1.0)),
    })?;
    let ten_k_edit_ms = ten_k_edit_started.elapsed().as_secs_f64() * 1_000.0;
    let ten_k_upload = renderer.sync_model(ten_k.render_model(), &ten_k_edit.render)?;

    let hundred_k_started = Instant::now();
    let mut hundred_k = EngineRuntime::new(build_fixture(FixtureKind::BenchC)?, Camera::default())?;
    let hundred_k_load_ms = hundred_k_started.elapsed().as_secs_f64() * 1_000.0;
    let hundred_k_initial = renderer.sync_model(
        hundred_k.render_model(),
        &full_delta(hundred_k.render_model()),
    )?;
    let hundred_k_edit_started = Instant::now();
    let hundred_k_edit = hundred_k.dispatch(Command::SetLocalTransform {
        target: fixture_node_id(1),
        transform: Affine2::translation(Vec2::new(2.0, 2.0)),
    })?;
    let hundred_k_edit_ms = hundred_k_edit_started.elapsed().as_secs_f64() * 1_000.0;
    let hundred_k_upload = renderer.sync_model(hundred_k.render_model(), &hundred_k_edit.render)?;

    let mut grouping = EngineRuntime::new(build_fixture(FixtureKind::Preview)?, Camera::default())?;
    let initial_snapshot = grouping.document().snapshot();
    let group_id = NodeId::from_uuid(Uuid::from_u128(0xe000_0000_0000_0000));
    let group_started = Instant::now();
    grouping.dispatch(Command::Group {
        group: NodeSpec::group(group_id, "Proof Group"),
        targets: vec![fixture_node_id(1), fixture_node_id(2)],
    })?;
    let group_ms = group_started.elapsed().as_secs_f64() * 1_000.0;
    let grouped_snapshot = grouping.document().snapshot();
    let group_history = grouping.history_state();
    let group_undo = grouping.undo()?.is_some();
    let group_undo_exact = grouping.document().snapshot() == initial_snapshot;
    let group_redo = grouping.redo()?.is_some();
    let group_redo_exact = grouping.document().snapshot() == grouped_snapshot;

    let ungroup_started = Instant::now();
    grouping.dispatch(Command::Ungroup { target: group_id })?;
    let ungroup_ms = ungroup_started.elapsed().as_secs_f64() * 1_000.0;
    let ungrouped_snapshot = grouping.document().snapshot();
    let ungroup_history = grouping.history_state();
    let ungroup_undo = grouping.undo()?.is_some();
    let ungroup_undo_exact = grouping.document().snapshot() == grouped_snapshot;
    let ungroup_redo = grouping.redo()?.is_some();
    let ungroup_redo_exact = grouping.document().snapshot() == initial_snapshot;

    let checks = json!({
        "actual_wgpu_device": !capabilities.adapter_name.is_empty(),
        "ten_k_single_leaf_bounded": ten_k_edit.render.dirty_slots.len() == 1
            && ten_k_edit.render.stats.document_nodes_scanned == 1
            && ten_k_edit.render.stats.render_items_cloned == 0
            && ten_k_edit.render.stats.full_render_model_scans == 0
            && ten_k_edit.render.stats.order_nodes_visited == 0
            && ten_k_upload.instance_upload_bytes == 48,
        "hundred_k_single_leaf_bounded": hundred_k_edit.render.dirty_slots.len() == 1
            && hundred_k_edit.render.stats.document_nodes_scanned == 1
            && hundred_k_edit.render.stats.render_items_cloned == 0
            && hundred_k_edit.render.stats.full_render_model_scans == 0
            && hundred_k_edit.render.stats.order_nodes_visited == 0
            && hundred_k_upload.instance_upload_bytes == 48,
        "group_one_history_entry": group_history.undo_depth == 1 && group_history.redo_depth == 0,
        "group_undo_redo_exact": group_undo && group_undo_exact && group_redo && group_redo_exact,
        "ungroup_one_history_entry": ungroup_history.undo_depth == 2 && ungroup_history.redo_depth == 0,
        "ungroup_undo_redo_exact": ungroup_undo && ungroup_undo_exact && ungroup_redo && ungroup_redo_exact,
        "group_ungroup_round_trip_exact": ungrouped_snapshot == initial_snapshot,
        "gpu_validation_errors_zero": renderer.metrics().validation_errors == 0,
    });
    let all_passed = checks
        .as_object()
        .expect("checks object")
        .values()
        .all(|value| value.as_bool() == Some(true));

    let metrics = json!({
        "phase": "0E",
        "captured_at_utc": chrono_free_timestamp(),
        "renderer": {
            "actual_wgpu": true,
            "adapter": capabilities.adapter_name,
            "backend": capabilities.backend,
            "device_type": capabilities.device_type,
            "driver": capabilities.driver,
            "driver_info": capabilities.driver_info,
        },
        "ten_k": {
            "load_ms": ten_k_load_ms,
            "initial_instance_upload_bytes": ten_k_initial.instance_upload_bytes,
            "single_leaf": {
                "wall_ms": ten_k_edit_ms,
                "dirty_items": ten_k_edit.render.stats.dirty_items,
                "document_nodes_scanned": ten_k_edit.render.stats.document_nodes_scanned,
                "render_items_cloned": ten_k_edit.render.stats.render_items_cloned,
                "full_render_model_scans": ten_k_edit.render.stats.full_render_model_scans,
                "order_nodes_visited": ten_k_edit.render.stats.order_nodes_visited,
                "instance_upload_bytes": ten_k_upload.instance_upload_bytes,
            },
        },
        "hundred_k": {
            "load_ms": hundred_k_load_ms,
            "initial_instance_upload_bytes": hundred_k_initial.instance_upload_bytes,
            "single_leaf": {
                "wall_ms": hundred_k_edit_ms,
                "dirty_items": hundred_k_edit.render.stats.dirty_items,
                "document_nodes_scanned": hundred_k_edit.render.stats.document_nodes_scanned,
                "render_items_cloned": hundred_k_edit.render.stats.render_items_cloned,
                "full_render_model_scans": hundred_k_edit.render.stats.full_render_model_scans,
                "order_nodes_visited": hundred_k_edit.render.stats.order_nodes_visited,
                "instance_upload_bytes": hundred_k_upload.instance_upload_bytes,
            },
        },
        "grouping": {
            "stable_group_node_id": group_id.to_string(),
            "group_ms": group_ms,
            "ungroup_ms": ungroup_ms,
            "group_history_depth": group_history.undo_depth,
            "ungroup_history_depth": ungroup_history.undo_depth,
            "round_trip_exact": ungrouped_snapshot == initial_snapshot,
        },
        "checks": checks,
        "all_passed": all_passed,
    });
    fs::write(&metrics_path, serde_json::to_vec_pretty(&metrics)?)?;
    println!("Phase 0E native structural proof");
    println!("metrics_json={}", metrics_path.display());
    println!(
        "adapter={} backend={}",
        capabilities.adapter_name, capabilities.backend
    );
    println!(
        "10k_load_ms={ten_k_load_ms:.3} single_ms={ten_k_edit_ms:.3} upload_bytes={}",
        ten_k_upload.instance_upload_bytes
    );
    println!(
        "100k_load_ms={hundred_k_load_ms:.3} single_ms={hundred_k_edit_ms:.3} upload_bytes={}",
        hundred_k_upload.instance_upload_bytes
    );
    println!("group_ms={group_ms:.3} ungroup_ms={ungroup_ms:.3}");
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("Phase 0E native proof failed".into());
    }
    Ok(())
}

fn full_delta(model: &RenderModel) -> RenderDelta {
    let mut dirty_slots = model.items().map(|item| item.slot).collect::<Vec<_>>();
    dirty_slots.sort_unstable();
    let dirty_ranges = if dirty_slots.is_empty() {
        Vec::new()
    } else {
        vec![DirtySlotRange {
            first: 0,
            count: dirty_slots.len() as u32,
        }]
    };
    RenderDelta {
        dirty_slots,
        removed_slots: Vec::new(),
        dirty_ranges,
        stats: model.last_update().clone(),
    }
}

fn chrono_free_timestamp() -> String {
    format!(
        "unix-seconds:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    )
}

fn metrics_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = std::env::args_os();
    let _binary = args.next();
    match args.next() {
        Some(path) => Ok(PathBuf::from(path)),
        None => Err("usage: phase0e_proof <metrics-json-path>".into()),
    }
}
