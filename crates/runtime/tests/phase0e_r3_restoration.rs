use serde_json::{json, Value};
use visual_authoring_core_math::Vec2;
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::{Camera, EngineRuntime};
use visual_authoring_serialization::{from_json, to_json_pretty};

fn siblings(runtime: &mut EngineRuntime, count: usize) -> (NodeId, Vec<NodeId>) {
    let root = runtime.document().root_id();
    let mut result = Vec::new();
    for index in 0..count {
        let id = NodeId::new();
        runtime
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(id, format!("Sibling {index}"), Vec2::new(8.0, 8.0)),
                parent: root,
                index,
            })
            .expect("create sibling");
        result.push(id);
    }
    (root, result)
}

#[test]
fn restoration_v2_defines_internal_reorder_reparent_and_missing_anchor_fallback() {
    let mut runtime = EngineRuntime::blank("R3 current order within runs").expect("runtime");
    let (root, ids) = siblings(&mut runtime, 6);
    let group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Reordered group"),
            targets: vec![ids[1], ids[2], ids[4]],
        })
        .expect("group");
    runtime
        .dispatch(Command::Reparent {
            child: ids[2],
            new_parent: group,
            index: 0,
        })
        .expect("internal reorder");
    runtime
        .dispatch(Command::Ungroup { target: group })
        .expect("ungroup reordered");
    assert_eq!(
        runtime.document().node(root).unwrap().children(),
        &[ids[0], ids[2], ids[1], ids[3], ids[4], ids[5]]
    );

    let mut runtime = EngineRuntime::blank("R3 group reparent").expect("runtime");
    let (root, ids) = siblings(&mut runtime, 5);
    let container = NodeId::new();
    runtime
        .dispatch(Command::CreateNode {
            spec: NodeSpec::group(container, "Container"),
            parent: root,
            index: 5,
        })
        .expect("container");
    let group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Moved group"),
            targets: vec![ids[1], ids[2], ids[4]],
        })
        .expect("group");
    runtime
        .dispatch(Command::Reparent {
            child: group,
            new_parent: container,
            index: 0,
        })
        .expect("reparent group");
    runtime
        .dispatch(Command::Ungroup { target: group })
        .expect("ungroup after reparent");
    assert_eq!(
        runtime.document().node(container).unwrap().children(),
        &[ids[1], ids[2], ids[4]]
    );

    let mut runtime = EngineRuntime::blank("R3 deleted anchor fallback").expect("runtime");
    let (root, ids) = siblings(&mut runtime, 6);
    let group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Missing anchors"),
            targets: vec![ids[1], ids[3]],
        })
        .expect("group");
    for target in [ids[0], ids[2], ids[4]] {
        runtime
            .dispatch(Command::DeleteSubtree { target })
            .expect("delete anchor");
    }
    runtime
        .dispatch(Command::Ungroup { target: group })
        .expect("fallback ungroup");
    assert_eq!(
        runtime.document().node(root).unwrap().children(),
        &[ids[1], ids[3], ids[5]]
    );

    println!(
        "PHASE0E_R3_RESTORATION_JSON={}",
        json!({
            "schema_version": 2,
            "internal_reorder_policy": "current order inside each original run; original run order across runs",
            "group_reparent_policy": "missing anchors restore at the current group rank",
            "deleted_anchor_policy": "surviving anchor, otherwise current group rank",
            "all_passed": true
        })
    );
}

#[test]
fn version_one_restoration_migrates_to_v2_and_restores_exact_order() {
    let mut runtime = EngineRuntime::blank("R3 v1 migration").expect("runtime");
    let (root, ids) = siblings(&mut runtime, 6);
    let group = NodeId::new();
    runtime
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Legacy restoration"),
            targets: vec![ids[1], ids[4]],
        })
        .expect("group");
    let mut stored: Value =
        serde_json::from_str(&to_json_pretty(runtime.document()).unwrap()).expect("stored JSON");
    let nodes = stored["document"]["nodes"]
        .as_array_mut()
        .expect("nodes array");
    let group_node = nodes
        .iter_mut()
        .find(|node| node["id"] == group.to_string())
        .expect("group record");
    let restoration = group_node["internal_group_restoration"]
        .as_object_mut()
        .expect("restoration object");
    let runs = restoration
        .remove("runs")
        .expect("v2 runs")
        .as_array()
        .expect("runs array")
        .clone();
    let mut entries = Vec::new();
    for run in runs {
        for child in run["children"].as_array().expect("run children") {
            entries.push(json!({
                "child": child,
                "before_anchor": run["before_anchor"],
                "after_anchor": run["after_anchor"]
            }));
        }
    }
    restoration.insert("version".to_owned(), json!(1));
    restoration.insert("entries".to_owned(), Value::Array(entries));

    let migrated = from_json(&serde_json::to_string(&stored).unwrap()).expect("v1 migration");
    assert_eq!(
        migrated
            .node(group)
            .unwrap()
            .group_restoration()
            .unwrap()
            .version,
        2
    );
    let mut loaded = EngineRuntime::new(migrated, Camera::default()).expect("loaded runtime");
    loaded
        .dispatch(Command::Ungroup { target: group })
        .expect("ungroup migrated document");
    assert_eq!(loaded.document().node(root).unwrap().children(), ids);
}
