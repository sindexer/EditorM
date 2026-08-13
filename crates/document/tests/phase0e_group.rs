use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{Command, HeadlessEditorCore, NodeId, NodeSpec};

fn create_rectangle(editor: &mut HeadlessEditorCore, parent: NodeId, name: &str, x: f64) -> NodeId {
    let id = NodeId::new();
    let mut spec = NodeSpec::rectangle(id, name, Vec2::new(40.0, 30.0));
    spec.local_transform = Affine2::translation(Vec2::new(x, 10.0));
    let index = editor.document().node(parent).unwrap().children().len();
    editor
        .dispatch(Command::CreateNode {
            spec,
            parent,
            index,
        })
        .unwrap();
    id
}

#[test]
fn group_and_ungroup_are_atomic_single_history_entries() {
    let mut editor = HeadlessEditorCore::blank("Phase 0E");
    let root = editor.document().root_id();
    let first = create_rectangle(&mut editor, root, "First", 20.0);
    let second = create_rectangle(&mut editor, root, "Second", 100.0);
    let group = NodeId::new();
    let before_group = editor.document().snapshot();
    let depth = editor.history_state().undo_depth;

    editor
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Group"),
            targets: vec![first, second],
        })
        .unwrap();
    assert_eq!(editor.history_state().undo_depth, depth + 1);
    assert_eq!(
        editor.document().node(group).unwrap().children(),
        &[first, second]
    );
    assert_eq!(editor.document().node(first).unwrap().parent(), Some(group));
    assert_eq!(
        editor.document().node(second).unwrap().parent(),
        Some(group)
    );

    let grouped = editor.document().snapshot();
    editor.undo().unwrap();
    assert_eq!(editor.document().snapshot(), before_group);
    editor.redo().unwrap();
    assert_eq!(editor.document().snapshot(), grouped);

    let depth = editor.history_state().undo_depth;
    editor.dispatch(Command::Ungroup { target: group }).unwrap();
    assert_eq!(editor.history_state().undo_depth, depth + 1);
    assert!(editor.document().node(group).is_none());
    assert_eq!(
        editor.document().node(root).unwrap().children(),
        &[first, second]
    );

    editor.undo().unwrap();
    assert_eq!(editor.document().snapshot(), grouped);
}

#[test]
fn failed_group_preserves_document_and_history() {
    let mut editor = HeadlessEditorCore::blank("Phase 0E");
    let root = editor.document().root_id();
    let first = create_rectangle(&mut editor, root, "First", 20.0);
    let snapshot = editor.document().snapshot();
    let history = editor.history_state();

    let result = editor.dispatch(Command::Group {
        group: NodeSpec::group(NodeId::new(), "Invalid"),
        targets: vec![first, NodeId::new()],
    });
    assert!(result.is_err());
    assert_eq!(editor.document().snapshot(), snapshot);
    assert_eq!(editor.history_state(), history);
}

#[test]
fn internal_group_restoration_survives_sibling_insert_delete_and_reorder() {
    let mut editor = HeadlessEditorCore::blank("Phase 0E R2");
    let root = editor.document().root_id();
    let a = create_rectangle(&mut editor, root, "A", 0.0);
    let b = create_rectangle(&mut editor, root, "B", 10.0);
    let c = create_rectangle(&mut editor, root, "C", 20.0);
    let d = create_rectangle(&mut editor, root, "D", 30.0);
    let e = create_rectangle(&mut editor, root, "E", 40.0);
    let f = create_rectangle(&mut editor, root, "F", 50.0);
    let group = NodeId::new();

    let grouped = editor
        .dispatch(Command::Group {
            group: NodeSpec::group(group, "Internal restoration"),
            targets: vec![b, d],
        })
        .unwrap();
    assert_eq!(grouped.sequence_work().full_sequence_scans, 0);
    assert_eq!(grouped.sequence_work().full_sequence_copies, 0);
    assert_eq!(grouped.sequence_work().dense_index_rewrites, 0);
    assert!(!editor
        .document()
        .node(group)
        .unwrap()
        .spec()
        .metadata
        .contains_key("__phase0e_r1_group_positions"));

    let x = create_rectangle(&mut editor, root, "X", -10.0);
    editor
        .dispatch(Command::Reparent {
            child: x,
            new_parent: root,
            index: 0,
        })
        .unwrap();
    editor
        .dispatch(Command::DeleteSubtree { target: c })
        .unwrap();
    editor
        .dispatch(Command::Reparent {
            child: f,
            new_parent: root,
            index: 1,
        })
        .unwrap();

    let ungrouped = editor.dispatch(Command::Ungroup { target: group }).unwrap();
    assert_eq!(ungrouped.sequence_work().full_sequence_scans, 0);
    assert_eq!(ungrouped.sequence_work().full_sequence_copies, 0);
    assert_eq!(ungrouped.sequence_work().dense_index_rewrites, 0);
    assert_eq!(
        editor.document().node(root).unwrap().children(),
        &[x, f, a, b, d, e]
    );
}
