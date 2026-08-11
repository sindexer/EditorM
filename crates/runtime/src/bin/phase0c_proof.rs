use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::{json, Value};
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::Command;
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};
use visual_authoring_scene::SceneUpdateStats;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metrics_path = metrics_path()?;

    let (bench_a, _runtime_a) = build_runtime(FixtureKind::BenchA)?;

    let (mut bench_b, mut runtime_b) = build_runtime(FixtureKind::BenchB)?;
    let dirty_start = Instant::now();
    let dirty = runtime_b.dispatch(Command::SetLocalTransform {
        target: fixture_node_id(5_000),
        transform: Affine2::translation(Vec2::new(-1_000.0, -1_000.0)),
    })?;
    let dirty_elapsed_ms = elapsed_ms(dirty_start);
    let irrelevant = runtime_b.dispatch(Command::SetName {
        target: fixture_node_id(5_001),
        name: "Irrelevant property proof".into(),
    })?;
    bench_b["dirty_update_ms"] = json!(dirty_elapsed_ms);
    bench_b["dirty_update"] = stats_json(dirty.scene);
    bench_b["irrelevant_property_update"] = stats_json(irrelevant.scene);
    bench_b["document_revision"] = json!(runtime_b.document_revision());
    bench_b["scene_revision"] = json!(runtime_b.scene().revision());

    let (mut bench_c, mut runtime_c) = build_runtime(FixtureKind::BenchC)?;
    let viewport = runtime_c
        .camera()
        .world_to_viewport(WorldPoint(Vec2::new(5.0, 5.0)))?;
    let query_start = Instant::now();
    let hit = runtime_c.hit_test_viewport(viewport)?;
    let query_elapsed_ms = elapsed_ms(query_start);
    bench_c["point_query_ms"] = json!(query_elapsed_ms);
    bench_c["spatial_candidate_count"] = json!(hit.candidate_count());
    bench_c["exact_geometry_hit_test_count"] = json!(hit.exact_geometry_test_count());
    bench_c["topmost"] = json!(hit.topmost().map(|id| id.to_string()));

    let (bench_d, runtime_d) = build_runtime(FixtureKind::BenchD)?;

    let acceptance = json!({
        "bench_b_full_rebuild_zero": dirty.scene.full_scene_rebuild_count == 0,
        "bench_b_visited_below_64": dirty.scene.visited_scene_nodes < 64,
        "bench_b_recomputed_below_64": dirty.scene.world_transforms_recomputed < 64,
        "bench_b_one_spatial_update": dirty.scene.spatial_entries_updated == 1,
        "irrelevant_property_zero_recompute": irrelevant.scene.world_transforms_recomputed == 0
            && irrelevant.scene.bounds_recomputed == 0
            && irrelevant.scene.spatial_entries_updated == 0,
        "bench_c_candidates_at_most_64": hit.candidate_count() <= 64,
        "bench_c_exact_tests_at_most_64": hit.exact_geometry_test_count() <= 64,
        "bench_d_iterative_build_complete": runtime_d.scene().len() == 10_001,
        "revisions_synchronized": runtime_b.document_revision() == runtime_b.scene().revision()
    });
    let all_passed = acceptance
        .as_object()
        .expect("object")
        .values()
        .all(|value| value.as_bool() == Some(true));

    let report = json!({
        "schema": "visual-authoring-phase0c-metrics",
        "version": 1,
        "mode": "release",
        "fixtures": {
            "BENCH-A": bench_a,
            "BENCH-B": bench_b,
            "BENCH-C": bench_c,
            "BENCH-D": bench_d
        },
        "acceptance": acceptance,
        "all_passed": all_passed
    });
    if let Some(parent) = metrics_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&metrics_path, serde_json::to_string_pretty(&report)? + "\n")?;

    println!("Phase 0C release proof");
    println!("metrics_json={}", metrics_path.display());
    println!(
        "BENCH-A total_nodes={}",
        report["fixtures"]["BENCH-A"]["total_nodes"]
    );
    println!(
        "BENCH-B total_nodes={} visited={} world_recomputed={} bounds_recomputed={} spatial_updates={} full_rebuilds={}",
        report["fixtures"]["BENCH-B"]["total_nodes"],
        dirty.scene.visited_scene_nodes,
        dirty.scene.world_transforms_recomputed,
        dirty.scene.bounds_recomputed,
        dirty.scene.spatial_entries_updated,
        dirty.scene.full_scene_rebuild_count
    );
    println!(
        "BENCH-C total_nodes={} candidates={} exact_tests={}",
        report["fixtures"]["BENCH-C"]["total_nodes"],
        hit.candidate_count(),
        hit.exact_geometry_test_count()
    );
    println!(
        "BENCH-D total_nodes={} visited={} full_rebuilds={}",
        report["fixtures"]["BENCH-D"]["total_nodes"],
        runtime_d.scene().metrics().last_update.visited_scene_nodes,
        runtime_d
            .scene()
            .metrics()
            .last_update
            .full_scene_rebuild_count
    );
    println!(
        "revisions document={} scene={}",
        runtime_b.document_revision(),
        runtime_b.scene().revision()
    );
    println!("all_passed={all_passed}");
    if !all_passed {
        return Err("one or more structural acceptance checks failed".into());
    }
    Ok(())
}

fn build_runtime(kind: FixtureKind) -> Result<(Value, EngineRuntime), Box<dyn std::error::Error>> {
    let document_start = Instant::now();
    let document = build_fixture(kind)?;
    let fixture_build_ms = elapsed_ms(document_start);
    let total_nodes = document.len();
    let scene_start = Instant::now();
    let runtime = EngineRuntime::new(document, Camera::default())?;
    let scene_build_ms = elapsed_ms(scene_start);
    let stats = runtime.scene().metrics().last_update;
    let report = json!({
        "fixture": kind.label(),
        "total_nodes": total_nodes,
        "fixture_build_ms": fixture_build_ms,
        "scene_build_ms": scene_build_ms,
        "initial_scene": stats_json(stats),
        "attached_nodes": runtime.scene().metrics().attached_scene_nodes,
        "visible_nodes": runtime.scene().metrics().effective_visible_nodes,
        "invalid_derived_nodes": runtime.scene().metrics().invalid_derived_nodes,
        "indexed_nodes": runtime.scene().metrics().indexed_nodes
    });
    Ok((report, runtime))
}

fn stats_json(stats: SceneUpdateStats) -> Value {
    json!({
        "dirty_nodes": stats.dirty_nodes,
        "visited_scene_nodes": stats.visited_scene_nodes,
        "world_transforms_recomputed": stats.world_transforms_recomputed,
        "bounds_recomputed": stats.bounds_recomputed,
        "ancestor_aggregate_bounds_updated": stats.ancestor_aggregate_bounds_updated,
        "spatial_entries_inserted": stats.spatial_entries_inserted,
        "spatial_entries_removed": stats.spatial_entries_removed,
        "spatial_entries_updated": stats.spatial_entries_updated,
        "spatial_candidate_count": stats.spatial_candidate_count,
        "exact_geometry_hit_test_count": stats.exact_geometry_hit_test_count,
        "full_scene_rebuild_count": stats.full_scene_rebuild_count,
        "fallback_rebuild_count": stats.fallback_rebuild_count,
        "document_revision": stats.document_revision,
        "scene_revision": stats.scene_revision
    })
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn metrics_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut path = PathBuf::from("docs/PHASE_0C_METRICS.json");
    while let Some(argument) = args.next() {
        if argument == "--metrics-json" {
            path = PathBuf::from(args.next().ok_or("--metrics-json requires a path")?);
        } else {
            return Err(format!("unknown argument: {}", argument.to_string_lossy()).into());
        }
    }
    Ok(path)
}
