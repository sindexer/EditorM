use std::time::Instant;

use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use visual_authoring_runtime::{Camera, EngineRuntime, RenderSyncStatus, SceneSyncStatus};

fn exercise(kind: FixtureKind, count: usize, group_id: NodeId) {
    let document = build_fixture(kind).expect("fixture is valid");
    let initial = document.snapshot();
    let root = document.root_id();
    let targets = vec![
        fixture_node_id(1),
        fixture_node_id((count / 2 + 1) as u128),
        fixture_node_id(count as u128),
    ];
    let mut runtime = EngineRuntime::new(document, Camera::default()).expect("runtime builds");
    let revision = runtime.document_revision();
    let history = runtime.history_state().undo_depth;

    let group_started = Instant::now();
    let grouped = runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group_id, "R1 bounded group"),
            targets: targets.clone(),
        })
        .expect("group succeeds incrementally");
    let group_elapsed = group_started.elapsed();
    assert_eq!(runtime.document_revision(), revision + 1);
    assert_eq!(runtime.scene().revision(), revision + 1);
    assert_eq!(runtime.render_model().revision(), revision + 1);
    assert_eq!(runtime.history_state().undo_depth, history + 1);
    assert!(matches!(grouped.sync, SceneSyncStatus::Incremental));
    assert!(matches!(grouped.render_sync, RenderSyncStatus::Incremental));
    assert_eq!(grouped.scene.full_scene_rebuild_count, 0);
    assert_eq!(grouped.scene.fallback_rebuild_count, 0);
    assert_eq!(grouped.render.stats.full_render_rebuild_count, 0);
    assert_eq!(grouped.render.stats.render_items_cloned, 0);
    assert_eq!(grouped.render.dirty_slots.len(), 0);
    assert_eq!(grouped.render.stats.dirty_items, 0);
    assert_eq!(
        runtime.document().node(group_id).unwrap().children(),
        targets
    );
    assert_eq!(
        runtime.document().node(root).unwrap().children()[0],
        group_id
    );
    let grouped_snapshot = runtime.document().snapshot();

    let ungroup_started = Instant::now();
    let ungrouped = runtime
        .dispatch(Command::Ungroup { target: group_id })
        .expect("ungroup succeeds incrementally");
    let ungroup_elapsed = ungroup_started.elapsed();
    assert_eq!(runtime.document_revision(), revision + 2);
    assert_eq!(runtime.scene().revision(), revision + 2);
    assert_eq!(runtime.render_model().revision(), revision + 2);
    assert_eq!(runtime.history_state().undo_depth, history + 2);
    assert!(matches!(ungrouped.sync, SceneSyncStatus::Incremental));
    assert!(matches!(
        ungrouped.render_sync,
        RenderSyncStatus::Incremental
    ));
    assert_eq!(ungrouped.scene.full_scene_rebuild_count, 0);
    assert_eq!(ungrouped.scene.fallback_rebuild_count, 0);
    assert_eq!(ungrouped.render.stats.full_render_rebuild_count, 0);
    assert_eq!(ungrouped.render.stats.render_items_cloned, 0);
    assert_eq!(ungrouped.render.dirty_slots.len(), 0);
    assert_eq!(runtime.document().snapshot(), initial);

    runtime
        .undo()
        .expect("undo ungroup succeeds")
        .expect("entry");
    assert_eq!(runtime.document().snapshot(), grouped_snapshot);
    runtime.undo().expect("undo group succeeds").expect("entry");
    assert_eq!(runtime.document().snapshot(), initial);
    runtime.redo().expect("redo group succeeds").expect("entry");
    assert_eq!(runtime.document().snapshot(), grouped_snapshot);
    runtime
        .redo()
        .expect("redo ungroup succeeds")
        .expect("entry");
    assert_eq!(runtime.document().snapshot(), initial);

    println!(
        "nodes={count} group_us={} ungroup_us={} group_scene_visited={} ungroup_scene_visited={} group_render_visited={} ungroup_render_visited={}",
        group_elapsed.as_micros(),
        ungroup_elapsed.as_micros(),
        grouped.scene.visited_scene_nodes,
        ungrouped.scene.visited_scene_nodes,
        grouped.render.stats.scene_nodes_visited,
        ungrouped.render.stats.scene_nodes_visited,
    );
}

#[test]
fn group_ungroup_first_middle_last_are_bounded_at_10k() {
    exercise(FixtureKind::BenchB, 10_000, fixture_node_id(200_000));
}

#[test]
fn group_ungroup_first_middle_last_are_bounded_at_100k() {
    exercise(FixtureKind::BenchC, 100_000, fixture_node_id(300_000));
}
