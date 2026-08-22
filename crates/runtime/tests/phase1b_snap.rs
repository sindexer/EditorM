//! Phase 1B: snapping corrects a proposed drag position from bounded spatial candidates and
//! never treats the dragged subtree as its own snap target.

use visual_authoring_core_math::{Affine2, Rect, Vec2};
use visual_authoring_document::{Command, NodeId, NodeSpec};
use visual_authoring_runtime::{EngineRuntime, RuntimeError, SnapAxis, SnapError};

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

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::from_min_max(Vec2::new(x, y), Vec2::new(x + width, y + height))
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-9,
        "expected {expected}, found {actual}"
    );
}

#[test]
fn a_near_edge_pulls_the_proposed_bounds_onto_it() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let stationary = rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 0.0),
        "Stationary",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(50.0, 50.0),
        Vec2::new(400.0, 400.0),
        "Moving",
    );

    // The dragged rectangle would land three units right of the stationary left edge.
    let resolution = runtime
        .resolve_snap(rect(3.0, 120.0, 50.0, 50.0), &[moving], 8.0)
        .unwrap();

    assert!(resolution.snapped());
    assert_close(resolution.correction().x, -3.0);
    assert_close(resolution.correction().y, 0.0);
    let guide = resolution
        .guides()
        .iter()
        .find(|guide| guide.axis == SnapAxis::Vertical)
        .unwrap();
    assert_close(guide.position, 0.0);
    assert_eq!(guide.target, stationary);
    assert_close(guide.start, 0.0);
    assert_close(guide.end, 170.0);
}

#[test]
fn a_distant_edge_does_not_move_the_drag() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(50.0, 50.0),
        Vec2::new(400.0, 400.0),
        "Moving",
    );
    rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 0.0),
        "Stationary",
    );

    // Every edge and center pairing is more than the threshold away.
    let resolution = runtime
        .resolve_snap(rect(40.0, 200.0, 50.0, 50.0), &[moving], 8.0)
        .unwrap();
    assert!(!resolution.snapped());
    assert_close(resolution.correction().x, 0.0);
    assert_close(resolution.correction().y, 0.0);
}

#[test]
fn centers_snap_on_both_axes_at_once() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 0.0),
        "Stationary",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(20.0, 20.0),
        Vec2::new(400.0, 400.0),
        "Moving",
    );

    // Centered on (52, 48) against a stationary center of (50, 50).
    let resolution = runtime
        .resolve_snap(rect(42.0, 38.0, 20.0, 20.0), &[moving], 6.0)
        .unwrap();
    assert_eq!(resolution.guides().len(), 2);
    assert_close(resolution.correction().x, -2.0);
    assert_close(resolution.correction().y, 2.0);
}

#[test]
fn the_dragged_subtree_is_never_its_own_snap_target() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let group_id = NodeId::new();
    let mut group = NodeSpec::group(group_id, "Moving group");
    group.local_transform = Affine2::translation(Vec2::new(0.0, 0.0));
    runtime
        .dispatch(Command::CreateNode {
            spec: group,
            parent: root,
            index: 0,
        })
        .unwrap();
    rectangle(
        &mut runtime,
        group_id,
        Vec2::new(40.0, 40.0),
        Vec2::new(0.0, 0.0),
        "Inside",
    );

    let resolution = runtime
        .resolve_snap(rect(2.0, 2.0, 40.0, 40.0), &[group_id], 8.0)
        .unwrap();
    assert!(
        !resolution.snapped(),
        "the group snapped to its own child: {:?}",
        resolution.guides()
    );
    assert_eq!(resolution.candidates_considered(), 0);
}

#[test]
fn a_zero_threshold_disables_snapping_without_querying_candidates() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::ZERO,
        "Stationary",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(10.0, 10.0),
        Vec2::new(300.0, 300.0),
        "Moving",
    );

    let resolution = runtime
        .resolve_snap(rect(0.0, 0.0, 10.0, 10.0), &[moving], 0.0)
        .unwrap();
    assert!(!resolution.snapped());
    assert_eq!(resolution.candidates_examined(), 0);
}

#[test]
fn invalid_snap_input_is_a_typed_error() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let moving = NodeId::new();

    let error = runtime
        .resolve_snap(rect(0.0, 0.0, 10.0, 10.0), &[moving], f64::NAN)
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::Snap(SnapError::InvalidThreshold(_))
    ));

    let error = runtime
        .resolve_snap(
            Rect::from_min_max(Vec2::new(f64::INFINITY, 0.0), Vec2::new(1.0, 1.0)),
            &[moving],
            8.0,
        )
        .unwrap_err();
    assert_eq!(error, RuntimeError::Snap(SnapError::InvalidBounds));
}

#[test]
fn hidden_objects_are_not_snap_targets() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let hidden = rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::ZERO,
        "Hidden",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(10.0, 10.0),
        Vec2::new(300.0, 300.0),
        "Moving",
    );
    runtime
        .dispatch(Command::SetVisible {
            target: hidden,
            visible: false,
        })
        .unwrap();

    let resolution = runtime
        .resolve_snap(rect(2.0, 2.0, 10.0, 10.0), &[moving], 8.0)
        .unwrap();
    assert!(!resolution.snapped());
}

#[test]
fn the_closest_candidate_wins_deterministically() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let near = rectangle(
        &mut runtime,
        root,
        Vec2::new(4.0, 200.0),
        Vec2::new(95.0, 0.0),
        "Near edge",
    );
    rectangle(
        &mut runtime,
        root,
        Vec2::new(28.0, 200.0),
        Vec2::new(112.0, 0.0),
        "Far edge",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(10.0, 10.0),
        Vec2::new(300.0, 300.0),
        "Moving",
    );

    // Proposed x spans [100, 110]. The near rectangle ends at 99 (one unit away) and the far one
    // starts at 112 (two units away), so the near edge must win on both axes of comparison.
    let resolution = runtime
        .resolve_snap(rect(100.0, 50.0, 10.0, 10.0), &[moving], 8.0)
        .unwrap();
    assert!(resolution.snapped());
    assert_close(resolution.correction().x, -1.0);
    let guide = resolution
        .guides()
        .iter()
        .find(|guide| guide.axis == SnapAxis::Vertical)
        .unwrap();
    assert_eq!(guide.target, near);
    assert_close(guide.position, 99.0);
}

#[test]
fn an_object_outside_the_moving_band_still_provides_an_alignment_guide() {
    let mut runtime = EngineRuntime::blank("Root").unwrap();
    let root = runtime.document().root_id();
    let stationary = rectangle(
        &mut runtime,
        root,
        Vec2::new(100.0, 100.0),
        Vec2::new(0.0, 0.0),
        "Stationary",
    );
    let moving = rectangle(
        &mut runtime,
        root,
        Vec2::new(50.0, 50.0),
        Vec2::new(400.0, 400.0),
        "Moving",
    );

    // Far below the stationary rectangle, but three units off its left edge.
    let resolution = runtime
        .resolve_snap(rect(3.0, 200.0, 50.0, 50.0), &[moving], 8.0)
        .unwrap();
    assert!(resolution.snapped());
    assert_close(resolution.correction().x, -3.0);
    let guide = resolution
        .guides()
        .iter()
        .find(|guide| guide.axis == SnapAxis::Vertical)
        .unwrap();
    assert_eq!(guide.target, stationary);
    assert_close(guide.position, 0.0);
    // The guide spans both rectangles so a person can see what it lined up with.
    assert_close(guide.start, 0.0);
    assert_close(guide.end, 250.0);
}
