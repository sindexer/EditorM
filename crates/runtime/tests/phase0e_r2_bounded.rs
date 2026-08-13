use std::time::Instant;

use serde_json::{json, Value};
use visual_authoring_document::{Command, HeadlessEditorCore, NodeId, NodeSpec, SequenceWork};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, RuntimeCommandOutcome, RuntimeSceneOutcome};
use visual_authoring_serialization::{from_json, to_json_pretty};

const WARMUPS: usize = 10;
const MEASURED: usize = 30;
const STRUCTURAL_EXECUTIONS_PER_SAMPLE: usize = if cfg!(debug_assertions) { 1 } else { 8 };

struct Measurements {
    elapsed_ms: Vec<f64>,
    work: Vec<u64>,
    allocated_bytes: Vec<u64>,
    executions_per_sample: usize,
}

impl Default for Measurements {
    fn default() -> Self {
        Self {
            elapsed_ms: Vec::new(),
            work: Vec::new(),
            allocated_bytes: Vec::new(),
            executions_per_sample: 1,
        }
    }
}

impl Measurements {
    fn structural() -> Self {
        Self {
            executions_per_sample: STRUCTURAL_EXECUTIONS_PER_SAMPLE,
            ..Self::default()
        }
    }

    fn push(&mut self, elapsed_ms: f64, document: SequenceWork, scene: SequenceWork) {
        self.elapsed_ms.push(elapsed_ms);
        self.work
            .push(structural_work(document) + structural_work(scene));
        self.allocated_bytes
            .push(document.allocated_bytes + scene.allocated_bytes);
    }

    fn push_time(&mut self, elapsed_ms: f64) {
        self.elapsed_ms.push(elapsed_ms);
    }

    fn measured_times(&self) -> Vec<f64> {
        self.elapsed_ms
            .chunks_exact(self.executions_per_sample)
            .skip(WARMUPS)
            .map(|chunk| chunk.iter().sum::<f64>() / self.executions_per_sample as f64)
            .collect()
    }

    fn measured_maxima(values: &[u64], executions_per_sample: usize) -> Vec<u64> {
        values
            .chunks_exact(executions_per_sample)
            .skip(WARMUPS)
            .map(|chunk| chunk.iter().copied().max().unwrap_or(0))
            .collect()
    }

    fn measured_json(&self) -> Value {
        let times = self.measured_times();
        let work = Self::measured_maxima(&self.work, self.executions_per_sample);
        let bytes = Self::measured_maxima(&self.allocated_bytes, self.executions_per_sample);
        json!({
            "samples_ms": times,
            "median_ms": percentile(&times, 0.50),
            "p95_ms": percentile(&times, 0.95),
            "work_samples": work,
            "max_work": work.iter().copied().max().unwrap_or(0),
            "allocated_bytes_samples": bytes,
            "max_allocated_bytes": bytes.iter().copied().max().unwrap_or(0),
            "executions_per_timing_sample": self.executions_per_sample,
        })
    }

    fn median(&self) -> f64 {
        percentile(&self.measured_times(), 0.50)
    }

    fn max_work(&self) -> u64 {
        Self::measured_maxima(&self.work, self.executions_per_sample)
            .into_iter()
            .max()
            .unwrap_or(0)
    }
}

struct FixtureResult {
    count: usize,
    targets: Vec<NodeId>,
    group: Measurements,
    undo: Measurements,
    redo: Measurements,
    ungroup: Measurements,
    edited_ungroup: Measurements,
    save: Measurements,
    load: Measurements,
    loaded_ungroup: Measurements,
}

fn structural_work(work: SequenceWork) -> u64 {
    work.entries_examined.max(work.rank_order_comparisons)
        + work.entries_copied
        + work.entries_moved_or_shifted
        + work.sequence_nodes_allocated
        + work.full_sequence_scans
        + work.full_sequence_copies
        + work.dense_index_rewrites
}

fn assert_sequence_bounded(work: SequenceWork, layer: &str) {
    assert_eq!(work.full_sequence_scans, 0, "{layer} full scan");
    assert_eq!(work.full_sequence_copies, 0, "{layer} full copy");
    assert_eq!(work.dense_index_rewrites, 0, "{layer} dense rewrite");
}

fn assert_command_bounded(outcome: &RuntimeCommandOutcome) {
    assert_sequence_bounded(outcome.command.sequence_work(), "document");
    assert_sequence_bounded(outcome.scene.sequence_work, "scene");
    assert_eq!(outcome.scene.full_scene_rebuild_count, 0);
    assert_eq!(outcome.scene.fallback_rebuild_count, 0);
    assert_eq!(outcome.render.stats.full_render_rebuild_count, 0);
    assert_eq!(outcome.render.stats.full_render_model_scans, 0);
    assert_eq!(outcome.render.stats.render_items_cloned, 0);
    assert!(outcome.render.dirty_slots.is_empty());
}

fn assert_scene_bounded(outcome: &RuntimeSceneOutcome) {
    assert_sequence_bounded(outcome.sequence_work, "document");
    assert_sequence_bounded(outcome.scene.sequence_work, "scene");
    assert_eq!(outcome.scene.full_scene_rebuild_count, 0);
    assert_eq!(outcome.scene.fallback_rebuild_count, 0);
    assert_eq!(outcome.render.stats.full_render_rebuild_count, 0);
    assert_eq!(outcome.render.stats.full_render_model_scans, 0);
    assert_eq!(outcome.render.stats.render_items_cloned, 0);
    assert!(outcome.render.dirty_slots.is_empty());
}

fn run_fixture(kind: FixtureKind, count: usize) -> FixtureResult {
    let baseline = build_fixture(kind).expect("fixture");
    let baseline_snapshot = baseline.snapshot();
    let root = baseline.root_id();
    let targets = vec![
        fixture_node_id(1),
        fixture_node_id((count / 2 + 1) as u128),
        fixture_node_id(count as u128),
    ];
    let group_id = fixture_node_id((count as u128) + 1_000_000);
    let mut runtime = EngineRuntime::new(baseline, Camera::default()).expect("runtime");
    let mut group = Measurements::structural();
    let mut undo = Measurements::structural();
    let mut redo = Measurements::structural();
    let mut ungroup = Measurements::structural();

    for _ in 0..((WARMUPS + MEASURED) * STRUCTURAL_EXECUTIONS_PER_SAMPLE) {
        let started = Instant::now();
        let grouped = runtime
            .dispatch(Command::Group {
                group: NodeSpec::group(group_id, "R2 bounded group"),
                targets: targets.clone(),
            })
            .expect("group");
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        assert_command_bounded(&grouped);
        group.push(
            elapsed,
            grouped.command.sequence_work(),
            grouped.scene.sequence_work,
        );

        let started = Instant::now();
        let undone = runtime.undo().expect("undo").expect("undo entry");
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        assert_scene_bounded(&undone);
        undo.push(elapsed, undone.sequence_work, undone.scene.sequence_work);

        let started = Instant::now();
        let redone = runtime.redo().expect("redo").expect("redo entry");
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        assert_scene_bounded(&redone);
        redo.push(elapsed, redone.sequence_work, redone.scene.sequence_work);

        let started = Instant::now();
        let ungrouped = runtime
            .dispatch(Command::Ungroup { target: group_id })
            .expect("ungroup");
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        assert_command_bounded(&ungrouped);
        ungroup.push(
            elapsed,
            ungrouped.command.sequence_work(),
            ungrouped.scene.sequence_work,
        );
    }
    assert_eq!(runtime.document().snapshot(), baseline_snapshot);

    let edited_group_id = fixture_node_id((count as u128) + 8_000_000);
    let inserted_id = fixture_node_id((count as u128) + 8_000_001);
    let deleted_id = fixture_node_id(2);
    let reordered_id = fixture_node_id((count - 1) as u128);
    let mut edited_ungroup = Measurements::structural();
    for _ in 0..((WARMUPS + MEASURED) * STRUCTURAL_EXECUTIONS_PER_SAMPLE) {
        let grouped = runtime
            .dispatch(Command::Group {
                group: NodeSpec::group(edited_group_id, "R2 edited group"),
                targets: targets.clone(),
            })
            .expect("edited group");
        assert_command_bounded(&grouped);
        let created = runtime
            .dispatch(Command::CreateNode {
                spec: NodeSpec::group(inserted_id, "Inserted sibling"),
                parent: root,
                index: 0,
            })
            .expect("insert sibling");
        assert_command_bounded(&created);
        let deleted = runtime
            .dispatch(Command::DeleteSubtree { target: deleted_id })
            .expect("delete sibling");
        assert_command_bounded(&deleted);
        let reordered = runtime
            .dispatch(Command::Reparent {
                child: reordered_id,
                new_parent: root,
                index: 1,
            })
            .expect("reorder sibling");
        assert_command_bounded(&reordered);
        let started = Instant::now();
        let result = runtime
            .dispatch(Command::Ungroup {
                target: edited_group_id,
            })
            .expect("edited ungroup");
        assert_command_bounded(&result);
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        edited_ungroup.push(
            elapsed,
            result.command.sequence_work(),
            result.scene.sequence_work,
        );
        for _ in 0..5 {
            runtime.undo().expect("workflow reset").expect("entry");
        }
    }
    assert_eq!(runtime.document().snapshot(), baseline_snapshot);

    let persistence_group_id = fixture_node_id((count as u128) + 4_000_000);
    let mut editor =
        HeadlessEditorCore::new(build_fixture(kind).expect("persistence fixture")).expect("editor");
    editor
        .dispatch(Command::Group {
            group: NodeSpec::group(persistence_group_id, "Persisted group"),
            targets: targets.clone(),
        })
        .expect("persistence group");
    let grouped_document = editor.document();
    let encoded = to_json_pretty(grouped_document).expect("encode baseline");
    let mut save = Measurements::default();
    for _ in 0..(WARMUPS + MEASURED) {
        let started = Instant::now();
        let current = to_json_pretty(grouped_document).expect("save");
        save.push_time(started.elapsed().as_secs_f64() * 1_000.0);
        assert_eq!(current.len(), encoded.len());
    }
    let mut load = Measurements::default();
    for _ in 0..(WARMUPS + MEASURED) {
        let started = Instant::now();
        let loaded = from_json(&encoded).expect("load");
        load.push_time(started.elapsed().as_secs_f64() * 1_000.0);
        assert_eq!(loaded.len(), count + 2);
    }

    let loaded = from_json(&encoded).expect("loaded ungroup fixture");
    let mut loaded_editor = HeadlessEditorCore::new(loaded).expect("loaded editor");
    let mut loaded_ungroup = Measurements::default();
    for _ in 0..(WARMUPS + MEASURED) {
        let started = Instant::now();
        let outcome = loaded_editor
            .dispatch(Command::Ungroup {
                target: persistence_group_id,
            })
            .expect("loaded ungroup");
        loaded_ungroup.push(
            started.elapsed().as_secs_f64() * 1_000.0,
            outcome.sequence_work(),
            SequenceWork::default(),
        );
        assert_sequence_bounded(outcome.sequence_work(), "loaded document");
        assert_eq!(
            loaded_editor
                .document()
                .node(root)
                .unwrap()
                .children()
                .len(),
            count
        );
        loaded_editor
            .undo()
            .expect("loaded ungroup reset")
            .expect("loaded ungroup history entry");
    }

    FixtureResult {
        count,
        targets,
        group,
        undo,
        redo,
        ungroup,
        edited_ungroup,
        save,
        load,
        loaded_ungroup,
    }
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * fraction).ceil() as usize;
    sorted[index]
}

fn operation_json(measurements: &Measurements) -> Value {
    measurements.measured_json()
}

fn fixture_json(result: &FixtureResult) -> Value {
    json!({
        "node_count": result.count,
        "targets": result.targets.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "warmups": WARMUPS,
        "measured_iterations": MEASURED,
        "structural_executions_per_timing_sample": STRUCTURAL_EXECUTIONS_PER_SAMPLE,
        "operations": {
            "group": operation_json(&result.group),
            "undo": operation_json(&result.undo),
            "redo": operation_json(&result.redo),
            "ungroup": operation_json(&result.ungroup),
            "sibling_insert_delete_reorder_then_ungroup": operation_json(&result.edited_ungroup),
            "save_explicit_full_boundary": operation_json(&result.save),
            "load_explicit_full_boundary": operation_json(&result.load),
            "loaded_ungroup": operation_json(&result.loaded_ungroup),
        }
    })
}

#[test]
fn native_release_r2_structural_work_is_bounded_at_10k_and_100k() {
    if cfg!(debug_assertions) {
        println!("Phase 0E-R2 bounded timing matrix is exercised by its release test command");
        return;
    }
    let ten_k = run_fixture(FixtureKind::BenchB, 10_000);
    let hundred_k = run_fixture(FixtureKind::BenchC, 100_000);
    for (name, small, large) in [
        ("group", &ten_k.group, &hundred_k.group),
        ("undo", &ten_k.undo, &hundred_k.undo),
        ("redo", &ten_k.redo, &hundred_k.redo),
        ("ungroup", &ten_k.ungroup, &hundred_k.ungroup),
        (
            "sibling_edit_ungroup",
            &ten_k.edited_ungroup,
            &hundred_k.edited_ungroup,
        ),
        (
            "loaded_ungroup",
            &ten_k.loaded_ungroup,
            &hundred_k.loaded_ungroup,
        ),
    ] {
        let work_ratio = hundred_k_work_ratio(small, large);
        let time_ratio = large.median() / small.median().max(f64::EPSILON);
        assert!(work_ratio <= 1.35, "{name} work ratio {work_ratio}");
        assert!(time_ratio <= 3.0, "{name} time ratio {time_ratio}");
    }
    let report = json!({
        "phase": "0E-R2",
        "layer": "native-release",
        "threshold_scope": "bounded structural edits; save/load are explicit full persistence boundaries",
        "fixtures": [fixture_json(&ten_k), fixture_json(&hundred_k)],
        "ratios_100k_over_10k": {
            "group": ratio_json(&ten_k.group, &hundred_k.group),
            "undo": ratio_json(&ten_k.undo, &hundred_k.undo),
            "redo": ratio_json(&ten_k.redo, &hundred_k.redo),
            "ungroup": ratio_json(&ten_k.ungroup, &hundred_k.ungroup),
            "sibling_insert_delete_reorder_then_ungroup": ratio_json(&ten_k.edited_ungroup, &hundred_k.edited_ungroup),
            "loaded_ungroup": ratio_json(&ten_k.loaded_ungroup, &hundred_k.loaded_ungroup),
        },
        "all_passed": true,
    });
    println!("PHASE0E_R2_NATIVE_JSON={report}");
}

fn hundred_k_work_ratio(small: &Measurements, large: &Measurements) -> f64 {
    large.max_work() as f64 / (small.max_work().max(1) as f64)
}

fn ratio_json(small: &Measurements, large: &Measurements) -> Value {
    json!({
        "work_count": hundred_k_work_ratio(small, large),
        "median_time": large.median() / small.median().max(f64::EPSILON),
    })
}
