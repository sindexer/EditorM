use serde_json::{json, Value};
use visual_authoring_wasm_bridge::EngineHost;

fn send(host: &mut EngineHost, request_id: &str, operation: Value) -> Value {
    let mut envelope = operation;
    envelope["protocol_version"] = json!(1);
    envelope["request_id"] = json!(request_id);
    serde_json::from_str(&host.handle_json(&envelope.to_string())).expect("valid response JSON")
}

fn slide_ids(response: &Value) -> Vec<String> {
    response["editor_session"]["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slide| slide["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn typed_slide_protocol_updates_session_projection_history_and_visibility() {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load",
        json!({ "type": "load_fixture", "fixture": "editor" }),
    );
    assert_eq!(loaded["ok"], true);
    let first = slide_ids(&loaded)[0].clone();
    let second = "10000000-0000-0000-0000-000000000002";
    let created = send(
        &mut host,
        "create",
        json!({
            "type": "slide",
            "slide": {
                "kind": "create",
                "slide_id": second,
                "index": 1
            }
        }),
    );
    assert_eq!(created["ok"], true);
    assert_eq!(created["history"]["undo_depth"], 1);
    assert_eq!(created["editor_session"]["active_slide_id"], second);
    assert_eq!(slide_ids(&created), vec![first.clone(), second.to_owned()]);
    assert_eq!(created["editor_session"]["slides"][1]["width"], 1920.0);
    assert_eq!(created["editor_session"]["slides"][1]["height"], 1080.0);
    assert_eq!(created["binary"]["visible_slots"], true);
    assert_eq!(created["projection"]["full"], false);

    let outside_create = send(
        &mut host,
        "outside",
        json!({
            "type": "command",
            "command": {
                "kind": "create_shape",
                "node_id": "10000000-0000-0000-0000-000000000099",
                "parent_id": first,
                "index": 0,
                "shape": "rectangle",
                "name": "Wrong Slide",
                "x": 10.0,
                "y": 10.0,
                "width": 80.0,
                "height": 60.0
            }
        }),
    );
    assert_eq!(outside_create["ok"], false);
    assert_eq!(outside_create["error"]["code"], "node_outside_active_slide");
    assert_eq!(outside_create["revisions"], created["revisions"]);

    let duplicate = "10000000-0000-0000-0000-000000000003";
    let duplicated = send(
        &mut host,
        "duplicate",
        json!({
            "type": "slide",
            "slide": {
                "kind": "duplicate",
                "source_id": first,
                "slide_id": duplicate,
                "index": 2
            }
        }),
    );
    assert_eq!(duplicated["ok"], true);
    assert_eq!(duplicated["history"]["undo_depth"], 2);
    assert_eq!(duplicated["editor_session"]["active_slide_id"], duplicate);
    assert!(!duplicated["projection"]["upserts"]
        .as_array()
        .unwrap()
        .is_empty());

    let revisions = duplicated["revisions"].clone();
    let history = duplicated["history"].clone();
    let activated = send(
        &mut host,
        "activate",
        json!({
            "type": "slide",
            "slide": { "kind": "activate", "slide_id": first }
        }),
    );
    assert_eq!(activated["ok"], true);
    assert_eq!(activated["revisions"], revisions);
    assert_eq!(activated["history"], history);
    assert_eq!(activated["editor_session"]["active_slide_id"], first);
    assert_eq!(activated["binary"]["visible_slots"], true);
}

#[test]
fn last_slide_delete_is_fail_closed_at_the_protocol_boundary() {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load",
        json!({ "type": "load_fixture", "fixture": "editor" }),
    );
    let only = slide_ids(&loaded)[0].clone();
    let failed = send(
        &mut host,
        "delete",
        json!({
            "type": "slide",
            "slide": { "kind": "delete", "slide_id": only }
        }),
    );
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["revisions"], loaded["revisions"]);
    assert_eq!(failed["history"], loaded["history"]);
    assert_eq!(slide_ids(&failed).len(), 1);
}

#[test]
fn thumbnail_slots_are_render_model_derived_and_do_not_mutate_editor_state() {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load-thumbnail",
        json!({ "type": "load_fixture", "fixture": "editor" }),
    );
    let slide = slide_ids(&loaded)[0].clone();
    let response = send(
        &mut host,
        "thumbnail-slots",
        json!({ "type": "thumbnail_slots", "slide_id": slide }),
    );
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["kind"], "thumbnail_slots");
    assert_eq!(response["result"]["slide_id"], slide);
    assert!(!response["result"]["slots"].as_array().unwrap().is_empty());
    assert_eq!(response["result"]["full_render_model_scans"], 0);
    assert_eq!(response["revisions"], loaded["revisions"]);
    assert_eq!(response["history"], loaded["history"]);
    assert_eq!(response["selection"], loaded["selection"]);
    assert_eq!(
        response["editor_session"]["active_slide_id"],
        loaded["editor_session"]["active_slide_id"]
    );
    assert_eq!(response["binary"]["full_instances"], false);
    assert_eq!(response["binary"]["visible_slots"], false);
}

#[test]
fn save_load_preserves_slide_order_names_and_independent_contents() {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load-save",
        json!({ "type": "load_fixture", "fixture": "editor" }),
    );
    let first = slide_ids(&loaded)[0].clone();
    let second = "20000000-0000-0000-0000-000000000002";
    send(
        &mut host,
        "create-save",
        json!({
            "type": "slide",
            "slide": { "kind": "create", "slide_id": second, "index": 1, "name": "Closing" }
        }),
    );
    let shape = "20000000-0000-0000-0000-000000000099";
    send(
        &mut host,
        "shape-save",
        json!({
            "type": "command",
            "command": {
                "kind": "create_shape",
                "node_id": shape,
                "parent_id": second,
                "index": 0,
                "shape": "ellipse",
                "name": "Closing mark",
                "x": 120.0,
                "y": 140.0,
                "width": 320.0,
                "height": 180.0
            }
        }),
    );
    let reordered = send(
        &mut host,
        "reorder-save",
        json!({
            "type": "slide",
            "slide": { "kind": "reorder", "slide_id": second, "index": 0 }
        }),
    );
    assert_eq!(
        slide_ids(&reordered),
        vec![second.to_owned(), first.clone()]
    );
    let saved = send(&mut host, "save", json!({ "type": "save_document" }));
    let document_json = saved["result"]["document_json"].as_str().unwrap();

    let mut restored_host = EngineHost::new().expect("restored host");
    let restored = send(
        &mut restored_host,
        "restore",
        json!({ "type": "load_document", "document_json": document_json }),
    );
    assert_eq!(restored["ok"], true);
    assert_eq!(slide_ids(&restored), vec![second.to_owned(), first]);
    assert_eq!(restored["editor_session"]["slides"][0]["name"], "Closing");
    assert_eq!(restored["editor_session"]["slides"][0]["child_count"], 1);
    let snapshot = send(
        &mut restored_host,
        "snapshot",
        json!({ "type": "get_ui_snapshot" }),
    );
    assert!(snapshot["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == shape && node["name"] == "Closing mark"));
}
