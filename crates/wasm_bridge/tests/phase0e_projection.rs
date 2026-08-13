use serde_json::{json, Value};
use visual_authoring_wasm_bridge::EngineHost;

fn send(host: &mut EngineHost, request_id: &str, operation: Value) -> Value {
    let mut envelope = operation;
    envelope["protocol_version"] = json!(1);
    envelope["request_id"] = json!(request_id);
    serde_json::from_str(&host.handle_json(&envelope.to_string())).expect("valid response JSON")
}

#[test]
fn stable_node_id_command_emits_one_node_ui_delta_and_selection_is_ephemeral() {
    let mut host = EngineHost::new().expect("host");
    let initialized = send(&mut host, "init", json!({ "type": "initialize" }));
    assert_eq!(initialized["ok"], true);
    assert_eq!(initialized["projection"]["full"], true);
    assert_eq!(
        initialized["projection"]["upserts"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let rectangle = initialized["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "rectangle")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let revisions = initialized["revisions"].clone();
    let selected = send(
        &mut host,
        "select",
        json!({ "type": "selection", "target": rectangle, "mode": "replace" }),
    );
    assert_eq!(selected["ok"], true);
    assert_eq!(selected["revisions"], revisions);
    assert_eq!(selected["selection"]["primary"], rectangle);
    assert_eq!(selected["projection"]["full"], false);
    assert!(selected["projection"]["upserts"]
        .as_array()
        .unwrap()
        .is_empty());

    let updated = send(
        &mut host,
        "transform",
        json!({
            "type": "command",
            "command": {
                "kind": "set_transform",
                "node_id": rectangle,
                "matrix": [1.0, 0.0, 0.0, 1.0, 12.0, 18.0]
            }
        }),
    );
    assert_eq!(updated["ok"], true);
    assert_eq!(updated["projection"]["full"], false);
    assert_eq!(
        updated["projection"]["upserts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(updated["metrics"]["ui_full_snapshots"], 0);
    assert_eq!(updated["metrics"]["ui_nodes_serialized"], 1);
    assert_eq!(updated["render_delta"]["dirty_slots"], 1);
    assert_eq!(updated["render_delta"]["render_full_rebuilds"], 0);
}

#[test]
fn stable_id_group_and_ungroup_are_atomic_protocol_commands() {
    let mut host = EngineHost::new().expect("host");
    let initialized = send(&mut host, "init", json!({ "type": "initialize" }));
    let shapes = initialized["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["kind"] == "rectangle" || node["kind"] == "ellipse")
        .map(|node| node["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let group_id = "00000000-0000-0000-e000-000000000001";
    let grouped = send(
        &mut host,
        "group",
        json!({
            "type": "command",
            "command": { "kind": "group", "group_id": group_id, "name": "Group", "targets": shapes }
        }),
    );
    assert_eq!(grouped["ok"], true);
    assert_eq!(grouped["history"]["undo_depth"], 1);
    assert_eq!(grouped["projection"]["full"], false);
    assert!(grouped["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == group_id));

    let ungrouped = send(
        &mut host,
        "ungroup",
        json!({
            "type": "command",
            "command": { "kind": "ungroup", "node_id": group_id }
        }),
    );
    assert_eq!(ungrouped["ok"], true);
    assert_eq!(ungrouped["history"]["undo_depth"], 2);
    assert!(ungrouped["projection"]["removed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id == group_id));
}

#[test]
fn invalid_stable_id_command_preserves_revisions_and_has_no_projection_delta() {
    let mut host = EngineHost::new().expect("host");
    let initialized = send(&mut host, "init", json!({ "type": "initialize" }));
    let failed = send(
        &mut host,
        "missing",
        json!({
            "type": "command",
            "command": {
                "kind": "set_opacity",
                "node_id": "ffffffff-ffff-ffff-ffff-ffffffffffff",
                "opacity": 0.5
            }
        }),
    );
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["revisions"], initialized["revisions"]);
    assert_eq!(failed["projection"]["full"], false);
    assert!(failed["projection"]["upserts"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(failed["projection"]["removed"]
        .as_array()
        .unwrap()
        .is_empty());
}
