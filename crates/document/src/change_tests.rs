use visual_authoring_core_math::{Affine2, Vec2};

use crate::{
    Command, Document, DocumentChange, Geometry, HeadlessEditorCore, NodeId, NodePlacement,
    NodeSpec, PersistentProperty,
};

fn create_rectangle(editor: &mut HeadlessEditorCore) -> NodeId {
    let root = editor.document().root_id();
    let id = NodeId::new();
    editor
        .dispatch(Command::CreateNode {
            spec: NodeSpec::rectangle(id, "Rectangle", Vec2::new(10.0, 10.0)),
            parent: root,
            index: editor.document().node(root).unwrap().children().len(),
        })
        .unwrap();
    id
}

#[test]
fn command_outcomes_classify_scene_relevant_and_irrelevant_properties() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let id = create_rectangle(&mut editor);
    let transform = editor
        .dispatch(Command::SetLocalTransform {
            target: id,
            transform: Affine2::translation(Vec2::new(20.0, 30.0)),
        })
        .unwrap();
    assert!(matches!(
        transform.change_set().changes(),
        [DocumentChange::LocalTransformChanged { node }] if *node == id
    ));
    let geometry = editor
        .dispatch(Command::SetGeometry {
            target: id,
            geometry: Geometry::Rectangle {
                size: Vec2::new(30.0, 40.0),
            },
        })
        .unwrap();
    assert!(matches!(
        geometry.change_set().changes(),
        [DocumentChange::GeometryChanged { node }] if *node == id
    ));
    let name = editor
        .dispatch(Command::SetName {
            target: id,
            name: "Renamed".into(),
        })
        .unwrap();
    assert!(matches!(
        name.change_set().changes(),
        [DocumentChange::PersistentPropertyChanged { node, property: PersistentProperty::Name }]
            if *node == id
    ));
}

#[test]
fn transaction_rollback_reports_reverse_semantic_changes_and_revision() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let id = create_rectangle(&mut editor);
    editor.begin_transaction().unwrap();
    let preview = editor
        .update_transaction(Command::SetVisible {
            target: id,
            visible: false,
        })
        .unwrap();
    let preview_after = preview.change_set().revision().after;
    let rollback = editor.rollback_transaction().unwrap();
    assert!(matches!(
        rollback.changes(),
        [DocumentChange::VisibilityChanged { node }] if *node == id
    ));
    assert_eq!(rollback.revision().before, preview_after);
    assert_eq!(rollback.revision().after, editor.revision());
}

#[test]
fn undo_redo_reports_correct_insert_remove_direction_without_exposing_effects() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let id = create_rectangle(&mut editor);
    let undo = editor.undo().unwrap().unwrap();
    assert!(matches!(
        undo.changes(),
        [DocumentChange::NodesRemoved { root, nodes, .. }]
            if *root == id && nodes == &vec![id]
    ));
    let redo = editor.redo().unwrap().unwrap();
    assert!(matches!(
        redo.changes(),
        [DocumentChange::NodesInserted { root, nodes, .. }]
            if *root == id && nodes == &vec![id]
    ));
}

#[test]
fn reorder_report_contains_before_after_placement_and_subtree() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let root = editor.document().root_id();
    let first = create_rectangle(&mut editor);
    create_rectangle(&mut editor);
    let outcome = editor
        .dispatch(Command::Reparent {
            child: first,
            new_parent: root,
            index: 1,
        })
        .unwrap();
    assert!(matches!(
        outcome.change_set().changes(),
        [DocumentChange::PlacementChanged {
            root: moved,
            subtree,
            before: Some(NodePlacement { parent: before_parent, index: 0 }),
            after: Some(NodePlacement { parent: after_parent, index: 1 }),
            ..
        }] if *moved == first
            && subtree == &vec![first]
            && *before_parent == root
            && *after_parent == root
    ));
}

#[test]
fn document_replacement_reports_full_reset_and_clears_history() {
    let mut editor = HeadlessEditorCore::blank("Root");
    create_rectangle(&mut editor);
    let report = editor
        .replace_document(Document::new("Replacement"))
        .unwrap();
    assert_eq!(report.changes(), &[DocumentChange::FullDocumentReset]);
    assert_eq!(report.revision().after, editor.revision());
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
}
