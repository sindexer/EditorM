use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use visual_authoring_core_math::Vec2;
use visual_authoring_render_model::{DirtySlotRange, RenderDelta, RenderModel};
use visual_authoring_renderer_wgpu::{RenderView, WgpuRenderer};
use visual_authoring_runtime::fixtures::{build_fixture, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};

const WIDTH: u32 = 1_920;
const HEIGHT: u32 = 1_080;
const DPR: f64 = 1.0;

fn main() -> Result<(), Box<dyn Error>> {
    let metrics_path = metrics_path()?;
    let mut renderer = pollster::block_on(WgpuRenderer::new_headless())?;
    let capabilities = renderer.capabilities().clone();
    let one_k = run_fixture(
        &mut renderer,
        FixtureKind::BenchA,
        Camera::new(
            WorldPoint(Vec2::new(620.0, 620.0)),
            0.8,
            Vec2::new(WIDTH as f64, HEIGHT as f64),
            DPR,
        )?,
        30,
        300,
    )?;
    let ten_k = run_fixture(
        &mut renderer,
        FixtureKind::BenchB,
        Camera::new(
            WorldPoint(Vec2::new(1_980.0, 1_980.0)),
            0.25,
            Vec2::new(WIDTH as f64, HEIGHT as f64),
            DPR,
        )?,
        10,
        60,
    )?;
    let checks = json!({
        "actual_wgpu_device": !capabilities.adapter_name.is_empty(),
        "one_thousand_visible": one_k.visible == 1_000,
        "one_thousand_single_batch": one_k.draw_calls == 1 && one_k.batches == 1,
        "one_thousand_measured_300": one_k.samples_ms.len() == 300,
        "one_thousand_p95_at_most_16_7_ms": one_k.p95_ms <= 16.7,
        "ten_thousand_visible_comparative": ten_k.visible == 10_000,
        "ten_thousand_measured_60": ten_k.samples_ms.len() == 60,
        "gpu_validation_errors_zero": renderer.metrics().validation_errors == 0,
    });
    let all_passed = checks
        .as_object()
        .expect("checks is an object")
        .values()
        .all(|value| value.as_bool() == Some(true));
    let output = json!({
        "phase": "1A",
        "proof_kind": "actual-native-wgpu-performance",
        "renderer": {
            "adapter": capabilities.adapter_name,
            "backend": capabilities.backend,
            "device_type": capabilities.device_type,
            "driver": capabilities.driver,
            "driver_info": capabilities.driver_info,
        },
        "required_1000_visible": one_k.to_json(),
        "comparative_10000_visible": ten_k.to_json(),
        "threshold_ms": 16.7,
        "checks": checks,
        "all_passed": all_passed,
    });
    fs::write(&metrics_path, serde_json::to_vec_pretty(&output)?)?;
    println!(
        "adapter={} backend={}",
        capabilities.adapter_name, capabilities.backend
    );
    println!("one_k_p95_ms={:.4}", one_k.p95_ms);
    println!("ten_k_comparative_p95_ms={:.4}", ten_k.p95_ms);
    println!("metrics_json={}", metrics_path.display());
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("Phase 1A native performance proof failed".into());
    }
    Ok(())
}

struct Measurement {
    fixture: &'static str,
    visible: usize,
    draw_calls: u64,
    batches: u64,
    warmups: usize,
    samples_ms: Vec<f64>,
    median_ms: f64,
    p95_ms: f64,
    maximum_ms: f64,
}

impl Measurement {
    fn to_json(&self) -> serde_json::Value {
        json!({
            "fixture": self.fixture,
            "viewport": [WIDTH, HEIGHT],
            "dpr": DPR,
            "visible_instances": self.visible,
            "draw_calls": self.draw_calls,
            "batches": self.batches,
            "warmup_frames": self.warmups,
            "measured_frames": self.samples_ms.len(),
            "raw_frame_ms": self.samples_ms,
            "median_ms": self.median_ms,
            "p95_ms": self.p95_ms,
            "maximum_ms": self.maximum_ms,
        })
    }
}

fn run_fixture(
    renderer: &mut WgpuRenderer,
    kind: FixtureKind,
    camera: Camera,
    warmups: usize,
    measured: usize,
) -> Result<Measurement, Box<dyn Error>> {
    let mut runtime = EngineRuntime::new(build_fixture(kind)?, camera)?;
    let culling = runtime.cull_viewport()?;
    renderer.sync_model(runtime.render_model(), &full_delta(runtime.render_model()))?;
    let view = render_view(runtime.camera());
    let mut last_frame =
        pollster::block_on(renderer.render_offscreen(&culling, view, WIDTH, HEIGHT))?;
    for _ in 1..warmups {
        last_frame = pollster::block_on(renderer.render_offscreen(&culling, view, WIDTH, HEIGHT))?;
    }
    let mut samples_ms = Vec::with_capacity(measured);
    for _ in 0..measured {
        let started = Instant::now();
        last_frame = pollster::block_on(renderer.render_offscreen(&culling, view, WIDTH, HEIGHT))?;
        samples_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    let mut sorted = samples_ms.clone();
    sorted.sort_by(f64::total_cmp);
    Ok(Measurement {
        fixture: kind.label(),
        visible: culling.exact_visible,
        draw_calls: last_frame.draw_calls,
        batches: last_frame.batches,
        warmups,
        median_ms: percentile(&sorted, 0.5),
        p95_ms: percentile(&sorted, 0.95),
        maximum_ms: sorted.last().copied().unwrap_or_default(),
        samples_ms,
    })
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index]
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
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: phase1a_proof <metrics-json-path>".into())
}
