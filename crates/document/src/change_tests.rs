use visual_authoring_core_math::{Affine2, Vec2};

use crate::{
    Command, Document, DocumentChange, Geometry, HeadlessEditorCore, NodeId, NodePlacement,
    NodeSpec, PathAnchor, PathAnchorId, PathGeometry, PersistentProperty,
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

fn open_path(points: &[(PathAnchorId, Vec2)]) -> PathGeometry {
    PathGeometry {
        closed: false,
        anchors: points
            .iter()
            .map(|(id, position)| PathAnchor::new(*id, *position))
            .collect(),
    }
}

#[test]
fn path_creation_preserves_anchor_identity_and_exact_curve_bounds() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let root = editor.document().root_id();
    let path_id = NodeId::new();
    let first = PathAnchorId::new();
    let second = PathAnchorId::new();
    let mut path = open_path(&[
        (first, Vec2::new(10.0, 20.0)),
        (second, Vec2::new(70.0, 80.0)),
    ]);
    path.anchors[0].handle_out = Some(Vec2::new(-15.0, 30.0));

    editor
        .dispatch(Command::CreateNode {
            spec: NodeSpec::path(path_id, "Path", path),
            parent: root,
            index: 0,
        })
        .unwrap();

    let stored = editor.document().node(path_id).unwrap().geometry().unwrap();
    let Geometry::Path(stored_path) = stored else {
        panic!("path node must retain path geometry");
    };
    assert_eq!(stored_path.anchors[0].id, first);
    assert_eq!(stored_path.anchors[1].id, second);
    // Phase 2A renders paths, so bounds are the box the curve actually occupies: the outgoing
    // handle at x = -15 pulls the curve left of both anchors, but only as far as x ≈ 5.398.
    let bounds = editor.document().local_bounds(path_id).unwrap().unwrap();
    assert!(
        (bounds.min.x - 5.397_764_628_538_05).abs() < 1e-9,
        "{bounds:?}"
    );
    assert_eq!(bounds.min.y, 20.0);
    assert_eq!(bounds.max, Vec2::new(70.0, 80.0));
    // The control hull stays available for callers that want the cheap outer box.
    assert_eq!(
        stored_path.conservative_bounds().unwrap(),
        visual_authoring_core_math::Rect::from_min_max(
            Vec2::new(-15.0, 20.0),
            Vec2::new(70.0, 80.0)
        )
    );
}

#[test]
fn invalid_path_geometry_command_is_failure_atomic() {
    let mut editor = HeadlessEditorCore::blank("Root");
    let root = editor.document().root_id();
    let path_id = NodeId::new();
    let first = PathAnchorId::new();
    let second = PathAnchorId::new();
    editor
        .dispatch(Command::CreateNode {
            spec: NodeSpec::path(
                path_id,
                "Path",
                open_path(&[(first, Vec2::ZERO), (second, Vec2::new(10.0, 10.0))]),
            ),
            parent: root,
            index: 0,
        })
        .unwrap();
    let before = editor.document().snapshot();
    let history_before = editor.history_state();

    let result = editor.dispatch(Command::SetGeometry {
        target: path_id,
        geometry: Geometry::Path(open_path(&[
            (first, Vec2::ZERO),
            (first, Vec2::new(20.0, 20.0)),
        ])),
    });

    assert!(result.is_err());
    assert_eq!(editor.document().snapshot(), before);
    assert_eq!(editor.history_state(), history_before);
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
