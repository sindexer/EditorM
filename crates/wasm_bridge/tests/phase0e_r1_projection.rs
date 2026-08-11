use serde_json::{json, Value};
use visual_authoring_runtime::fixtures::fixture_node_id;
use visual_authoring_wasm_bridge::EngineHost;

fn send(host: &mut EngineHost, request_id: &str, operation: Value) -> Value {
    let mut envelope = operation;
    envelope["protocol_version"] = json!(1);
    envelope["request_id"] = json!(request_id);
    serde_json::from_str(&host.handle_json(&envelope.to_string())).expect("valid response JSON")
}

#[test]
fn hundred_k_group_ungroup_projection_is_versioned_bounded_and_fallback_free() {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load-100k",
        json!({ "type": "load_fixture", "fixture": "bench-c" }),
    );
    assert_eq!(loaded["ok"], true);
    assert_eq!(loaded["projection"]["schema_version"], 2);
    let root = fixture_node_id(0).to_string();
    let targets = vec![
        fixture_node_id(1).to_string(),
        fixture_node_id(50_001).to_string(),
        fixture_node_id(100_000).to_string(),
    ];
    let group = "00000000-0000-0000-e100-000000000001";
    let before_revision = loaded["revisions"]["document"].as_u64().unwrap();

    let grouped = send(
        &mut host,
        "group-100k",
        json!({
            "type": "command",
            "command": { "kind": "group", "group_id": group, "name": "R1 group", "targets": targets }
        }),
    );
    assert_eq!(grouped["ok"], true);
    assert_eq!(grouped["revisions"]["document"], before_revision + 1);
    assert_eq!(grouped["revisions"]["scene"], before_revision + 1);
    assert_eq!(grouped["revisions"]["render"], before_revision + 1);
    assert_eq!(grouped["history"]["undo_depth"], 1);
    assert_eq!(grouped["metrics"]["fallback_rebuild_count"], 0);
    assert_eq!(grouped["metrics"]["document_full_clones"], 0);
    assert_eq!(grouped["metrics"]["document_nodes_touched"], 5);
    assert_eq!(grouped["metrics"]["scene_nodes_visited"], 8);
    assert_eq!(grouped["metrics"]["render_items_cloned"], 0);
    assert_eq!(grouped["metrics"]["ui_full_snapshots"], 0);
    assert_eq!(grouped["metrics"]["ui_nodes_serialized"], 1);
    assert_eq!(grouped["metrics"]["ui_child_ids_serialized"], 6);
    assert_eq!(grouped["metrics"]["ui_structural_operations"], 1);
    assert!(
        grouped["metrics"]["ui_projection_payload_bytes"]
            .as_u64()
            .unwrap()
            < 8_192
    );
    assert_eq!(grouped["render_delta"]["dirty_slots"], 0);
    assert_eq!(grouped["render_delta"]["upload_bytes"], 0);
    assert_eq!(grouped["render_delta"]["scene_full_rebuilds"], 0);
    assert_eq!(grouped["render_delta"]["render_full_rebuilds"], 0);
    assert_eq!(grouped["projection"]["full"], false);
    assert_eq!(
        grouped["projection"]["structural_ops"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(grouped["projection"]["structural_ops"][0]["kind"], "group");
    assert!(grouped["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|node| node["id"] != root));

    let ungrouped = send(
        &mut host,
        "ungroup-100k",
        json!({
            "type": "command",
            "command": { "kind": "ungroup", "node_id": group }
        }),
    );
    assert_eq!(ungrouped["ok"], true);
    assert_eq!(ungrouped["revisions"]["document"], before_revision + 2);
    assert_eq!(ungrouped["history"]["undo_depth"], 2);
    assert_eq!(ungrouped["metrics"]["fallback_rebuild_count"], 0);
    assert_eq!(ungrouped["metrics"]["document_full_clones"], 0);
    assert_eq!(ungrouped["metrics"]["scene_nodes_visited"], 6);
    assert_eq!(ungrouped["metrics"]["render_items_cloned"], 0);
    assert_eq!(ungrouped["metrics"]["ui_full_snapshots"], 0);
    assert_eq!(ungrouped["metrics"]["ui_nodes_serialized"], 0);
    assert_eq!(ungrouped["metrics"]["ui_child_ids_serialized"], 3);
    assert_eq!(ungrouped["metrics"]["ui_structural_operations"], 1);
    assert!(
        ungrouped["metrics"]["ui_projection_payload_bytes"]
            .as_u64()
            .unwrap()
            < 8_192
    );
    assert_eq!(ungrouped["render_delta"]["dirty_slots"], 0);
    assert_eq!(ungrouped["render_delta"]["upload_bytes"], 0);
    assert_eq!(
        ungrouped["projection"]["structural_ops"][0]["kind"],
        "ungroup"
    );
    assert_eq!(ungrouped["projection"]["removed"][0], group);
}
