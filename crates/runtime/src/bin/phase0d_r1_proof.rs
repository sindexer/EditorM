use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::Command;
use visual_authoring_render_model::{DirtySlotRange, RenderDelta, RenderModel};
use visual_authoring_renderer_wgpu::{RenderView, WgpuRenderer};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};

fn main() -> Result<(), Box<dyn Error>> {
    let metrics_path = metrics_path()?;
    let mut renderer = pollster::block_on(WgpuRenderer::new_headless())?;
    let capabilities = renderer.capabilities().clone();

    let ten_k_build_started = Instant::now();
    let ten_k_camera = Camera::new(
        WorldPoint(Vec2::new(2_000.0, 2_000.0)),
        1.0,
        Vec2::new(5_000.0, 5_000.0),
        1.0,
    )?;
    let mut ten_k = EngineRuntime::new(build_fixture(FixtureKind::BenchB)?, ten_k_camera)?;
    let ten_k_build_ms = ten_k_build_started.elapsed().as_secs_f64() * 1_000.0;
    let ten_k_sort_started = Instant::now();
    let ten_k_culling = ten_k.cull_viewport()?;
    let ten_k_sort_ms = ten_k_sort_started.elapsed().as_secs_f64() * 1_000.0;
    let ten_k_upload =
        renderer.sync_model(ten_k.render_model(), &full_delta(ten_k.render_model()))?;
    let ten_k_frame = pollster::block_on(renderer.render_offscreen(
        &ten_k_culling,
        render_view(ten_k.camera()),
        1_024,
        768,
    ))?;
    let ten_k_single_started = Instant::now();
    let ten_k_single = ten_k.dispatch(Command::SetLocalTransform {
        target: fixture_node_id(1),
        transform: Affine2::translation(Vec2::new(1.0, 1.0)),
    })?;
    let ten_k_single_ms = ten_k_single_started.elapsed().as_secs_f64() * 1_000.0;
    let ten_k_single_upload = renderer.sync_model(ten_k.render_model(), &ten_k_single.render)?;

    let hundred_k_build_started = Instant::now();
    let mut hundred_k = EngineRuntime::new(build_fixture(FixtureKind::BenchC)?, Camera::default())?;
    let hundred_k_build_ms = hundred_k_build_started.elapsed().as_secs_f64() * 1_000.0;
    let initial_hundred_k = renderer.sync_model(
        hundred_k.render_model(),
        &full_delta(hundred_k.render_model()),
    )?;
    let target = fixture_node_id(1);
    let stable_slot = hundred_k.render_model().item(target).expect("item").slot;

    let single_started = Instant::now();
    let single = hundred_k.dispatch(Command::SetLocalTransform {
        target,
        transform: Affine2::translation(Vec2::new(1.0, 1.0)),
    })?;
    let single_cpu_ms = single_started.elapsed().as_secs_f64() * 1_000.0;
    let single_upload = renderer.sync_model(hundred_k.render_model(), &single.render)?;

    let revisions_before_camera = (
        hundred_k.document_revision(),
        hundred_k.scene().revision(),
        hundred_k.render_model().revision(),
    );
    hundred_k.camera_mut().pan(Vec2::new(4.0, 3.0))?;
    let camera_started = Instant::now();
    let narrow = hundred_k.cull_viewport()?;
    let camera_cpu_ms = camera_started.elapsed().as_secs_f64() * 1_000.0;
    let revisions_after_camera = (
        hundred_k.document_revision(),
        hundred_k.scene().revision(),
        hundred_k.render_model().revision(),
    );

    let omitted = hundred_k.dispatch(Command::SetLocalTransform {
        target,
        transform: Affine2::translation(Vec2::new(1.0e100, 0.0)),
    })?;
    let omitted_upload = renderer.sync_model(hundred_k.render_model(), &omitted.render)?;
    let omitted_slot = hundred_k.render_model().item(target).expect("item").slot;
    let omitted_culling = hundred_k.cull_viewport()?;
    let recovered = hundred_k.dispatch(Command::SetLocalTransform {
        target,
        transform: Affine2::translation(Vec2::new(2.0, 2.0)),
    })?;
    let recovered_upload = renderer.sync_model(hundred_k.render_model(), &recovered.render)?;
    let recovered_slot = hundred_k.render_model().item(target).expect("item").slot;

    let checks = json!({
        "actual_wgpu_device": !capabilities.adapter_name.is_empty(),
        "ten_k_one_batch_one_draw": ten_k_frame.batches == 1
            && ten_k_frame.draw_calls == 1
            && ten_k_frame.submitted_instances == 10_000,
        "ten_k_sibling_search_zero": ten_k_culling.sibling_search_steps == 0,
        "ten_k_single_bounded": ten_k_single.render.dirty_slots.len() == 1
            && ten_k_single.render.stats.render_items_cloned == 0
            && ten_k_single.render.stats.full_render_model_scans == 0
            && ten_k_single.render.stats.order_nodes_visited == 0
            && ten_k_single_upload.instance_upload_bytes == 48,
        "hundred_k_single_dirty_item": single.render.dirty_slots.len() == 1,
        "hundred_k_single_no_clone": single.render.stats.render_items_cloned == 0,
        "hundred_k_single_no_full_scan": single.render.stats.full_render_model_scans == 0,
        "hundred_k_single_no_order_walk": single.render.stats.order_nodes_visited == 0,
        "hundred_k_single_one_document_node": single.render.stats.document_nodes_scanned == 1,
        "hundred_k_single_one_render_item": single.render.stats.render_items_planned == 1
            && single.render.stats.render_items_read == 1
            && single.render.stats.render_items_written == 1,
        "hundred_k_single_no_rebuild": single.scene.full_scene_rebuild_count == 0
            && single.render.stats.full_render_rebuild_count == 0,
        "hundred_k_single_one_gpu_instance": single_upload.dirty_instance_ranges == 1
            && single_upload.instance_upload_bytes == 48
            && single_upload.full_instance_buffer_uploads == 0,
        "hundred_k_camera_no_full_render_scan": narrow.full_render_model_scans == 0,
        "hundred_k_camera_spatial_candidates_only": narrow.render_items_read == narrow.spatial_candidates
            && narrow.spatial_candidates < hundred_k.render_model().renderable_count(),
        "camera_revisions_unchanged": revisions_before_camera == revisions_after_camera,
        "f32_omission_zero_upload_and_visibility": omitted.render.dirty_slots.len() == 1
            && omitted_upload.gpu_encode_omitted == 1
            && omitted_upload.instance_upload_bytes == 48
            && hundred_k.render_model().gpu_omitted_count() == 0
            && omitted_culling.ids_top_to_bottom.iter().all(|id| *id != target),
        "f32_recovery_same_slot": stable_slot == omitted_slot
            && omitted_slot == recovered_slot
            && recovered_upload.gpu_encode_omitted == 0,
        "gpu_validation_errors_zero": renderer.metrics().validation_errors == 0,
    });
    let all_passed = checks
        .as_object()
        .expect("checks object")
        .values()
        .all(|value| value.as_bool() == Some(true));

    let metrics = json!({
        "phase": "0D-R1",
        "protocol_version": 1,
        "render_binary_schema_version": 1,
        "baseline_external_measurements": {
            "ten_k_single_edit_ms": 7.4,
            "hundred_k_single_edit_ms": 96.1,
            "ten_k_fully_visible_order_ms": 568.9,
            "hundred_k_load_ms": 2640.0,
        },
        "renderer": {
            "actual_wgpu": true,
            "adapter": capabilities.adapter_name,
            "backend": capabilities.backend,
            "device_type": capabilities.device_type,
            "driver": capabilities.driver,
            "driver_info": capabilities.driver_info,
        },
        "ten_k_fully_visible": {
            "build_ms": ten_k_build_ms,
            "cull_and_order_ms": ten_k_sort_ms,
            "total": ten_k.render_model().renderable_count(),
            "culling_candidates": ten_k_culling.spatial_candidates,
            "visible_items": ten_k_culling.exact_visible,
            "order_nodes_visited": ten_k_culling.order_nodes_visited,
            "sibling_search_steps": ten_k_culling.sibling_search_steps,
            "full_render_model_scans": ten_k_culling.full_render_model_scans,
            "draw_calls": ten_k_frame.draw_calls,
            "batches": ten_k_frame.batches,
            "submitted_instances": ten_k_frame.submitted_instances,
            "initial_instance_upload_bytes": ten_k_upload.instance_upload_bytes,
            "single_leaf_transform": {
                "wall_ms": ten_k_single_ms,
                "dirty_render_items": ten_k_single.render.stats.dirty_items,
                "render_items_cloned": ten_k_single.render.stats.render_items_cloned,
                "full_render_model_scans": ten_k_single.render.stats.full_render_model_scans,
                "order_nodes_visited": ten_k_single.render.stats.order_nodes_visited,
                "instance_upload_bytes": ten_k_single_upload.instance_upload_bytes,
            },
        },
        "hundred_k": {
            "load_ms": hundred_k_build_ms,
            "initial_instance_upload_bytes": initial_hundred_k.instance_upload_bytes,
            "single_leaf_transform": {
                "wall_ms": single_cpu_ms,
                "dirty_render_items": single.render.stats.dirty_items,
                "document_nodes_scanned": single.render.stats.document_nodes_scanned,
                "scene_nodes_visited": single.scene.visited_scene_nodes,
                "render_items_planned": single.render.stats.render_items_planned,
                "render_items_read": single.render.stats.render_items_read,
                "render_items_written": single.render.stats.render_items_written,
                "render_items_cloned": single.render.stats.render_items_cloned,
                "order_nodes_visited": single.render.stats.order_nodes_visited,
                "sibling_search_steps": single.render.stats.sibling_search_steps,
                "full_render_model_scans": single.render.stats.full_render_model_scans,
                "full_scene_rebuilds": single.scene.full_scene_rebuild_count,
                "full_render_rebuilds": single.render.stats.full_render_rebuild_count,
                "gpu_dirty_ranges": single_upload.dirty_instance_ranges,
                "instance_upload_bytes": single_upload.instance_upload_bytes,
                "full_instance_uploads": single_upload.full_instance_buffer_uploads,
                "allocation_growth_count": single_upload.allocation_growth_count,
            },
            "narrow_camera_update": {
                "wall_ms": camera_cpu_ms,
                "culling_candidates": narrow.spatial_candidates,
                "render_items_read": narrow.render_items_read,
                "visible_items": narrow.exact_visible,
                "full_render_model_scans": narrow.full_render_model_scans,
                "document_scene_render_revision_changes": revisions_before_camera != revisions_after_camera,
            },
            "f32_omission": {
                "gpu_encode_attempted": omitted_upload.gpu_encode_attempted,
                "gpu_encode_omitted": omitted_upload.gpu_encode_omitted,
                "instance_upload_bytes": omitted_upload.instance_upload_bytes,
                "visible_after_omission": omitted_culling.exact_visible,
                "stable_slot": stable_slot == omitted_slot && omitted_slot == recovered_slot,
                "recovered_gpu_encode_omitted": recovered_upload.gpu_encode_omitted,
            },
        },
        "checks": checks,
        "all_passed": all_passed,
    });
    fs::write(&metrics_path, serde_json::to_vec_pretty(&metrics)?)?;
    println!("Phase 0D-R1 native structural proof");
    println!("metrics_json={}", metrics_path.display());
    println!(
        "adapter={} backend={} device_type={}",
        capabilities.adapter_name, capabilities.backend, capabilities.device_type
    );
    println!(
        "10k order_ms={ten_k_sort_ms:.3} sibling_search_steps={} draw_calls={}",
        ten_k_culling.sibling_search_steps, ten_k_frame.draw_calls
    );
    println!(
        "10k single_ms={ten_k_single_ms:.3} cloned={} full_scans={} upload_bytes={}",
        ten_k_single.render.stats.render_items_cloned,
        ten_k_single.render.stats.full_render_model_scans,
        ten_k_single_upload.instance_upload_bytes
    );
    println!(
        "100k single_ms={single_cpu_ms:.3} cloned={} full_scans={} upload_bytes={}",
        single.render.stats.render_items_cloned,
        single.render.stats.full_render_model_scans,
        single_upload.instance_upload_bytes
    );
    println!(
        "100k camera_ms={camera_cpu_ms:.3} candidates={} full_scans={}",
        narrow.spatial_candidates, narrow.full_render_model_scans
    );
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("Phase 0D-R1 native structural proof failed".into());
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

fn render_view(camera: &Camera) -> RenderView {
    RenderView {
        center: camera.center().0,
        zoom: camera.zoom(),
        viewport_size: camera.viewport_size(),
        device_pixel_ratio: camera.device_pixel_ratio(),
    }
}

fn metrics_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = std::env::args_os();
    let _binary = args.next();
    match args.next() {
        Some(path) => Ok(PathBuf::from(path)),
        None => Err("usage: phase0d_r1_proof <metrics-json-path>".into()),
    }
}
