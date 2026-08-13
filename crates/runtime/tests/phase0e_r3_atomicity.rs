use serde_json::json;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::EngineRuntime;

#[test]
fn invalid_public_specs_and_duplicate_ids_leave_every_runtime_plane_unchanged() {
    let mut runtime = EngineRuntime::blank("R3 atomicity").expect("runtime");
    let root = runtime.document().root_id();
    let child = NodeId::new();
    runtime
        .dispatch(Command::CreateNode {
            spec: NodeSpec::rectangle(child, "Stable", Vec2::new(10.0, 10.0)),
            parent: root,
            index: 0,
        })
        .expect("seed node");
    runtime.select_only(child).expect("selection");

    let document = runtime.document().snapshot();
    let sequence_work = runtime.document().sequence_work();
    let scene = runtime.scene().semantic_snapshot();
    let render_revision = runtime.render_model().revision();
    let render_counters = runtime.render_model().counters();
    let revision = runtime.document_revision();
    let history = runtime.history_state();
    let selection = runtime.selection().clone();

    let assert_unchanged = |runtime: &EngineRuntime| {
        assert_eq!(runtime.document().snapshot(), document);
        assert_eq!(runtime.document().sequence_work(), sequence_work);
        assert_eq!(runtime.scene().semantic_snapshot(), scene);
        assert_eq!(runtime.render_model().revision(), render_revision);
        assert_eq!(runtime.render_model().counters(), render_counters);
        assert_eq!(runtime.document_revision(), revision);
        assert_eq!(runtime.history_state(), history);
        assert_eq!(runtime.selection(), &selection);
    };

    let mut invalid = NodeSpec::rectangle(NodeId::new(), "Invalid", Vec2::new(4.0, 4.0));
    invalid.local_transform = Affine2::from_components(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0);
    assert!(runtime
        .dispatch(Command::CreateNode {
            spec: invalid,
            parent: root,
            index: 1,
        })
        .is_err());
    assert_unchanged(&runtime);

    assert!(runtime
        .dispatch(Command::CreateNode {
            spec: NodeSpec::rectangle(child, "Duplicate", Vec2::new(4.0, 4.0)),
            parent: root,
            index: 1,
        })
        .is_err());
    assert_unchanged(&runtime);

    println!(
        "PHASE0E_R3_ATOMICITY_JSON={}",
        json!({
            "invalid_public_node_spec": "typed_error",
            "duplicate_node_id": "typed_error",
            "document_semantic_unchanged": true,
            "history_unchanged": true,
            "revision_unchanged": true,
            "selection_unchanged": true,
            "scene_unchanged": true,
            "render_unchanged": true,
            "sequence_diagnostics_unchanged": true,
            "all_passed": true
        })
    );
}
