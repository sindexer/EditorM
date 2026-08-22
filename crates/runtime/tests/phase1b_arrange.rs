//! Phase 1B: alignment and distribution are planned in world space, applied as one atomic
//! transaction, and reversed by a single undo.

use visual_authoring_core_math::{Affine2, Vec2, DEFAULT_EPSILON};
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::{
    AlignMode, ArrangeError, DistributeAxis, EngineRuntime, RuntimeError,
};

fn rectangle(
    runtime: &mut EngineRuntime,
    parent: NodeId,
    size: Vec2,
    position: Vec2,
    name: &str,
) -> NodeId {
    let id = NodeId::new();
    let mut spec = NodeSpec::rectangle(id, name, size);
    spec.local_transform = Affine2::translation(position);
    let index = runtime.document().node(parent).unwrap().children().len();
    runtime
        .dispatch(Command::CreateNode {
            spec,
            parent,
            index,
        })
        .unwrap();
    id
}

fn world_bounds(runtime: &EngineRuntime, id: NodeId) -> visual_authoring_core_math::Rect {
    runtime
        .scene()
        .node(id)
        .unwrap()
        .subtree_world_bounds()
        .unwrap()
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-6,
        "expected {expected}, found {actual}"
    );
}

#[test]
fn align_left_moves_every_target_to_the_selection_edge() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(
        &mut runtime,
        root,
        Vec2::new(40.0, 20.0),
        Vec2::new(10.0, 0.0),
        "A",
    );
    let second = rectangle(
        &mut runtime,
        root,
        Vec2::new(30.0, 20.0),
        Vec2::new(90.0, 40.0),
        "B",
    );
    let third = rectangle(
        &mut runtime,
        root,
        Vec2::new(50.0, 20.0),
        Vec2::new(55.0, 80.0),
        "C",
    );

    let plan = runtime
        .plan_align(&[first, second, third], AlignMode::Left)
        .unwrap();
    assert_eq!(plan.operation(), "align_left");
    assert_eq!(plan.targets_examined(), 3);
    assert_eq!(plan.unchanged_targets(), 1, "the leftmost target stays put");
    assert_eq!(plan.moves().len(), 2);

    let outcome = runtime.apply_arrange(&plan).unwrap();
    assert!(outcome.committed);
    assert_eq!(outcome.applied(), 2);
    assert!(outcome.changed());

    for id in [first, second, third] {
        assert_close(world_bounds(&runtime, id).min.x, 10.0);
    }
    // Only the horizontal axis moves.
    assert_close(world_bounds(&runtime, third).min.y, 80.0);
}

#[test]
fn align_center_uses_the_union_of_selection_bounds() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 20.0),
        Vec2::new(0.0, 0.0),
        "A",
    );
    let second = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 20.0),
        Vec2::new(80.0, 40.0),
        "B",
    );

    let plan = runtime
        .plan_align(&[first, second], AlignMode::HorizontalCenter)
        .unwrap();
    runtime.apply_arrange(&plan).unwrap();

    let expected_center = 50.0;
    for id in [first, second] {
        assert_close(world_bounds(&runtime, id).center().x, expected_center);
    }
}

#[test]
fn one_alignment_is_one_undo_step_and_restores_every_target() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let ids = [
        rectangle(
            &mut runtime,
            root,
            Vec2::new(20.0, 20.0),
            Vec2::new(0.0, 0.0),
            "A",
        ),
        rectangle(
            &mut runtime,
            root,
            Vec2::new(20.0, 20.0),
            Vec2::new(60.0, 15.0),
            "B",
        ),
        rectangle(
            &mut runtime,
            root,
            Vec2::new(20.0, 20.0),
            Vec2::new(120.0, 35.0),
            "C",
        ),
    ];
    let before = ids
        .iter()
        .map(|id| runtime.document().node(*id).unwrap().local_transform())
        .collect::<Vec<_>>();
    let undo_depth = runtime.history_state().undo_depth;

    let plan = runtime.plan_align(&ids, AlignMode::Top).unwrap();
    runtime.apply_arrange(&plan).unwrap();
    assert_eq!(runtime.history_state().undo_depth, undo_depth + 1);

    runtime.undo().unwrap().unwrap();
    for (id, expected) in ids.iter().zip(before.iter()) {
        assert!(runtime
            .document()
            .node(*id)
            .unwrap()
            .local_transform()
            .approx_eq(*expected, DEFAULT_EPSILON));
    }
    assert_eq!(runtime.history_state().undo_depth, undo_depth);
}

#[test]
fn distribute_horizontal_produces_equal_gaps_and_keeps_the_extremes() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let left = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 10.0),
        Vec2::new(0.0, 0.0),
        "A",
    );
    let middle = rectangle(
        &mut runtime,
        root,
        Vec2::new(40.0, 10.0),
        Vec2::new(30.0, 0.0),
        "B",
    );
    let right = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 10.0),
        Vec2::new(180.0, 0.0),
        "C",
    );

    let plan = runtime
        .plan_distribute(&[left, middle, right], DistributeAxis::Horizontal)
        .unwrap();
    runtime.apply_arrange(&plan).unwrap();

    let left_bounds = world_bounds(&runtime, left);
    let middle_bounds = world_bounds(&runtime, middle);
    let right_bounds = world_bounds(&runtime, right);
    assert_close(left_bounds.min.x, 0.0);
    assert_close(right_bounds.max.x, 200.0);
    let first_gap = middle_bounds.min.x - left_bounds.max.x;
    let second_gap = right_bounds.min.x - middle_bounds.max.x;
    assert_close(first_gap, second_gap);
    assert_close(first_gap, 60.0);
}

#[test]
fn distribution_rejects_fewer_than_three_targets() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(&mut runtime, root, Vec2::new(10.0, 10.0), Vec2::ZERO, "A");
    let second = rectangle(
        &mut runtime,
        root,
        Vec2::new(10.0, 10.0),
        Vec2::new(50.0, 0.0),
        "B",
    );

    let error = runtime
        .plan_distribute(&[first, second], DistributeAxis::Horizontal)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Arrange(ArrangeError::InsufficientTargets {
            operation: "distribute_horizontal",
            required: 3,
            actual: 2,
        })
    );
}

#[test]
fn alignment_rejects_a_selection_that_contains_a_container_and_its_child() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let frame_id = NodeId::new();
    let mut frame = NodeSpec::frame(frame_id, "Frame", Vec2::new(200.0, 200.0));
    frame.local_transform = Affine2::translation(Vec2::new(0.0, 0.0));
    runtime
        .dispatch(Command::CreateNode {
            spec: frame,
            parent: root,
            index: 0,
        })
        .unwrap();
    let child = rectangle(
        &mut runtime,
        frame_id,
        Vec2::new(20.0, 20.0),
        Vec2::new(10.0, 10.0),
        "Child",
    );

    let error = runtime
        .plan_align(&[frame_id, child], AlignMode::Left)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Arrange(ArrangeError::NestedTargets {
            ancestor: frame_id,
            descendant: child,
        })
    );
}

#[test]
fn alignment_rejects_duplicate_targets_before_touching_the_document() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(&mut runtime, root, Vec2::new(10.0, 10.0), Vec2::ZERO, "A");
    let revision = runtime.document_revision();

    let error = runtime
        .plan_align(&[first, first], AlignMode::Left)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Arrange(ArrangeError::DuplicateTarget(first))
    );
    assert_eq!(runtime.document_revision(), revision);
}

#[test]
fn a_locked_target_fails_the_whole_alignment_without_moving_anything() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let first = rectangle(&mut runtime, root, Vec2::new(20.0, 20.0), Vec2::ZERO, "A");
    let second = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 20.0),
        Vec2::new(90.0, 30.0),
        "B",
    );
    let third = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 20.0),
        Vec2::new(150.0, 70.0),
        "C",
    );
    runtime
        .dispatch(Command::SetLocked {
            target: third,
            locked: true,
        })
        .unwrap();

    let before = [first, second, third]
        .iter()
        .map(|id| runtime.document().node(*id).unwrap().local_transform())
        .collect::<Vec<_>>();
    let undo_depth = runtime.history_state().undo_depth;

    let plan = runtime
        .plan_align(&[first, second, third], AlignMode::Left)
        .unwrap();
    let error = runtime.apply_arrange(&plan).unwrap_err();
    assert!(matches!(error, RuntimeError::Editor(_)), "{error:?}");

    for (id, expected) in [first, second, third].iter().zip(before.iter()) {
        assert!(
            runtime
                .document()
                .node(*id)
                .unwrap()
                .local_transform()
                .approx_eq(*expected, DEFAULT_EPSILON),
            "node {id} moved during a failed alignment"
        );
    }
    assert_eq!(runtime.history_state().undo_depth, undo_depth);
    assert!(!runtime.transaction_active());
}

#[test]
fn alignment_inside_a_scaled_parent_uses_parent_local_translation() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group_id = NodeId::new();
    let mut group = NodeSpec::group(group_id, "Scaled group");
    group.local_transform = Affine2::from_components(2.0, 0.0, 0.0, 2.0, 100.0, 50.0);
    runtime
        .dispatch(Command::CreateNode {
            spec: group,
            parent: root,
            index: 0,
        })
        .unwrap();
    let first = rectangle(
        &mut runtime,
        group_id,
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 0.0),
        "A",
    );
    let second = rectangle(
        &mut runtime,
        group_id,
        Vec2::new(10.0, 10.0),
        Vec2::new(25.0, 30.0),
        "B",
    );

    let plan = runtime
        .plan_align(&[first, second], AlignMode::Left)
        .unwrap();
    runtime.apply_arrange(&plan).unwrap();

    // The world edges match, and the local coordinate carries half the world delta because the
    // parent scales by two.
    assert_close(world_bounds(&runtime, first).min.x, 100.0);
    assert_close(world_bounds(&runtime, second).min.x, 100.0);
    let local = runtime.document().node(second).unwrap().local_transform();
    assert_close(local.tx, 0.0);
    assert_close(local.ty, 30.0);
}
