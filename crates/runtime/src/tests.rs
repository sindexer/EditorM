use std::collections::BTreeMap;

use visual_authoring_core_math::{Affine2, Rect, Vec2, DEFAULT_EPSILON};
use visual_authoring_document::{
    Appearance, Command, Document, EditorError, Geometry, Metadata, NodeId, NodeSpec,
};
use visual_authoring_scene::{ComputedScene, InvalidDerivedState};

use crate::fixtures::{build_fixture, fixture_node_id, FixtureKind};
use crate::{
    Camera, CameraError, EngineRuntime, RenderSyncStatus, RuntimeError, ViewportPoint, WorldPoint,
};

fn create_node(runtime: &mut EngineRuntime, mut spec: NodeSpec, parent: NodeId) -> NodeId {
    let id = spec.id;
    if spec.local_transform == Affine2::IDENTITY {
        spec.local_transform = Affine2::IDENTITY;
    }
    let index = runtime.document().node(parent).unwrap().children().len();
    runtime
        .dispatch(Command::CreateNode {
            spec,
            parent,
            index,
        })
        .unwrap();
    id
}

fn rectangle(
    runtime: &mut EngineRuntime,
    parent: NodeId,
    id: NodeId,
    size: Vec2,
    transform: Affine2,
) -> NodeId {
    let mut spec = NodeSpec::rectangle(id, "Rectangle", size);
    spec.local_transform = transform;
    create_node(runtime, spec, parent)
}

fn ellipse(
    runtime: &mut EngineRuntime,
    parent: NodeId,
    id: NodeId,
    size: Vec2,
    transform: Affine2,
) -> NodeId {
    let mut spec = NodeSpec::ellipse(id, "Ellipse", size);
    spec.local_transform = transform;
    create_node(runtime, spec, parent)
}

fn hit_world(runtime: &mut EngineRuntime, point: Vec2) -> visual_authoring_scene::HitTestResult {
    let viewport = runtime
        .camera()
        .world_to_viewport(WorldPoint(point))
        .unwrap();
    runtime.hit_test_viewport(viewport).unwrap()
}

fn assert_scene_matches_fresh(runtime: &EngineRuntime) {
    let fresh = ComputedScene::build(runtime.document(), runtime.document_revision()).unwrap();
    assert_eq!(
        runtime.scene().semantic_snapshot(),
        fresh.semantic_snapshot()
    );
    assert_eq!(runtime.scene().revision(), runtime.document_revision());
}

#[test]
fn fresh_scene_rebuild_matches_document_meaning() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 10.0),
        Affine2::translation(Vec2::new(30.0, 40.0)),
    );
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn incremental_transform_matches_fresh_rebuild() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 10.0),
        Affine2::IDENTITY,
    );
    let outcome = runtime
        .dispatch(Command::SetLocalTransform {
            target: id,
            transform: Affine2::translation(Vec2::new(70.0, -20.0)),
        })
        .unwrap();
    assert_eq!(outcome.scene.full_scene_rebuild_count, 0);
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn deterministic_command_sequence_matches_rebuild_oracle() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let ids: Vec<_> = (0..16)
        .map(|index| {
            rectangle(
                &mut runtime,
                root,
                NodeId::new(),
                Vec2::new(10.0 + index as f64, 8.0),
                Affine2::translation(Vec2::new(index as f64 * 20.0, 0.0)),
            )
        })
        .collect();
    let mut seed = 0x5eed_u64;
    for step in 0..128 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let id = ids[(seed as usize) % ids.len()];
        match step % 4 {
            0 => {
                runtime
                    .dispatch(Command::SetLocalTransform {
                        target: id,
                        transform: Affine2::translation(Vec2::new(
                            (seed % 1_000) as f64,
                            ((seed >> 12) % 1_000) as f64,
                        )),
                    })
                    .unwrap();
            }
            1 => {
                runtime
                    .dispatch(Command::SetVisible {
                        target: id,
                        visible: seed & 1 == 0,
                    })
                    .unwrap();
            }
            2 => {
                runtime
                    .dispatch(Command::SetGeometry {
                        target: id,
                        geometry: Geometry::Rectangle {
                            size: Vec2::new(5.0 + (seed % 50) as f64, 9.0),
                        },
                    })
                    .unwrap();
            }
            _ => {
                runtime
                    .dispatch(Command::SetName {
                        target: id,
                        name: format!("N{seed}"),
                    })
                    .unwrap();
            }
        }
    }
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn undo_and_redo_keep_scene_equal_to_rebuild() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime
        .dispatch(Command::SetLocalTransform {
            target: id,
            transform: Affine2::translation(Vec2::new(100.0, 0.0)),
        })
        .unwrap();
    runtime.undo().unwrap().unwrap();
    assert_scene_matches_fresh(&runtime);
    runtime.redo().unwrap().unwrap();
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn transaction_preview_and_commit_do_not_repeat_scene_work() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime.begin_transaction().unwrap();
    for x in 1..=32 {
        runtime
            .update_transaction(Command::SetLocalTransform {
                target: id,
                transform: Affine2::translation(Vec2::new(x as f64, 0.0)),
            })
            .unwrap();
    }
    let revision_before_commit = runtime.document_revision();
    let scene_before_commit = runtime.scene().semantic_snapshot();
    assert!(runtime.commit_transaction().unwrap());
    assert_eq!(runtime.document_revision(), revision_before_commit);
    assert_eq!(runtime.scene().semantic_snapshot(), scene_before_commit);
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn transaction_rollback_restores_scene_exactly() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let before = runtime.scene().semantic_snapshot();
    runtime.begin_transaction().unwrap();
    runtime
        .update_transaction(Command::SetLocalTransform {
            target: id,
            transform: Affine2::translation(Vec2::new(500.0, 200.0)),
        })
        .unwrap();
    runtime.rollback_transaction().unwrap();
    assert_eq!(runtime.scene().semantic_snapshot(), before);
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn document_replacement_rebuilds_and_synchronizes_revision() {
    let mut runtime = EngineRuntime::blank("Original").unwrap();
    let replacement = build_fixture(FixtureKind::BenchA).unwrap();
    let outcome = runtime.replace_document(replacement).unwrap();
    assert_eq!(outcome.scene.full_scene_rebuild_count, 1);
    assert_eq!(runtime.document_revision(), runtime.scene().revision());
    assert_eq!(runtime.scene().len(), 1_001);
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn transform_dirty_visits_only_target_subtree_and_ancestors() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Group"), root);
    let child = rectangle(
        &mut runtime,
        group,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let unrelated = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::translation(Vec2::new(1_000.0, 0.0)),
    );
    let unrelated_revision = runtime
        .scene()
        .node(unrelated)
        .unwrap()
        .last_changed_revision();
    let outcome = runtime
        .dispatch(Command::SetLocalTransform {
            target: group,
            transform: Affine2::translation(Vec2::new(50.0, 0.0)),
        })
        .unwrap();
    assert_eq!(outcome.scene.world_transforms_recomputed, 2);
    assert!(outcome.scene.visited_scene_nodes < 8);
    assert_eq!(
        runtime
            .scene()
            .node(unrelated)
            .unwrap()
            .last_changed_revision(),
        unrelated_revision
    );
    assert!(runtime
        .scene()
        .node(child)
        .unwrap()
        .world_transform()
        .is_some());
}

#[test]
fn geometry_dirty_updates_one_spatial_entry() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    let outcome = runtime
        .dispatch(Command::SetGeometry {
            target: id,
            geometry: Geometry::Rectangle {
                size: Vec2::new(40.0, 20.0),
            },
        })
        .unwrap();
    assert_eq!(outcome.scene.world_transforms_recomputed, 0);
    assert_eq!(outcome.scene.bounds_recomputed, 1);
    assert_eq!(outcome.scene.spatial_entries_updated, 1);
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn visibility_dirty_removes_and_restores_descendant_spatial_entries() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Group"), root);
    let child = rectangle(
        &mut runtime,
        group,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    let hidden = runtime
        .dispatch(Command::SetVisible {
            target: group,
            visible: false,
        })
        .unwrap();
    assert_eq!(hidden.scene.world_transforms_recomputed, 0);
    assert_eq!(hidden.scene.spatial_entries_removed, 1);
    assert!(!runtime.scene().node(child).unwrap().indexed());
    let restored = runtime.undo().unwrap().unwrap();
    assert_eq!(restored.scene.spatial_entries_inserted, 1);
    assert!(runtime.scene().node(child).unwrap().indexed());
}

#[test]
fn attach_detach_and_reparent_update_attached_and_world_state() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let detached = NodeId::new();
    runtime
        .dispatch(Command::RegisterNode {
            spec: NodeSpec::group(detached, "Detached"),
        })
        .unwrap();
    let child = rectangle(
        &mut runtime,
        detached,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    assert!(!runtime.scene().node(child).unwrap().attached());
    runtime
        .dispatch(Command::Attach {
            child: detached,
            parent: root,
            index: 0,
        })
        .unwrap();
    assert!(runtime.scene().node(child).unwrap().attached());
    runtime
        .dispatch(Command::Detach { child: detached })
        .unwrap();
    assert!(!runtime.scene().node(child).unwrap().attached());
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn delete_undo_redo_restore_scene_and_spatial_entries() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Group"), root);
    let child = rectangle(
        &mut runtime,
        group,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    runtime
        .dispatch(Command::DeleteSubtree { target: group })
        .unwrap();
    assert!(runtime.scene().node(group).is_none());
    assert!(runtime.scene().node(child).is_none());
    runtime.undo().unwrap().unwrap();
    assert!(runtime.scene().node(child).unwrap().indexed());
    runtime.redo().unwrap().unwrap();
    assert!(runtime.scene().node(child).is_none());
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn irrelevant_properties_trigger_no_derived_or_spatial_work() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    let name = runtime
        .dispatch(Command::SetName {
            target: id,
            name: "Renamed".into(),
        })
        .unwrap();
    let metadata = runtime
        .dispatch(Command::SetMetadata {
            target: id,
            metadata: BTreeMap::from([("key".into(), "value".into())]),
        })
        .unwrap();
    let locked = runtime
        .dispatch(Command::SetLocked {
            target: id,
            locked: true,
        })
        .unwrap();
    for stats in [name.scene, metadata.scene, locked.scene] {
        assert_eq!(stats.world_transforms_recomputed, 0);
        assert_eq!(stats.bounds_recomputed, 0);
        assert_eq!(stats.spatial_entries_updated, 0);
        assert_eq!(stats.full_scene_rebuild_count, 0);
    }
}

#[test]
fn group_and_frame_keep_own_and_subtree_bounds_distinct() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Group"), root);
    rectangle(
        &mut runtime,
        group,
        NodeId::new(),
        Vec2::new(20.0, 10.0),
        Affine2::translation(Vec2::new(30.0, 40.0)),
    );
    let group_scene = runtime.scene().node(group).unwrap();
    assert_eq!(group_scene.own_world_bounds(), None);
    assert_eq!(
        group_scene.subtree_world_bounds(),
        Some(Rect::from_min_max(
            Vec2::new(30.0, 40.0),
            Vec2::new(50.0, 50.0)
        ))
    );
}

#[test]
fn leaf_bounds_change_does_not_scan_ten_thousand_siblings() {
    let document = build_fixture(FixtureKind::BenchB).unwrap();
    let mut runtime = EngineRuntime::new(document, Camera::default()).unwrap();
    let leaf = fixture_node_id(5_000);
    let outcome = runtime
        .dispatch(Command::SetLocalTransform {
            target: leaf,
            transform: Affine2::translation(Vec2::new(-500.0, -500.0)),
        })
        .unwrap();
    assert_eq!(outcome.scene.full_scene_rebuild_count, 0);
    assert!(outcome.scene.visited_scene_nodes < 64);
    assert_eq!(outcome.scene.world_transforms_recomputed, 1);
    assert_eq!(outcome.scene.bounds_recomputed, 1);
    assert_eq!(outcome.scene.spatial_entries_updated, 1);
}

#[test]
fn rectangle_spatial_query_uses_index_and_document_order() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let second = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let query = runtime
        .query_world_rect(Rect::from_min_max(Vec2::ZERO, Vec2::new(5.0, 5.0)))
        .unwrap();
    assert_eq!(query.ids(), &[second, first]);
    assert_eq!(query.candidate_count(), 2);
}

#[test]
fn hit_test_reports_index_candidates_and_exact_tests() {
    let document = build_fixture(FixtureKind::BenchA).unwrap();
    let mut runtime = EngineRuntime::new(document, Camera::default()).unwrap();
    let hit = hit_world(&mut runtime, Vec2::new(5.0, 5.0));
    assert_eq!(hit.topmost(), Some(fixture_node_id(1)));
    assert!(hit.candidate_count() < 64);
    assert_eq!(hit.exact_geometry_test_count(), hit.candidate_count());
}

#[test]
fn ellipse_aabb_corner_is_not_an_exact_hit() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    ellipse(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(100.0, 50.0),
        Affine2::IDENTITY,
    );
    let result = hit_world(&mut runtime, Vec2::new(1.0, 1.0));
    assert_eq!(result.candidate_count(), 1);
    assert_eq!(result.topmost(), None);
}

#[test]
fn rotated_non_uniform_negative_scale_rectangle_hits_in_local_space() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let transform = Affine2::translation(Vec2::new(100.0, 80.0))
        * Affine2::rotation(0.6)
        * Affine2::scale(Vec2::new(-2.0, 0.5));
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(40.0, 20.0),
        transform,
    );
    let world_point = transform.transform_point(Vec2::new(20.0, 10.0));
    assert_eq!(hit_world(&mut runtime, world_point).topmost(), Some(id));
}

#[test]
fn rotated_non_uniform_negative_scale_ellipse_hits_in_local_space() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let transform = Affine2::translation(Vec2::new(-100.0, 30.0))
        * Affine2::rotation(-0.4)
        * Affine2::scale(Vec2::new(-1.5, 0.75));
    let id = ellipse(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(80.0, 30.0),
        transform,
    );
    let world_point = transform.transform_point(Vec2::new(40.0, 15.0));
    assert_eq!(hit_world(&mut runtime, world_point).topmost(), Some(id));
}

#[test]
fn hidden_and_detached_nodes_are_not_hit() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let hidden = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime
        .dispatch(Command::SetVisible {
            target: hidden,
            visible: false,
        })
        .unwrap();
    assert_eq!(hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(), None);
    runtime
        .dispatch(Command::SetVisible {
            target: hidden,
            visible: true,
        })
        .unwrap();
    runtime.dispatch(Command::Detach { child: hidden }).unwrap();
    assert_eq!(hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(), None);
}

#[test]
fn locked_nodes_remain_hittable() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime
        .dispatch(Command::SetLocked {
            target: id,
            locked: true,
        })
        .unwrap();
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(),
        Some(id)
    );
}

#[test]
fn overlapping_nodes_resolve_topmost_from_child_order() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let second = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let hit = hit_world(&mut runtime, Vec2::new(5.0, 5.0));
    assert_eq!(hit.all(), &[second, first]);
}

#[test]
fn reorder_undo_redo_change_topmost_without_dense_scene_reindex() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let second = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let reorder = runtime
        .dispatch(Command::Reparent {
            child: first,
            new_parent: root,
            index: 1,
        })
        .unwrap();
    assert_eq!(reorder.scene.world_transforms_recomputed, 0);
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(),
        Some(first)
    );
    runtime.undo().unwrap().unwrap();
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(),
        Some(second)
    );
    runtime.redo().unwrap().unwrap();
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(),
        Some(first)
    );
}

#[test]
fn camera_world_viewport_round_trip_is_stable() {
    let camera = Camera::new(
        WorldPoint(Vec2::new(100.0, -50.0)),
        2.5,
        Vec2::new(1_200.0, 800.0),
        2.0,
    )
    .unwrap();
    let world = WorldPoint(Vec2::new(-500.0, 260.0));
    let viewport = camera.world_to_viewport(world).unwrap();
    let restored = camera.viewport_to_world(viewport).unwrap();
    assert!(restored.0.approx_eq(world.0, DEFAULT_EPSILON));
}

#[test]
fn camera_pan_moves_world_center_opposite_viewport_delta() {
    let mut camera = Camera::default();
    camera.pan(Vec2::new(100.0, -50.0)).unwrap();
    assert_eq!(camera.center(), WorldPoint(Vec2::new(-100.0, 50.0)));
}

#[test]
fn camera_zoom_around_pointer_preserves_world_point() {
    let mut camera = Camera::default();
    let pointer = ViewportPoint(Vec2::new(700.0, 200.0));
    let before = camera.viewport_to_world(pointer).unwrap();
    camera.zoom_around(pointer, 8.0).unwrap();
    let after = camera.viewport_to_world(pointer).unwrap();
    assert!(before.0.approx_eq(after.0, DEFAULT_EPSILON));
}

#[test]
fn camera_dpr_conversion_is_explicit() {
    let camera = Camera::new(WorldPoint(Vec2::ZERO), 1.0, Vec2::new(800.0, 600.0), 2.5).unwrap();
    assert_eq!(
        camera
            .viewport_to_device(ViewportPoint(Vec2::new(10.0, 20.0)))
            .unwrap()
            .0,
        Vec2::new(25.0, 50.0)
    );
}

#[test]
fn camera_fit_bounds_respects_padding() {
    let mut camera =
        Camera::new(WorldPoint(Vec2::ZERO), 1.0, Vec2::new(1_000.0, 500.0), 1.0).unwrap();
    camera
        .fit_world_bounds(
            Rect::from_min_max(Vec2::new(100.0, 50.0), Vec2::new(500.0, 250.0)),
            50.0,
        )
        .unwrap();
    assert_eq!(camera.center(), WorldPoint(Vec2::new(300.0, 150.0)));
    assert!((camera.zoom() - 2.0).abs() < DEFAULT_EPSILON);
}

#[test]
fn invalid_camera_input_and_non_finite_results_are_typed() {
    assert_eq!(
        Camera::new(WorldPoint(Vec2::ZERO), 0.0, Vec2::new(100.0, 100.0), 1.0),
        Err(CameraError::InvalidZoom)
    );
    let camera = Camera::new(WorldPoint(Vec2::ZERO), 2.0, Vec2::new(1_024.0, 768.0), 1.0).unwrap();
    assert_eq!(
        camera.world_to_viewport(WorldPoint(Vec2::new(f64::MAX, f64::MAX))),
        Err(CameraError::NonFiniteResult)
    );
}

#[test]
fn camera_changes_do_not_mutate_document_or_history() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let snapshot = runtime.document().snapshot();
    let history = runtime.history_state();
    runtime.camera_mut().pan(Vec2::new(200.0, 100.0)).unwrap();
    runtime
        .camera_mut()
        .zoom_around(ViewportPoint(Vec2::new(50.0, 50.0)), 4.0)
        .unwrap();
    assert_eq!(runtime.document().snapshot(), snapshot);
    assert_eq!(runtime.history_state(), history);
}

#[test]
fn singular_zero_size_and_extreme_transforms_are_non_hittable_without_panic() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let singular = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::scale(Vec2::new(0.0, 1.0)),
    );
    let zero = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(0.0, 20.0),
        Affine2::translation(Vec2::new(100.0, 0.0)),
    );
    let extreme = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::translation(Vec2::new(f64::MAX / 2.0, f64::MAX / 2.0)),
    );
    for id in [singular, zero, extreme] {
        assert!(!runtime.scene().node(id).unwrap().indexed());
    }
    assert_eq!(hit_world(&mut runtime, Vec2::ZERO).topmost(), None);
}

#[test]
fn non_finite_derived_world_is_recorded_and_excluded_from_spatial_index() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let mut group_spec = NodeSpec::group(NodeId::new(), "Huge");
    group_spec.local_transform = Affine2::scale(Vec2::new(1.0e308, 1.0e308));
    let group = create_node(&mut runtime, group_spec, root);
    let child = rectangle(
        &mut runtime,
        group,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::scale(Vec2::new(1.0e308, 1.0e308)),
    );
    let scene_node = runtime.scene().node(child).unwrap();
    assert_eq!(
        scene_node.invalid(),
        Some(InvalidDerivedState::NonFiniteWorldTransform)
    );
    assert!(!scene_node.indexed());
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn bench_b_dirty_proof_meets_structural_thresholds() {
    let document = build_fixture(FixtureKind::BenchB).unwrap();
    let mut runtime = EngineRuntime::new(document, Camera::default()).unwrap();
    let outcome = runtime
        .dispatch(Command::SetLocalTransform {
            target: fixture_node_id(5_000),
            transform: Affine2::translation(Vec2::new(-1_000.0, -1_000.0)),
        })
        .unwrap();
    assert_eq!(outcome.scene.full_scene_rebuild_count, 0);
    assert!(outcome.scene.visited_scene_nodes < 64);
    assert!(outcome.scene.world_transforms_recomputed < 64);
    assert_eq!(outcome.scene.spatial_entries_updated, 1);
}

#[test]
fn bench_c_spatial_candidate_proof_avoids_full_scan() {
    let document = build_fixture(FixtureKind::BenchC).unwrap();
    let mut runtime = EngineRuntime::new(document, Camera::default()).unwrap();
    let result = hit_world(&mut runtime, Vec2::new(5.0, 5.0));
    assert!(result.candidate_count() <= 64);
    assert!(result.exact_geometry_test_count() <= 64);
    assert_eq!(result.topmost(), Some(fixture_node_id(1)));
}

#[test]
fn bench_d_deep_hierarchy_build_is_iterative() {
    let document = build_fixture(FixtureKind::BenchD).unwrap();
    let scene = ComputedScene::build(&document, 0).unwrap();
    assert_eq!(scene.len(), 10_001);
    assert_eq!(scene.metrics().last_update.visited_scene_nodes, 10_001);
    assert_eq!(scene.metrics().last_update.full_scene_rebuild_count, 1);
}

#[test]
fn runtime_scene_camera_and_metrics_are_not_serialized() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    runtime.camera_mut().pan(Vec2::new(10.0, 20.0)).unwrap();
    let json = visual_authoring_serialization::to_json_pretty(runtime.document()).unwrap();
    for forbidden in [
        "scene_revision",
        "spatial",
        "camera",
        "device_pixel_ratio",
        "metrics",
        "history",
        "selection",
    ] {
        assert!(!json.contains(forbidden));
    }
}

#[test]
fn appearance_dirty_is_tracked_without_transform_or_bounds_recompute() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    let outcome = runtime
        .dispatch(Command::SetAppearance {
            target: id,
            appearance: Appearance {
                opacity: 0.5,
                ..Appearance::default()
            },
        })
        .unwrap();
    assert_eq!(outcome.scene.dirty_nodes, 1);
    assert_eq!(outcome.scene.world_transforms_recomputed, 0);
    assert_eq!(outcome.scene.bounds_recomputed, 0);
    assert_eq!(outcome.scene.spatial_entries_updated, 0);
}

#[test]
fn nested_invisible_ancestor_blocks_hits_until_restored() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let outer = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Outer"), root);
    let inner = create_node(&mut runtime, NodeSpec::group(NodeId::new(), "Inner"), outer);
    let leaf = rectangle(
        &mut runtime,
        inner,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime
        .dispatch(Command::SetVisible {
            target: outer,
            visible: false,
        })
        .unwrap();
    assert_eq!(hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(), None);
    runtime.undo().unwrap().unwrap();
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).topmost(),
        Some(leaf)
    );
}

#[test]
fn metadata_type_remains_persistent_only() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    let mut metadata = Metadata::new();
    metadata.insert("role".into(), "proof".into());
    let outcome = runtime
        .dispatch(Command::SetMetadata {
            target: id,
            metadata,
        })
        .unwrap();
    assert_eq!(
        outcome.scene,
        visual_authoring_scene::SceneUpdateStats {
            document_revision: runtime.document_revision(),
            scene_revision: runtime.scene().revision(),
            ..visual_authoring_scene::SceneUpdateStats::default()
        }
    );
}

#[test]
fn camera_resize_and_dpr_validation_preserve_prior_state_on_error() {
    let mut camera = Camera::default();
    let before = camera.clone();
    assert_eq!(
        camera.resize_viewport(Vec2::new(0.0, 100.0)),
        Err(CameraError::InvalidViewport)
    );
    assert_eq!(camera, before);
    assert_eq!(
        camera.set_device_pixel_ratio(f64::NAN),
        Err(CameraError::InvalidDevicePixelRatio)
    );
    assert_eq!(camera, before);
}

#[test]
fn replacement_with_detached_subtree_keeps_records_but_not_index_entries() {
    let mut headless = visual_authoring_document::HeadlessEditorCore::blank("Replacement");
    let detached = NodeId::new();
    headless
        .dispatch(Command::RegisterNode {
            spec: NodeSpec::rectangle(detached, "Detached", Vec2::new(10.0, 10.0)),
        })
        .unwrap();
    let replacement = headless.into_document().unwrap();
    let mut runtime = EngineRuntime::blank("Original").unwrap();
    runtime.replace_document(replacement).unwrap();
    let record = runtime.scene().node(detached).unwrap();
    assert!(!record.attached());
    assert!(!record.indexed());
}

#[test]
fn world_preserving_reparent_stays_synchronized() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let mut left_spec = NodeSpec::group(NodeId::new(), "Left");
    left_spec.local_transform = Affine2::translation(Vec2::new(100.0, 0.0));
    let left = create_node(&mut runtime, left_spec, root);
    let mut right_spec = NodeSpec::group(NodeId::new(), "Right");
    right_spec.local_transform = Affine2::rotation(0.5);
    let right = create_node(&mut runtime, right_spec, root);
    let child = rectangle(
        &mut runtime,
        left,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::translation(Vec2::new(25.0, 10.0)),
    );
    let before = runtime
        .scene()
        .node(child)
        .unwrap()
        .world_transform()
        .unwrap();
    runtime
        .dispatch(Command::ReparentPreservingWorld {
            child,
            new_parent: right,
            index: 0,
        })
        .unwrap();
    let after = runtime
        .scene()
        .node(child)
        .unwrap()
        .world_transform()
        .unwrap();
    assert!(before.approx_eq(after, DEFAULT_EPSILON));
    assert_scene_matches_fresh(&runtime);
}

#[test]
fn invalid_fit_bounds_are_rejected_atomically() {
    let mut camera = Camera::default();
    let before = camera.clone();
    assert_eq!(
        camera.fit_world_bounds(Rect::from_size(Vec2::ZERO), 10.0),
        Err(CameraError::InvalidFitBounds)
    );
    assert_eq!(camera, before);
}

#[test]
fn scene_metrics_revisions_match_after_every_successful_mutation() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(10.0, 10.0),
        Affine2::IDENTITY,
    );
    for x in 1..=8 {
        runtime
            .dispatch(Command::SetLocalTransform {
                target: id,
                transform: Affine2::translation(Vec2::new(x as f64, 0.0)),
            })
            .unwrap();
        assert_eq!(runtime.document_revision(), runtime.scene().revision());
        assert_eq!(
            runtime.scene().metrics().last_update.document_revision,
            runtime.document_revision()
        );
    }
}

#[test]
fn full_document_replacement_does_not_use_incremental_fallback() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let replacement = Document::new("Replacement");
    let outcome = runtime.replace_document(replacement).unwrap();
    assert_eq!(outcome.scene.full_scene_rebuild_count, 1);
    assert_eq!(outcome.scene.fallback_rebuild_count, 0);
}
#[test]
fn revision_exhaustion_preserves_document_scene_render_history_and_selection() {
    let mut runtime = EngineRuntime::blank("revision").unwrap();
    let root = runtime.document().root_id();
    let node = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime.select_only(node).unwrap();
    runtime.set_revision_for_test(u64::MAX).unwrap();

    let document = runtime.document().snapshot();
    let scene = runtime.scene().semantic_snapshot();
    let render = runtime.render_model().items().cloned().collect::<Vec<_>>();
    let history = runtime.history_state();
    let selection = runtime.selection().clone();

    assert_eq!(
        runtime.dispatch(Command::SetLocalTransform {
            target: node,
            transform: Affine2::translation(Vec2::new(5.0, 5.0)),
        }),
        Err(RuntimeError::Editor(EditorError::RevisionExhausted {
            revision: u64::MAX,
        }))
    );
    assert_eq!(runtime.document().snapshot(), document);
    assert_eq!(runtime.scene().semantic_snapshot(), scene);
    assert_eq!(
        runtime.render_model().items().cloned().collect::<Vec<_>>(),
        render
    );
    assert_eq!(runtime.history_state(), history);
    assert_eq!(runtime.selection(), &selection);
    assert_eq!(runtime.document_revision(), u64::MAX);
    assert_eq!(runtime.scene().revision(), u64::MAX);
    assert_eq!(runtime.render_model().revision(), u64::MAX);
}

#[test]
fn failed_rollback_at_revision_limit_keeps_preview_scene_and_render_intact() {
    let mut runtime = EngineRuntime::blank("transaction revision").unwrap();
    let root = runtime.document().root_id();
    let node = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    runtime.begin_transaction().unwrap();
    runtime.set_revision_for_test(u64::MAX - 1).unwrap();
    let preview = runtime
        .update_transaction(Command::SetLocalTransform {
            target: node,
            transform: Affine2::translation(Vec2::new(12.0, 8.0)),
        })
        .unwrap();
    assert_eq!(preview.render.dirty_slots.len(), 1);
    assert_eq!(runtime.document_revision(), u64::MAX);

    let document = runtime.document().snapshot();
    let scene = runtime.scene().semantic_snapshot();
    let render = runtime.render_model().items().cloned().collect::<Vec<_>>();
    let history = runtime.history_state();
    assert_eq!(
        runtime.rollback_transaction(),
        Err(RuntimeError::Editor(EditorError::RevisionExhausted {
            revision: u64::MAX,
        }))
    );
    assert_eq!(runtime.document().snapshot(), document);
    assert_eq!(runtime.scene().semantic_snapshot(), scene);
    assert_eq!(
        runtime.render_model().items().cloned().collect::<Vec<_>>(),
        render
    );
    assert_eq!(runtime.history_state(), history);
    assert!(runtime.transaction_active());
}

#[test]
fn camera_culling_changes_visible_set_without_document_scene_or_render_revision() {
    let mut runtime = EngineRuntime::blank("culling").unwrap();
    let root = runtime.document().root_id();
    let near = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let far = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::translation(Vec2::new(10_000.0, 10_000.0)),
    );
    let revisions = (
        runtime.document_revision(),
        runtime.scene().revision(),
        runtime.render_model().revision(),
    );

    let near_view = runtime.cull_viewport().unwrap();
    assert_eq!(near_view.ids_top_to_bottom, vec![near]);
    assert_eq!(near_view.spatial_candidates, 1);
    assert_eq!(near_view.submitted_instances, 1);
    assert_eq!(near_view.culled, 1);

    runtime
        .camera_mut()
        .pan(Vec2::new(-10_000.0, -10_000.0))
        .unwrap();
    let far_view = runtime.cull_viewport().unwrap();
    assert_eq!(far_view.ids_top_to_bottom, vec![far]);
    assert_eq!(
        (
            runtime.document_revision(),
            runtime.scene().revision(),
            runtime.render_model().revision(),
        ),
        revisions
    );
}

#[test]
fn render_delta_is_incremental_and_replacement_is_an_explicit_full_rebuild() {
    let mut runtime = EngineRuntime::blank("render delta").unwrap();
    let root = runtime.document().root_id();
    let node = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(20.0, 20.0),
        Affine2::IDENTITY,
    );
    let outcome = runtime
        .dispatch(Command::SetLocalTransform {
            target: node,
            transform: Affine2::translation(Vec2::new(30.0, 40.0)),
        })
        .unwrap();
    assert_eq!(outcome.render.dirty_slots.len(), 1);
    assert_eq!(outcome.render.stats.full_render_rebuild_count, 0);
    assert_eq!(outcome.render_sync, RenderSyncStatus::Incremental);

    let replacement = runtime
        .replace_document(Document::new("replacement"))
        .unwrap();
    assert_eq!(replacement.render.stats.full_render_rebuild_count, 1);
    assert_eq!(replacement.render_sync, RenderSyncStatus::FullDocumentReset);
}

#[test]
fn eight_overlapping_siblings_keep_exact_order_and_hit_test_across_reorder_undo_redo() {
    let mut runtime = EngineRuntime::blank("R2 exact order").unwrap();
    let root = runtime.document().root_id();
    let siblings = (0..8)
        .map(|_| {
            rectangle(
                &mut runtime,
                root,
                NodeId::new(),
                Vec2::new(20.0, 20.0),
                Affine2::IDENTITY,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).all(),
        siblings.iter().rev().copied().collect::<Vec<_>>()
    );

    let reordered = runtime
        .dispatch(Command::Reparent {
            child: siblings[0],
            new_parent: root,
            index: 7,
        })
        .unwrap();
    assert_eq!(reordered.command.sequence_work().full_sequence_scans, 0);
    assert_eq!(reordered.command.sequence_work().full_sequence_copies, 0);
    assert_eq!(reordered.command.sequence_work().dense_index_rewrites, 0);
    assert_eq!(reordered.scene.sequence_work.full_sequence_scans, 0);
    assert_eq!(reordered.scene.sequence_work.full_sequence_copies, 0);
    assert_eq!(reordered.scene.sequence_work.dense_index_rewrites, 0);
    let mut expected = siblings[1..].to_vec();
    expected.push(siblings[0]);
    assert_eq!(runtime.document().node(root).unwrap().children(), expected);
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).all(),
        expected.iter().rev().copied().collect::<Vec<_>>()
    );

    runtime.undo().unwrap().unwrap();
    assert_eq!(runtime.document().node(root).unwrap().children(), siblings);
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).all(),
        siblings.iter().rev().copied().collect::<Vec<_>>()
    );
    runtime.redo().unwrap().unwrap();
    assert_eq!(runtime.document().node(root).unwrap().children(), expected);
    assert_eq!(
        hit_world(&mut runtime, Vec2::new(5.0, 5.0)).all(),
        expected.iter().rev().copied().collect::<Vec<_>>()
    );
}
#[test]
fn centered_stroke_expands_bounds_and_undo_restores_them_incrementally() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let id = rectangle(
        &mut runtime,
        root,
        NodeId::new(),
        Vec2::new(100.0, 50.0),
        Affine2::translation(Vec2::new(10.0, 20.0)),
    );
    let before = runtime
        .scene()
        .node(id)
        .unwrap()
        .own_world_bounds()
        .unwrap();
    let outcome = runtime
        .dispatch(Command::SetAppearance {
            target: id,
            appearance: Appearance {
                stroke: visual_authoring_document::Stroke {
                    width: 10.0,
                    ..visual_authoring_document::Stroke::default()
                },
                ..Appearance::default()
            },
        })
        .unwrap();
    let after = runtime
        .scene()
        .node(id)
        .unwrap()
        .own_world_bounds()
        .unwrap();

    assert_eq!(outcome.scene.bounds_recomputed, 1);
    assert_eq!(
        outcome.render.dirty_slots,
        vec![runtime.render_model().item(id).unwrap().slot]
    );
    assert_eq!(after.min, Vec2::new(5.0, 15.0));
    assert_eq!(after.max, Vec2::new(115.0, 75.0));

    runtime.undo().unwrap().unwrap();
    assert_eq!(
        runtime.scene().node(id).unwrap().own_world_bounds(),
        Some(before)
    );
}

#[test]
fn asymmetric_affine_ellipse_stroke_bounds_and_hit_test_share_semantics() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let size = Vec2::new(160.0, 32.0);
    let transform = Affine2::from_components(0.9, 0.35, -0.2, 1.1, 30.0, 40.0);
    let id = ellipse(&mut runtime, root, NodeId::new(), size, transform);
    runtime
        .dispatch(Command::SetAppearance {
            target: id,
            appearance: Appearance {
                stroke: visual_authoring_document::Stroke {
                    width: 8.0,
                    ..visual_authoring_document::Stroke::default()
                },
                ..Appearance::default()
            },
        })
        .unwrap();

    let major_inside = transform.transform_point(Vec2::new(size.x + 3.5, size.y * 0.5));
    let minor_inside = transform.transform_point(Vec2::new(size.x * 0.5, -3.5));
    let major_outside = transform.transform_point(Vec2::new(size.x + 5.0, size.y * 0.5));
    let minor_outside = transform.transform_point(Vec2::new(size.x * 0.5, -5.0));
    assert_eq!(hit_world(&mut runtime, major_inside).topmost(), Some(id));
    assert_eq!(hit_world(&mut runtime, minor_inside).topmost(), Some(id));
    assert_ne!(hit_world(&mut runtime, major_outside).topmost(), Some(id));
    assert_ne!(hit_world(&mut runtime, minor_outside).topmost(), Some(id));

    let semantic_local = Vec2::new(130.0, 8.0);
    let semantic_world = transform.transform_point(semantic_local);
    let transposed = Affine2::from_components(
        transform.m11,
        transform.m21,
        transform.m12,
        transform.m22,
        transform.tx,
        transform.ty,
    );
    let transposed_only_world = transposed.transform_point(semantic_local);
    assert_eq!(hit_world(&mut runtime, semantic_world).topmost(), Some(id));
    assert_ne!(
        hit_world(&mut runtime, transposed_only_world).topmost(),
        Some(id)
    );

    let bounds = runtime
        .scene()
        .node(id)
        .unwrap()
        .own_world_bounds()
        .unwrap();
    for point in [major_inside, minor_inside] {
        assert!(point.x >= bounds.min.x && point.x <= bounds.max.x);
        assert!(point.y >= bounds.min.y && point.y <= bounds.max.y);
    }
}
