use visual_authoring_core_math::Vec2;
use visual_authoring_document::{Command, Document, HeadlessEditorCore, NodeId, NodeSpec};
use visual_authoring_serialization::{from_json, to_json_pretty};

#[test]
fn noncontiguous_group_positions_survive_save_load_and_ungroup() {
    let mut editor = HeadlessEditorCore::new(Document::new("round trip")).unwrap();
    let root = editor.document().root_id();
    let ids = (0..5).map(|_| NodeId::new()).collect::<Vec<_>>();
    for (index, id) in ids.iter().copied().enumerate() {
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(id, format!("node {index}"), Vec2::new(10.0, 10.0)),
                parent: root,
                index,
            })
            .unwrap();
    }
    let initial = editor.document().snapshot();
    let group = NodeId::new();
    editor
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "group"),
            targets: vec![ids[0], ids[2], ids[4]],
        })
        .unwrap();

    let encoded = to_json_pretty(editor.document()).unwrap();
    let restored = from_json(&encoded).unwrap();
    let mut restored_editor = HeadlessEditorCore::new(restored).unwrap();
    restored_editor
        .dispatch(Command::Ungroup { target: group })
        .unwrap();
    assert_eq!(restored_editor.document().snapshot(), initial);
}
