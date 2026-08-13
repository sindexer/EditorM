use serde_json::json;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::{Camera, EngineRuntime, WorldPoint};
use visual_authoring_serialization::{from_json, to_json_pretty};

fn rectangle(runtime: &mut EngineRuntime, root: NodeId, name: &str) -> NodeId {
    let id = NodeId::new();
    let index = runtime.document().node(root).unwrap().children().len();
    let mut spec = NodeSpec::rectangle(id, name, Vec2::new(20.0, 20.0));
    spec.local_transform = Affine2::IDENTITY;
    runtime
        .dispatch(Command::CreateNode {
            spec,
            parent: root,
            index,
        })
        .unwrap();
    id
}

fn hit(runtime: &mut EngineRuntime) -> Vec<NodeId> {
    let viewport = runtime
        .camera()
        .world_to_viewport(WorldPoint(Vec2::new(5.0, 5.0)))
        .unwrap();
    runtime.hit_test_viewport(viewport).unwrap().all().to_vec()
}

fn assert_scene_ranks_match_document(runtime: &EngineRuntime, parent: NodeId) {
    let snapshots = runtime.scene().semantic_snapshot();
    let children = runtime.document().node(parent).unwrap().children();
    for (rank, child) in children.iter().copied().enumerate() {
        assert_eq!(snapshots[&child].sibling_index, rank, "rank for {child}");
    }
}

#[test]
fn eight_overlapping_siblings_match_document_scene_hit_order_and_persistence() {
    let mut runtime = EngineRuntime::blank("R2 order accuracy").unwrap();
    let root = runtime.document().root_id();
    let siblings = (0..8)
        .map(|index| rectangle(&mut runtime, root, &format!("Sibling {index}")))
        .collect::<Vec<_>>();
    let original_hit = siblings.iter().rev().copied().collect::<Vec<_>>();
    assert_eq!(hit(&mut runtime), original_hit);
    assert_scene_ranks_match_document(&runtime, root);

    let group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Noncontiguous"),
            targets: vec![siblings[0], siblings[3], siblings[7]],
        })
        .unwrap();
    let grouped_root = vec![
        group,
        siblings[1],
        siblings[2],
        siblings[4],
        siblings[5],
        siblings[6],
    ];
    assert_eq!(
        runtime.document().node(root).unwrap().children(),
        grouped_root
    );
    let grouped_hit = vec![
        siblings[6],
        siblings[5],
        siblings[4],
        siblings[2],
        siblings[1],
        siblings[7],
        siblings[3],
        siblings[0],
    ];
    assert_eq!(hit(&mut runtime), grouped_hit);
    assert_scene_ranks_match_document(&runtime, root);
    assert_scene_ranks_match_document(&runtime, group);

    let encoded = to_json_pretty(runtime.document()).unwrap();
    let loaded = from_json(&encoded).unwrap();
    let mut loaded_runtime = EngineRuntime::new(loaded, Camera::default()).unwrap();
    assert_eq!(hit(&mut loaded_runtime), grouped_hit);
    assert_scene_ranks_match_document(&loaded_runtime, root);
    assert_scene_ranks_match_document(&loaded_runtime, group);
    loaded_runtime
        .dispatch(Command::Ungroup { target: group })
        .unwrap();
    assert_eq!(
        loaded_runtime.document().node(root).unwrap().children(),
        siblings
    );
    assert_eq!(hit(&mut loaded_runtime), original_hit);

    runtime
        .dispatch(Command::Ungroup { target: group })
        .unwrap();
    assert_eq!(runtime.document().node(root).unwrap().children(), siblings);
    assert_eq!(hit(&mut runtime), original_hit);
    runtime.undo().unwrap().unwrap();
    assert_eq!(
        runtime.document().node(root).unwrap().children(),
        grouped_root
    );
    assert_eq!(hit(&mut runtime), grouped_hit);
    runtime.redo().unwrap().unwrap();
    assert_eq!(runtime.document().node(root).unwrap().children(), siblings);
    assert_eq!(hit(&mut runtime), original_hit);

    let edited_group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(edited_group, "Edited noncontiguous"),
            targets: vec![siblings[0], siblings[3], siblings[7]],
        })
        .unwrap();
    let inserted = rectangle(&mut runtime, root, "Inserted front");
    runtime
        .dispatch(Command::Reparent {
            child: inserted,
            new_parent: root,
            index: 0,
        })
        .unwrap();
    runtime
        .dispatch(Command::DeleteSubtree {
            target: siblings[2],
        })
        .unwrap();
    runtime
        .dispatch(Command::Reparent {
            child: siblings[6],
            new_parent: root,
            index: 1,
        })
        .unwrap();
    runtime
        .dispatch(Command::Ungroup {
            target: edited_group,
        })
        .unwrap();
    let deterministic = vec![
        inserted,
        siblings[6],
        siblings[0],
        siblings[1],
        siblings[3],
        siblings[7],
        siblings[4],
        siblings[5],
    ];
    assert_eq!(
        runtime.document().node(root).unwrap().children(),
        deterministic
    );
    assert_eq!(
        hit(&mut runtime),
        deterministic.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_scene_ranks_match_document(&runtime, root);

    let final_scene = runtime.scene().semantic_snapshot();
    let final_scene_ranks = runtime
        .document()
        .node(root)
        .unwrap()
        .children()
        .iter()
        .copied()
        .map(|id| (id, final_scene[&id].sibling_index))
        .collect::<Vec<_>>();
    let loaded_restored = loaded_runtime
        .document()
        .node(root)
        .unwrap()
        .children()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    let report = json!({
        "phase": "0E-R2",
        "overlapping_sibling_count": 8,
        "targets": [siblings[0], siblings[3], siblings[7]],
        "document_semantic_orders": {
            "original": siblings,
            "grouped_root": grouped_root,
            "persisted_then_ungrouped": loaded_restored,
            "edited_then_ungrouped": deterministic,
        },
        "hit_test_topmost_orders": {
            "original": original_hit,
            "grouped": grouped_hit,
            "edited_then_ungrouped": hit(&mut runtime),
        },
        "final_scene_ranks": final_scene_ranks,
        "undo_redo_verified": true,
        "save_load_verified": true,
        "stale_dense_index_present": false,
        "all_passed": true,
    });
    println!("PHASE0E_R2_ORDER_JSON={report}");
}
