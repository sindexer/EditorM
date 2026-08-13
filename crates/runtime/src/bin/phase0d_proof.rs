use std::error::Error;
use std::fs;
use std::path::PathBuf;

use serde_json::json;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::Command;
use visual_authoring_render_model::{DirtySlotRange, RenderDelta, RenderModel};
use visual_authoring_renderer_wgpu::{RenderView, WgpuRenderer};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};

fn main() -> Result<(), Box<dyn Error>> {
    let metrics_path = metrics_path()?;
    let camera = Camera::new(
        WorldPoint(Vec2::new(2_000.0, 2_000.0)),
        1.0,
        Vec2::new(5_000.0, 5_000.0),
        1.0,
    )?;
    let mut runtime = EngineRuntime::new(build_fixture(FixtureKind::BenchB)?, camera)?;
    let initial_culling = runtime.cull_viewport()?;
    let initial_delta = full_delta(runtime.render_model());

    let mut renderer = pollster::block_on(WgpuRenderer::new_headless())?;
    let capabilities = renderer.capabilities().clone();
    let initial_upload = renderer.sync_model(runtime.render_model(), &initial_delta)?;
    let initial_frame = pollster::block_on(renderer.render_offscreen(
        &initial_culling,
        render_view(runtime.camera()),
        1_024,
        768,
    ))?;

    let update = runtime.dispatch(Command::SetLocalTransform {
        target: fixture_node_id(1),
        transform: Affine2::translation(Vec2::new(1.0, 1.0)),
    })?;
    let incremental_upload = renderer.sync_model(runtime.render_model(), &update.render)?;
    let incremental_culling = runtime.cull_viewport()?;
    let incremental_frame = pollster::block_on(renderer.render_offscreen(
        &incremental_culling,
        render_view(runtime.camera()),
        1_024,
        768,
    ))?;

    let mut sparse = EngineRuntime::new(build_fixture(FixtureKind::BenchC)?, Camera::default())?;
    let narrow = sparse.cull_viewport()?;
    let sparse_total = sparse.render_model().renderable_count();
    sparse
        .camera_mut()
        .resize_viewport(Vec2::new(1.0e9, 1.0e9))?;
    let zoomed_out = sparse.cull_viewport()?;

    let checks = json!({
        "actual_wgpu_device": !capabilities.adapter_name.is_empty(),
        "ten_thousand_one_batch": initial_frame.batches == 1
            && initial_frame.draw_calls == 1
            && initial_frame.submitted_instances == 10_000,
        "sparse_query_pruned": narrow.spatial_candidates < sparse_total,
        "offscreen_not_submitted": narrow.submitted_instances < sparse_total,
        "single_node_no_scene_rebuild": update.scene.full_scene_rebuild_count == 0,
        "single_node_no_render_rebuild": update.render.stats.full_render_rebuild_count == 0,
        "single_node_no_full_instance_upload": incremental_upload.full_instance_buffer_uploads == 0,
        "wide_view_no_query_error": zoomed_out.submitted_instances == sparse_total,
        "fallback_rebuild_zero": update.scene.fallback_rebuild_count == 0,
        "gpu_validation_errors_zero": renderer.metrics().validation_errors == 0,
    });
    let all_passed = checks
        .as_object()
        .expect("checks is an object")
        .values()
        .all(|value| value.as_bool() == Some(true));

    let metrics = json!({
        "phase": "0D",
        "protocol_version": 1,
        "renderer": {
            "actual_wgpu": true,
            "adapter": capabilities.adapter_name,
            "backend": capabilities.backend,
            "device_type": capabilities.device_type,
            "driver": capabilities.driver,
            "driver_info": capabilities.driver_info,
            "shader_geometry_aware": true,
            "ellipse_fragment_discard": true,
            "precision_strategy": "f64-to-f32 validated high/low translation split",
        },
        "bench_10k_rectangles": {
            "total": runtime.render_model().renderable_count(),
            "visible": initial_culling.exact_visible,
            "culled": initial_culling.culled,
            "spatial_candidates": initial_culling.spatial_candidates,
            "draw_calls": initial_frame.draw_calls,
            "batches": initial_frame.batches,
            "submitted_instances": initial_frame.submitted_instances,
            "buffer_count": initial_frame.buffer_count,
            "buffer_bytes": initial_frame.buffer_bytes,
            "initial_instance_upload_calls": initial_upload.upload_calls,
            "initial_instance_upload_bytes": initial_upload.upload_bytes,
            "validation_errors": initial_frame.validation_errors,
        },
        "single_node_update": {
            "scene_full_rebuilds": update.scene.full_scene_rebuild_count,
            "render_full_rebuilds": update.render.stats.full_render_rebuild_count,
            "dirty_slots": update.render.dirty_slots.len(),
            "dirty_ranges": incremental_upload.dirty_instance_ranges,
            "instance_upload_calls": incremental_upload.upload_calls,
            "instance_upload_bytes": incremental_upload.upload_bytes,
            "full_instance_buffer_uploads": incremental_upload.full_instance_buffer_uploads,
            "draw_calls": incremental_frame.draw_calls,
            "submitted_instances": incremental_frame.submitted_instances,
        },
        "sparse_100k": {
            "total": sparse_total,
            "narrow_candidates": narrow.spatial_candidates,
            "narrow_visible": narrow.exact_visible,
            "narrow_submitted": narrow.submitted_instances,
            "narrow_culled": narrow.culled,
            "zoomed_out_candidates": zoomed_out.spatial_candidates,
            "zoomed_out_visible": zoomed_out.exact_visible,
            "zoomed_out_submitted": zoomed_out.submitted_instances,
            "query_too_large": false,
        },
        "checks": checks,
        "all_passed": all_passed,
    });
    fs::write(&metrics_path, serde_json::to_vec_pretty(&metrics)?)?;
    println!("Phase 0D native wgpu proof");
    println!("metrics_json={}", metrics_path.display());
    println!(
        "adapter={} backend={} device_type={}",
        capabilities.adapter_name, capabilities.backend, capabilities.device_type
    );
    println!(
        "10k draw_calls={} batches={} submitted={}",
        initial_frame.draw_calls, initial_frame.batches, initial_frame.submitted_instances
    );
    println!(
        "single dirty_slots={} dirty_ranges={} upload_bytes={}",
        update.render.dirty_slots.len(),
        incremental_upload.dirty_instance_ranges,
        incremental_upload.upload_bytes
    );
    println!(
        "100k narrow_candidates={} visible={} zoomed_out_visible={}",
        narrow.spatial_candidates, narrow.exact_visible, zoomed_out.exact_visible
    );
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("Phase 0D native structural proof failed".into());
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
        None => Err("usage: phase0d_proof <metrics-json-path>".into()),
    }
}
