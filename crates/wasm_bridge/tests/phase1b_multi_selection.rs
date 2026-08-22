//! Phase 1B EngineHost protocol: multiple selection, rubber-band selection, snapped batch
//! translation, and alignment as one undoable request.

use serde_json::{json, Value};
use visual_authoring_wasm_bridge::EngineHost;

fn send(host: &mut EngineHost, request_id: &str, operation: Value) -> Value {
    let mut envelope = operation;
    envelope["protocol_version"] = json!(1);
    envelope["request_id"] = json!(request_id);
    serde_json::from_str(&host.handle_json(&envelope.to_string())).expect("valid response JSON")
}

fn ok(response: &Value) -> bool {
    response["ok"] == true
}

struct RectangleSpec<'a> {
    request_id: &'a str,
    parent: &'a str,
    index: usize,
    name: &'a str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Creates one rectangle inside the editor fixture's Frame and returns its ID.
fn create_rectangle(host: &mut EngineHost, spec: RectangleSpec<'_>) -> String {
    let RectangleSpec {
        request_id,
        parent,
        index,
        name,
        x,
        y,
        width,
        height,
    } = spec;
    let id = format!("00000000-0000-4000-8000-{:012}", index + 100);
    let response = send(
        host,
        request_id,
        json!({
            "type": "command",
            "command": {
                "kind": "create_shape",
                "node_id": id,
                "parent_id": parent,
                "index": index,
                "shape": "rectangle",
                "name": name,
                "x": x,
                "y": y,
                "width": width,
                "height": height,
            },
        }),
    );
    assert!(ok(&response), "create failed: {response}");
    id
}

struct Fixture {
    host: EngineHost,
    frame: String,
    ids: Vec<String>,
}

/// Editor fixture with three rectangles laid out at different offsets inside the Frame.
fn scene() -> Fixture {
    let mut host = EngineHost::new().expect("host");
    let loaded = send(
        &mut host,
        "load",
        json!({ "type": "load_fixture", "fixture": "editor" }),
    );
    assert!(ok(&loaded), "{loaded}");
    // A Full HD viewport at zoom 1 makes the Frame exactly fill the visible world, so viewport
    // pixel x maps to world x + 960 and viewport pixel y maps to world y + 540.
    let resized = send(
        &mut host,
        "resize",
        json!({ "type": "camera", "camera": { "kind": "resize", "width": 1920.0, "height": 1080.0, "dpr": 1.0 } }),
    );
    assert!(ok(&resized), "{resized}");
    let frame = loaded["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["kind"] == "frame")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Frame-local positions; the Frame itself sits at world (-960, -540).
    let ids = vec![
        create_rectangle(
            &mut host,
            RectangleSpec {
                request_id: "make-a",
                parent: &frame,
                index: 0,
                name: "A",
                x: 0.0,
                y: 400.0,
                width: 100.0,
                height: 60.0,
            },
        ),
        create_rectangle(
            &mut host,
            RectangleSpec {
                request_id: "make-b",
                parent: &frame,
                index: 1,
                name: "B",
                x: 300.0,
                y: 500.0,
                width: 100.0,
                height: 60.0,
            },
        ),
        create_rectangle(
            &mut host,
            RectangleSpec {
                request_id: "make-c",
                parent: &frame,
                index: 2,
                name: "C",
                x: 700.0,
                y: 600.0,
                width: 100.0,
                height: 60.0,
            },
        ),
    ];
    Fixture { host, frame, ids }
}

fn selection(response: &Value) -> Vec<String> {
    response["selection"]["ordered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

fn world_x(host: &mut EngineHost, id: &str) -> f64 {
    let snapshot = send(host, "snapshot", json!({ "type": "get_ui_snapshot" }));
    snapshot["projection"]["upserts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap()["world_bounds"]["min"][0]
        .as_f64()
        .unwrap()
}

#[test]
fn selection_set_mode_replaces_the_whole_selection_atomically() {
    let mut fixture = scene();
    let response = send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    assert!(ok(&response), "{response}");
    assert_eq!(selection(&response), fixture.ids);
    assert_eq!(response["result"]["selection_count"], 3);

    // One unknown ID rejects the request and leaves the previous selection untouched.
    let mut broken = fixture.ids.clone();
    broken.push("00000000-0000-4000-8000-999999999999".to_owned());
    let rejected = send(
        &mut fixture.host,
        "select-broken",
        json!({ "type": "selection", "mode": "set", "targets": broken }),
    );
    assert!(!ok(&rejected));
    assert_eq!(selection(&rejected), fixture.ids);
}

#[test]
fn a_marquee_selects_every_intersecting_top_level_node() {
    let mut fixture = scene();
    // Viewport x 0..500 covers world x -960..-460, which crosses A and B but not C.
    let response = send(
        &mut fixture.host,
        "marquee",
        json!({
            "type": "marquee_select",
            "x0": 0.0,
            "y0": 0.0,
            "x1": 500.0,
            "y1": 1080.0,
            "root_id": fixture.frame.clone(),
        }),
    );
    assert!(ok(&response), "{response}");
    let mut selected = response["result"]["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    selected.sort();
    let mut expected = vec![fixture.ids[0].clone(), fixture.ids[1].clone()];
    expected.sort();
    assert_eq!(selected, expected);
    assert_eq!(selection(&response).len(), 2);
}

#[test]
fn an_additive_marquee_keeps_the_existing_selection() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-c",
        json!({ "type": "selection", "target": fixture.ids[2].clone(), "mode": "replace" }),
    );
    let response = send(
        &mut fixture.host,
        "marquee-additive",
        json!({
            "type": "marquee_select",
            "x0": 0.0,
            "y0": 0.0,
            "x1": 500.0,
            "y1": 1080.0,
            "additive": true,
            "root_id": fixture.frame.clone(),
        }),
    );
    assert!(ok(&response), "{response}");
    let ordered = selection(&response);
    assert_eq!(ordered.len(), 3);
    assert_eq!(
        ordered[0], fixture.ids[2],
        "the earlier selection stays first"
    );
}

#[test]
fn a_marquee_outside_every_object_clears_the_selection() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let response = send(
        &mut fixture.host,
        "marquee-empty",
        json!({
            "type": "marquee_select",
            "x0": -4000.0,
            "y0": -4000.0,
            "x1": -3900.0,
            "y1": -3900.0,
            "root_id": fixture.frame.clone(),
        }),
    );
    assert!(ok(&response), "{response}");
    assert!(selection(&response).is_empty());
}

#[test]
fn translating_a_multiple_selection_moves_every_node_by_one_delta() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let before = fixture
        .ids
        .iter()
        .map(|id| world_x(&mut fixture.host, id))
        .collect::<Vec<_>>();

    let begin = send(
        &mut fixture.host,
        "begin",
        json!({ "type": "begin_transaction" }),
    );
    assert_eq!(begin["result"]["drag_targets"], 3);
    let moved = send(
        &mut fixture.host,
        "translate",
        json!({ "type": "translate_selection", "dx": 40.0, "dy": 0.0, "snap": false }),
    );
    assert!(ok(&moved), "{moved}");
    assert_eq!(moved["result"]["moved"], 3);
    assert_eq!(moved["result"]["applied_delta"][0], 40.0);
    let committed = send(
        &mut fixture.host,
        "commit",
        json!({ "type": "commit_transaction" }),
    );
    assert!(ok(&committed));

    for (id, start) in fixture.ids.clone().iter().zip(before.iter()) {
        let now = world_x(&mut fixture.host, id);
        assert!(
            (now - (start + 40.0)).abs() < 1.0e-6,
            "{id}: {start} -> {now}"
        );
    }

    // The whole drag is one undo step.
    let undone = send(&mut fixture.host, "undo", json!({ "type": "undo" }));
    assert!(ok(&undone));
    for (id, start) in fixture.ids.clone().iter().zip(before.iter()) {
        let now = world_x(&mut fixture.host, id);
        assert!(
            (now - start).abs() < 1.0e-6,
            "{id} did not return to {start}"
        );
    }
}

#[test]
fn repeated_drag_frames_are_measured_from_the_transaction_base() {
    let mut fixture = scene();
    let target = fixture.ids[0].clone();
    send(
        &mut fixture.host,
        "select",
        json!({ "type": "selection", "target": target, "mode": "replace" }),
    );
    let start = world_x(&mut fixture.host, &target);

    send(
        &mut fixture.host,
        "begin",
        json!({ "type": "begin_transaction" }),
    );
    for (index, delta) in [10.0, 25.0, 60.0].iter().enumerate() {
        let response = send(
            &mut fixture.host,
            &format!("translate-{index}"),
            json!({ "type": "translate_selection", "dx": delta, "dy": 0.0, "snap": false }),
        );
        assert!(ok(&response), "{response}");
    }
    send(
        &mut fixture.host,
        "commit",
        json!({ "type": "commit_transaction" }),
    );

    let end = world_x(&mut fixture.host, &target);
    assert!(
        (end - (start + 60.0)).abs() < 1.0e-6,
        "drag accumulated drift: {start} -> {end}"
    );
}

#[test]
fn snapping_corrects_the_applied_delta_and_reports_a_guide() {
    let mut fixture = scene();
    let target = fixture.ids[1].clone();
    send(
        &mut fixture.host,
        "select",
        json!({ "type": "selection", "target": target, "mode": "replace" }),
    );
    send(
        &mut fixture.host,
        "begin",
        json!({ "type": "begin_transaction" }),
    );

    // Rectangle B starts at frame-local x = 300 and rectangle A's left edge is at 0, so a drag of
    // -297 lands three units away from lining up with A.
    let response = send(
        &mut fixture.host,
        "translate-snap",
        json!({
            "type": "translate_selection",
            "dx": -297.0,
            "dy": 0.0,
            "snap": true,
            "snap_threshold_px": 8.0,
        }),
    );
    assert!(ok(&response), "{response}");
    assert_eq!(response["result"]["snapped"], true);
    let applied = response["result"]["applied_delta"][0].as_f64().unwrap();
    assert!((applied - -300.0).abs() < 1.0e-6, "applied {applied}");
    let guides = response["result"]["guides"].as_array().unwrap();
    assert!(!guides.is_empty());
    assert_eq!(guides[0]["axis"], "vertical");
    assert_eq!(guides[0]["target"], fixture.ids[0]);
    send(
        &mut fixture.host,
        "commit",
        json!({ "type": "commit_transaction" }),
    );
}

#[test]
fn translation_without_a_transaction_is_a_typed_failure() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let response = send(
        &mut fixture.host,
        "translate-no-transaction",
        json!({ "type": "translate_selection", "dx": 10.0, "dy": 0.0, "snap": false }),
    );
    assert!(!ok(&response));
    assert_eq!(response["error"]["code"], "no_transaction");
}

#[test]
fn aligning_the_selection_is_one_request_and_one_undo_step() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let before_history = send(&mut fixture.host, "probe", json!({ "type": "heartbeat" }))
        ["history"]["undo_depth"]
        .as_u64()
        .unwrap();

    let response = send(
        &mut fixture.host,
        "align",
        json!({ "type": "arrange", "operation": "align_left" }),
    );
    assert!(ok(&response), "{response}");
    assert_eq!(response["result"]["moved"], 2);
    assert_eq!(response["result"]["operation"], "align_left");
    assert_eq!(
        response["history"]["undo_depth"].as_u64().unwrap(),
        before_history + 1
    );
    assert_eq!(response["history"]["transaction_active"], false);

    let left_edges = fixture
        .ids
        .clone()
        .iter()
        .map(|id| world_x(&mut fixture.host, id))
        .collect::<Vec<_>>();
    for edge in &left_edges[1..] {
        assert!((edge - left_edges[0]).abs() < 1.0e-6, "{left_edges:?}");
    }

    let undone = send(&mut fixture.host, "undo", json!({ "type": "undo" }));
    assert!(ok(&undone));
    let restored = fixture
        .ids
        .clone()
        .iter()
        .map(|id| world_x(&mut fixture.host, id))
        .collect::<Vec<_>>();
    assert!(
        (restored[1] - restored[0]).abs() > 1.0,
        "undo did not restore the layout: {restored:?}"
    );
}

#[test]
fn distributing_fewer_than_three_nodes_fails_without_changing_the_document() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-two",
        json!({ "type": "selection", "mode": "set", "targets": vec![fixture.ids[0].clone(), fixture.ids[1].clone()] }),
    );
    let before = world_x(&mut fixture.host, &fixture.ids[0].clone());
    let response = send(
        &mut fixture.host,
        "distribute",
        json!({ "type": "arrange", "operation": "distribute_horizontal" }),
    );
    assert!(!ok(&response));
    assert_eq!(response["history"]["transaction_active"], false);
    let after = world_x(&mut fixture.host, &fixture.ids[0].clone());
    assert!((before - after).abs() < 1.0e-9);
}

#[test]
fn an_unknown_arrange_operation_is_rejected() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let response = send(
        &mut fixture.host,
        "bad-arrange",
        json!({ "type": "arrange", "operation": "align_diagonal" }),
    );
    assert!(!ok(&response));
    assert_eq!(response["error"]["code"], "invalid_arrange");
}

#[test]
fn a_failed_drag_frame_rolls_the_transaction_back_instead_of_moving_part_of_it() {
    let mut fixture = scene();
    send(
        &mut fixture.host,
        "select-many",
        json!({ "type": "selection", "mode": "set", "targets": fixture.ids.clone() }),
    );
    let before = fixture
        .ids
        .clone()
        .iter()
        .map(|id| world_x(&mut fixture.host, id))
        .collect::<Vec<_>>();

    send(
        &mut fixture.host,
        "begin",
        json!({ "type": "begin_transaction" }),
    );
    // Remove one dragged node inside the same transaction, so the next drag frame cannot apply
    // every command it planned.
    let deleted = send(
        &mut fixture.host,
        "delete-mid-drag",
        json!({ "type": "update_transaction", "command": { "kind": "delete_node", "node_id": fixture.ids[1].clone() } }),
    );
    assert!(ok(&deleted), "{deleted}");

    let failed = send(
        &mut fixture.host,
        "translate-after-delete",
        json!({ "type": "translate_selection", "dx": 40.0, "dy": 0.0, "snap": false }),
    );
    assert!(!ok(&failed), "{failed}");
    assert_eq!(failed["history"]["transaction_active"], false);

    for (id, start) in fixture.ids.clone().iter().zip(before.iter()) {
        let now = world_x(&mut fixture.host, id);
        assert!(
            (now - start).abs() < 1.0e-9,
            "{id} kept a partial drag: {start} -> {now}"
        );
    }
}

#[test]
fn a_band_drawn_on_a_frame_excludes_that_frame_from_its_result() {
    let mut fixture = scene();
    // Pressing a Frame's own area bands across its siblings and children; the Frame itself is the
    // backdrop the band was drawn on, so selecting it would move the whole artboard.
    let response = send(
        &mut fixture.host,
        "marquee-exclude",
        json!({
            "type": "marquee_select",
            "x0": 0.0,
            "y0": 0.0,
            "x1": 1920.0,
            "y1": 1080.0,
            "exclude_ids": [fixture.frame.clone()],
        }),
    );
    assert!(ok(&response), "{response}");
    let selected = response["result"]["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(
        !selected.contains(&fixture.frame),
        "the band selected its own backdrop: {selected:?}"
    );

    // Without the exclusion the same band selects the Frame, because the band crosses it.
    let included = send(
        &mut fixture.host,
        "marquee-include",
        json!({
            "type": "marquee_select",
            "x0": 0.0,
            "y0": 0.0,
            "x1": 1920.0,
            "y1": 1080.0,
        }),
    );
    assert!(ok(&included), "{included}");
    let with_frame = included["result"]["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(with_frame.contains(&fixture.frame), "{with_frame:?}");
}
