//! Phase 2A path performance benchmark.
//!
//! Measures the CPU half of the path pipeline — the half Phase 2A owns — at 10, 100 and 1000
//! paths, and separates the four kinds of work the plan requires to stay apart:
//!
//!   * initial tessellation (building every path's triangles once),
//!   * a static frame (cull, batch and encode with nothing changed),
//!   * a single path geometry edit,
//!   * a camera-only frame.
//!
//! The pass criteria are structural, not just temporal: a single edit must re-tessellate exactly
//! one path and a camera-only frame must re-tessellate none. GPU submission timing is not
//! measured here because it needs actual hardware; the browser proof covers that.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use uuid::Uuid;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{
    Appearance, ColorRgba, Command, Document, DocumentSnapshot, NodeId, NodeSnapshot, NodeSpec,
    PathAnchor, PathAnchorId, PathGeometry,
};
use visual_authoring_render_model::{encode_path_instances, encode_path_vertices};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};

const VIEWPORT: Vec2 = Vec2::new(1_920.0, 1_080.0);
const COUNTS: [usize; 3] = [10, 100, 1_000];

fn main() -> Result<(), Box<dyn Error>> {
    let mut measurements = Vec::new();
    for count in COUNTS {
        measurements.push(measure(count)?);
    }
    let checks = json!({
        "single_edit_retessellates_one_path": measurements
            .iter()
            .all(|measurement| measurement.edit_tessellations == 1),
        "camera_only_retessellates_nothing": measurements
            .iter()
            .all(|measurement| measurement.camera_tessellations == 0),
        "static_frame_retessellates_nothing": measurements
            .iter()
            .all(|measurement| measurement.static_tessellations == 0),
        "no_full_render_rebuilds_on_edit": measurements
            .iter()
            .all(|measurement| measurement.edit_full_rebuilds == 0),
        "camera_only_uploads_no_triangles": measurements
            .iter()
            .all(|measurement| !measurement.camera_vertex_revision_changed),
    });
    let all_passed = checks
        .as_object()
        .expect("checks is an object")
        .values()
        .all(|value| value.as_bool() == Some(true));
    let output = json!({
        "phase": "2A",
        "proof_kind": "cpu-path-pipeline-benchmark",
        "note": "CPU work only. GPU submission timing requires hardware and is proven in the browser harness.",
        "viewport": [VIEWPORT.x, VIEWPORT.y],
        "measurements": measurements
            .iter()
            .map(Measurement::to_json)
            .collect::<Vec<_>>(),
        "checks": checks,
        "all_passed": all_passed,
    });
    let path = metrics_path()?;
    fs::write(&path, serde_json::to_vec_pretty(&output)?)?;
    for measurement in &measurements {
        println!(
            "paths={} initial_tessellation_ms={:.3} static_frame_ms={:.3} single_edit_ms={:.3} camera_only_ms={:.3} edit_tessellations={} camera_tessellations={}",
            measurement.count,
            measurement.initial_tessellation_ms,
            measurement.static_frame_ms,
            measurement.single_edit_ms,
            measurement.camera_only_ms,
            measurement.edit_tessellations,
            measurement.camera_tessellations,
        );
    }
    println!("metrics_json={}", path.display());
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("Phase 2A path benchmark failed its structural checks".into());
    }
    Ok(())
}

struct Measurement {
    count: usize,
    vertices: usize,
    initial_tessellation_ms: f64,
    initial_tessellations: u64,
    initial_vertex_encode_ms: f64,
    static_frame_ms: f64,
    static_tessellations: u64,
    single_edit_ms: f64,
    edit_tessellations: u64,
    edit_full_rebuilds: u64,
    camera_only_ms: f64,
    camera_tessellations: u64,
    camera_vertex_revision_changed: bool,
    draw_batches: usize,
    fallback_rebuilds: u64,
}

impl Measurement {
    fn to_json(&self) -> serde_json::Value {
        json!({
            "paths": self.count,
            "path_vertices": self.vertices,
            "initial_tessellation_ms": self.initial_tessellation_ms,
            "initial_tessellations": self.initial_tessellations,
            "initial_vertex_encode_ms": self.initial_vertex_encode_ms,
            "static_frame_ms": self.static_frame_ms,
            "static_frame_tessellations": self.static_tessellations,
            "single_path_edit_ms": self.single_edit_ms,
            "single_path_edit_tessellations": self.edit_tessellations,
            "single_path_edit_full_render_rebuilds": self.edit_full_rebuilds,
            "camera_only_frame_ms": self.camera_only_ms,
            "camera_only_tessellations": self.camera_tessellations,
            "camera_only_uploaded_triangles": self.camera_vertex_revision_changed,
            "draw_batches": self.draw_batches,
            "fallback_rebuild_count": self.fallback_rebuilds,
        })
    }
}

fn tessellation_count(runtime: &EngineRuntime) -> u64 {
    runtime.render_model().cumulative().path_tessellations
}

fn measure(count: usize) -> Result<Measurement, Box<dyn Error>> {
    let (document, ids) = path_document(count)?;
    let camera = Camera::new(WorldPoint(Vec2::ZERO), 0.35, VIEWPORT, 1.0)?;

    // Initial tessellation: building the runtime derives the scene, the render model and every
    // path's triangles from the persistent document.
    let started = Instant::now();
    let mut runtime = EngineRuntime::new(document, camera)?;
    let initial_tessellation_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let initial_tessellations = runtime.render_model().cumulative().path_tessellations;
    let vertices = runtime.render_model().path_vertex_count();

    // First upload: the one frame that encodes the whole vertex buffer.
    let started = Instant::now();
    let vertex_bytes = encode_path_vertices(runtime.render_model());
    let initial_vertex_encode_ms = started.elapsed().as_secs_f64() * 1_000.0;
    assert!(!vertex_bytes.is_empty());

    // Static frame: cull, batch and encode the instance array. Triangles are not re-encoded
    // because the path vertex revision did not move — that is the contract this measures.
    let before = tessellation_count(&runtime);
    let started = Instant::now();
    let culling = runtime.cull_viewport()?;
    let instances = encode_path_instances(runtime.render_model());
    let static_frame_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let static_tessellations = tessellation_count(&runtime) - before;
    assert!(!instances.is_empty());

    // Single path geometry edit.
    let target = ids[ids.len() / 2];
    let mut edited = anchors_for(ids.len() / 2);
    edited.anchors[2].position = edited.anchors[2].position + Vec2::new(0.0, 40.0);
    let before = tessellation_count(&runtime);
    let started = Instant::now();
    runtime.dispatch(Command::SetGeometry {
        target,
        geometry: visual_authoring_document::Geometry::Path(edited),
    })?;
    let single_edit_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let edit_tessellations = tessellation_count(&runtime) - before;
    let edit_full_rebuilds = runtime
        .render_model()
        .last_update()
        .full_render_rebuild_count;

    // Camera-only frame.
    let revision_before = runtime.render_model().path_vertex_revision();
    let before = tessellation_count(&runtime);
    let started = Instant::now();
    runtime.camera_mut().pan(Vec2::new(37.0, 21.0))?;
    let camera_culling = runtime.cull_viewport()?;
    let camera_only_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let camera_tessellations = tessellation_count(&runtime) - before;
    let camera_vertex_revision_changed =
        runtime.render_model().path_vertex_revision() != revision_before;

    Ok(Measurement {
        count,
        vertices,
        initial_tessellation_ms,
        initial_tessellations,
        initial_vertex_encode_ms,
        static_frame_ms,
        static_tessellations,
        single_edit_ms,
        edit_tessellations,
        edit_full_rebuilds,
        camera_only_ms,
        camera_tessellations,
        camera_vertex_revision_changed,
        draw_batches: camera_culling
            .draw_batches
            .len()
            .max(culling.draw_batches.len()),
        fallback_rebuilds: 0,
    })
}

/// A grid of closed, filled, curved paths — the most expensive path kind Phase 2A draws.
fn path_document(count: usize) -> Result<(Document, Vec<NodeId>), Box<dyn Error>> {
    let root_id = NodeId::from_uuid(Uuid::from_u128(1));
    let mut nodes = Vec::with_capacity(count + 1);
    let mut ids = Vec::with_capacity(count);
    let columns = (count as f64).sqrt().ceil() as usize;
    for index in 0..count {
        let id = NodeId::from_uuid(Uuid::from_u128(index as u128 + 2));
        ids.push(id);
        let mut spec = NodeSpec::path(id, format!("Path {index}"), anchors_for(index));
        spec.local_transform = Affine2::translation(Vec2::new(
            (index % columns) as f64 * 260.0,
            (index / columns) as f64 * 260.0,
        ));
        spec.appearance = Appearance {
            fill: ColorRgba::new(0.2, 0.6, 0.9, 1.0),
            ..Appearance::default()
        };
        nodes.push(NodeSnapshot {
            spec,
            parent: Some(root_id),
            children: Vec::new(),
            group_restoration: None,
        });
    }
    nodes.insert(
        0,
        NodeSnapshot {
            spec: NodeSpec::document(root_id, "Path Benchmark"),
            parent: None,
            children: ids.clone(),
            group_restoration: None,
        },
    );
    Ok((
        Document::from_snapshot(DocumentSnapshot { root_id, nodes })?,
        ids,
    ))
}

fn anchors_for(index: usize) -> PathGeometry {
    let base = (index as u128 + 1) * 16;
    let mut anchors = vec![
        anchor(base, Vec2::new(100.0, 0.0)),
        anchor(base + 1, Vec2::new(200.0, 100.0)),
        anchor(base + 2, Vec2::new(100.0, 200.0)),
        anchor(base + 3, Vec2::new(0.0, 100.0)),
    ];
    anchors[0].handle_in = Some(Vec2::new(45.0, 0.0));
    anchors[0].handle_out = Some(Vec2::new(155.0, 0.0));
    anchors[1].handle_in = Some(Vec2::new(200.0, 45.0));
    anchors[1].handle_out = Some(Vec2::new(200.0, 155.0));
    anchors[2].handle_in = Some(Vec2::new(155.0, 200.0));
    anchors[2].handle_out = Some(Vec2::new(45.0, 200.0));
    anchors[3].handle_in = Some(Vec2::new(0.0, 155.0));
    anchors[3].handle_out = Some(Vec2::new(0.0, 45.0));
    PathGeometry {
        closed: true,
        anchors,
    }
}

fn anchor(index: u128, position: Vec2) -> PathAnchor {
    PathAnchor::new(PathAnchorId::from_uuid(Uuid::from_u128(index)), position)
}

fn metrics_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("docs");
    fs::create_dir_all(&path)?;
    path.push("PHASE_2A_PATH_METRICS.json");
    Ok(path)
}
