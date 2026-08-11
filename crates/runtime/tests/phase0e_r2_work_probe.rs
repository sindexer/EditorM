use serde_json::json;
use visual_authoring_document::{Command, NodeSpec, SequenceWork};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime};

fn count(work: SequenceWork) -> u64 {
    work.entries_examined.max(work.rank_order_comparisons)
        + work.entries_copied
        + work.entries_moved_or_shifted
        + work.sequence_nodes_allocated
        + work.full_sequence_scans
        + work.full_sequence_copies
        + work.dense_index_rewrites
}

fn work_json(work: SequenceWork) -> serde_json::Value {
    json!({
        "entries_examined": work.entries_examined,
        "entries_copied": work.entries_copied,
        "entries_moved_or_shifted": work.entries_moved_or_shifted,
        "rank_order_comparisons": work.rank_order_comparisons,
        "sequence_nodes_allocated": work.sequence_nodes_allocated,
        "allocated_bytes": work.allocated_bytes,
        "full_sequence_scans": work.full_sequence_scans,
        "full_sequence_copies": work.full_sequence_copies,
        "dense_index_rewrites": work.dense_index_rewrites,
    })
}

fn probe(kind: FixtureKind, size: usize) -> serde_json::Value {
    let mut runtime = EngineRuntime::new(build_fixture(kind).unwrap(), Camera::default()).unwrap();
    let root = runtime.document().root_id();
    let targets = vec![
        fixture_node_id(1),
        fixture_node_id((size / 2 + 1) as u128),
        fixture_node_id(size as u128),
    ];
    let group_id = fixture_node_id(size as u128 + 8_000_000);
    let grouped = runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group_id, "Probe"),
            targets: targets.clone(),
        })
        .unwrap();
    let inserted = fixture_node_id(size as u128 + 8_000_001);
    runtime
        .dispatch(Command::CreateNode {
            spec: NodeSpec::group(inserted, "Inserted"),
            parent: root,
            index: 0,
        })
        .unwrap();
    runtime
        .dispatch(Command::DeleteSubtree {
            target: fixture_node_id(2),
        })
        .unwrap();
    runtime
        .dispatch(Command::Reparent {
            child: fixture_node_id((size - 1) as u128),
            new_parent: root,
            index: 1,
        })
        .unwrap();
    let ungrouped = runtime
        .dispatch(Command::Ungroup { target: group_id })
        .unwrap();
    json!({
        "size": size,
        "group": {
            "document": work_json(grouped.command.sequence_work()),
            "scene": work_json(grouped.scene.sequence_work),
            "composite": count(grouped.command.sequence_work()) + count(grouped.scene.sequence_work),
        },
        "edited_ungroup": {
            "document": work_json(ungrouped.command.sequence_work()),
            "scene": work_json(ungrouped.scene.sequence_work),
            "composite": count(ungrouped.command.sequence_work()) + count(ungrouped.scene.sequence_work),
        }
    })
}

#[test]
fn print_phase0e_r2_work_probe() {
    let ten = probe(FixtureKind::BenchB, 10_000);
    let hundred = probe(FixtureKind::BenchC, 100_000);
    println!("PHASE0E_R2_WORK_PROBE={}", json!([ten, hundred]));
}
