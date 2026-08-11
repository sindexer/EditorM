use std::time::Instant;

use serde_json::{json, Value};
use visual_authoring_document::{Command, NodeSpec, SequenceWork};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, RuntimeCommandOutcome, RuntimeSceneOutcome};

const WARMUPS: usize = 10;
const MEASURED: usize = 30;
const KS: [usize; 7] = [3, 30, 100, 300, 1_000, 3_000, 10_000];
const PATTERNS: [&str; 5] = [
    "contiguous_front",
    "contiguous_middle",
    "uniform",
    "random_seed_0x5eed",
    "multiple_runs",
];

#[derive(Default)]
struct Samples {
    times: Vec<f64>,
    work: Vec<u64>,
    depth: Vec<u64>,
    rebalances: Vec<u64>,
}

impl Samples {
    fn push(&mut self, elapsed: f64, document: SequenceWork, scene: SequenceWork) {
        self.times.push(elapsed);
        self.work.push(work(document) + work(scene));
        self.depth.push(
            document
                .maximum_sequence_depth
                .max(scene.maximum_sequence_depth),
        );
        self.rebalances
            .push(document.tree_rebalances + scene.tree_rebalances);
    }

    fn json(&self) -> Value {
        json!({
            "raw_samples_ms": self.times,
            "raw_work_samples": self.work,
            "median_ms": percentile(&self.times, 0.5),
            "p95_ms": percentile(&self.times, 0.95),
            "maximum_work": self.work.iter().copied().max().unwrap_or(0),
            "maximum_sequence_depth": self.depth.iter().copied().max().unwrap_or(0),
            "tree_rebalances": self.rebalances.iter().copied().max().unwrap_or(0),
        })
    }

    fn max_work(&self) -> u64 {
        self.work.iter().copied().max().unwrap_or(0)
    }
}

struct MatrixEntry {
    count: usize,
    k: usize,
    pattern: &'static str,
    group: Samples,
    undo: Samples,
    redo: Samples,
    ungroup: Samples,
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize]
}

fn work(value: SequenceWork) -> u64 {
    value.entries_examined.max(value.rank_order_comparisons)
        + value.entries_copied
        + value.entries_moved_or_shifted
        + value.sequence_nodes_allocated
        + value.allocated_bytes.div_ceil(8)
        + value.tree_rebalances
        + value.full_sequence_scans
        + value.full_sequence_copies
        + value.dense_index_rewrites
        + value.fallback_or_rebuild_count
}

fn assert_sequence(value: SequenceWork, label: &str) {
    assert_eq!(value.full_sequence_scans, 0, "{label}: full scan");
    assert_eq!(value.full_sequence_copies, 0, "{label}: full copy");
    assert_eq!(value.dense_index_rewrites, 0, "{label}: dense rewrite");
    assert_eq!(value.fallback_or_rebuild_count, 0, "{label}: fallback");
}

fn assert_command(value: &RuntimeCommandOutcome) {
    assert_sequence(value.command.sequence_work(), "document");
    assert_sequence(value.scene.sequence_work, "scene");
    assert_eq!(value.scene.full_scene_rebuild_count, 0);
    assert_eq!(value.scene.fallback_rebuild_count, 0);
    assert_eq!(value.render.stats.full_render_rebuild_count, 0);
    assert_eq!(value.render.stats.full_render_model_scans, 0);
    assert_eq!(value.render.stats.render_items_cloned, 0);
    assert!(value.render.dirty_slots.is_empty());
}

fn assert_scene(value: &RuntimeSceneOutcome) {
    assert_sequence(value.sequence_work, "document");
    assert_sequence(value.scene.sequence_work, "scene");
    assert_eq!(value.scene.full_scene_rebuild_count, 0);
    assert_eq!(value.scene.fallback_rebuild_count, 0);
    assert_eq!(value.render.stats.full_render_rebuild_count, 0);
    assert_eq!(value.render.stats.full_render_model_scans, 0);
    assert_eq!(value.render.stats.render_items_cloned, 0);
    assert!(value.render.dirty_slots.is_empty());
}

fn target_indexes(count: usize, k: usize, pattern: &str) -> Vec<usize> {
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

fn run_count(kind: FixtureKind, count: usize) -> Vec<MatrixEntry> {
    let baseline = build_fixture(kind).expect("fixture");
    let root = baseline.root_id();
    let expected = baseline
        .node(root)
        .unwrap()
        .children()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    for k in KS {
        for (pattern_index, pattern) in PATTERNS.into_iter().enumerate() {
            let mut runtime = EngineRuntime::new(
                build_fixture(kind).expect("identical semantic baseline"),
                Camera::default(),
            )
            .expect("runtime");
            let targets = target_indexes(count, k, pattern)
                .into_iter()
                .map(|index| fixture_node_id(index as u128))
                .collect::<Vec<_>>();
            assert_eq!(targets.len(), k);
            let group_id =
                fixture_node_id(count as u128 * 1_000 + k as u128 * 10 + pattern_index as u128 + 1);
            let mut entry = MatrixEntry {
                count,
                k,
                pattern,
                group: Samples::default(),
                undo: Samples::default(),
                redo: Samples::default(),
                ungroup: Samples::default(),
            };
            for iteration in 0..WARMUPS + MEASURED {
                let started = Instant::now();
                let grouped = runtime
                    .dispatch(Command::Group {
                        group: NodeSpec::group(group_id, "R3 native matrix"),
                        targets: targets.clone(),
                    })
                    .expect("group");
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                assert_command(&grouped);
                if iteration >= WARMUPS {
                    entry.group.push(
                        elapsed,
                        grouped.command.sequence_work(),
                        grouped.scene.sequence_work,
                    );
                }

                let started = Instant::now();
                let undone = runtime.undo().expect("undo").expect("undo entry");
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                assert_scene(&undone);
                if iteration >= WARMUPS {
                    entry
                        .undo
                        .push(elapsed, undone.sequence_work, undone.scene.sequence_work);
                }

                let started = Instant::now();
                let redone = runtime.redo().expect("redo").expect("redo entry");
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                assert_scene(&redone);
                if iteration >= WARMUPS {
                    entry
                        .redo
                        .push(elapsed, redone.sequence_work, redone.scene.sequence_work);
                }

                let started = Instant::now();
                let ungrouped = runtime
                    .dispatch(Command::Ungroup { target: group_id })
                    .expect("ungroup");
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                assert_command(&ungrouped);
                if iteration >= WARMUPS {
                    entry.ungroup.push(
                        elapsed,
                        ungrouped.command.sequence_work(),
                        ungrouped.scene.sequence_work,
                    );
                }
            }
            let actual = runtime
                .document()
                .node(root)
                .unwrap()
                .children()
                .iter()
                .copied()
                .collect::<Vec<_>>();
            if actual != expected {
                let mismatch = actual
                    .iter()
                    .zip(&expected)
                    .position(|(left, right)| left != right)
                    .unwrap_or(actual.len().min(expected.len()));
                panic!(
                    "exact order mismatch count={count} k={k} pattern={pattern} index={mismatch} actual={:?} expected={:?}",
                    actual.get(mismatch),
                    expected.get(mismatch)
                );
            }
            entries.push(entry);
        }
    }
    entries
}

fn matrix_entry<'a>(
    entries: &'a [MatrixEntry],
    count: usize,
    k: usize,
    pattern: &str,
) -> &'a MatrixEntry {
    entries
        .iter()
        .find(|entry| entry.count == count && entry.k == k && entry.pattern == pattern)
        .expect("matrix entry")
}

#[test]
fn phase0e_r3_native_release_complexity_matrix() {
    let started = format!("{:?}", std::time::SystemTime::now());
    if cfg!(debug_assertions) {
        println!("Phase 0E-R3 complexity matrix is exercised by the required release test command");
        return;
    }
    let mut entries = run_count(FixtureKind::BenchB, 10_000);
    entries.extend(run_count(FixtureKind::BenchC, 100_000));
    let mut thresholds = Vec::new();
    for count in [10_000, 100_000] {
        for pattern in PATTERNS {
            let work_30 = matrix_entry(&entries, count, 30, pattern).group.max_work() as f64 / 30.0;
            let work_3000 = matrix_entry(&entries, count, 3_000, pattern)
                .group
                .max_work() as f64
                / 3_000.0;
            let per_k = work_3000 / work_30;
            assert!(per_k <= 2.0, "{count}/{pattern}: work/k ratio {per_k}");
            thresholds.push(json!({ "name": format!("work_per_k_{count}_{pattern}"), "value": per_k, "limit": 2.0, "passed": true }));
            let x10 = matrix_entry(&entries, count, 3_000, pattern)
                .group
                .max_work() as f64
                / matrix_entry(&entries, count, 300, pattern)
                    .group
                    .max_work()
                    .max(1) as f64;
            assert!(x10 <= 15.0, "{count}/{pattern}: x10 work ratio {x10}");
            thresholds.push(json!({ "name": format!("x10_work_{count}_{pattern}"), "value": x10, "limit": 15.0, "passed": true }));
        }
    }
    for k in KS {
        for pattern in PATTERNS {
            let ratio = matrix_entry(&entries, 100_000, k, pattern).group.max_work() as f64
                / matrix_entry(&entries, 10_000, k, pattern)
                    .group
                    .max_work()
                    .max(1) as f64;
            assert!(ratio <= 1.35, "{k}/{pattern}: N ratio {ratio}");
            thresholds.push(json!({ "name": format!("n_ratio_{k}_{pattern}"), "value": ratio, "limit": 1.35, "passed": true }));
        }
    }
    let report_entries = entries
        .iter()
        .map(|entry| {
            json!({
                "node_count": entry.count,
                "selection_count": entry.k,
                "pattern": entry.pattern,
                "warmups": WARMUPS,
                "measured_iterations": MEASURED,
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
        "layer": "native-release-runtime",
        "command": "cargo test --release -p visual_authoring_runtime --test phase0e_r3_matrix -- --nocapture",
        "started_at_utc": started,
        "finished_at_utc": format!("{:?}", std::time::SystemTime::now()),
        "fixture_creation_and_load_excluded": true,
        "matrix": report_entries,
        "thresholds": thresholds,
        "all_passed": true,
    });
    println!("PHASE0E_R3_NATIVE_MATRIX_JSON={report}");
}
