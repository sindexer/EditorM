//! Alignment and distribution planning for multi-node selections.
//!
//! Planning is a pure function of the persistent document and the computed scene. It never
//! mutates state: it produces typed [`Command`] values that the caller applies inside one
//! transaction, so a whole alignment is one atomic, reversible history entry.
//!
//! Reference geometry is world space, but every produced command carries a *parent-local*
//! transform. A world translation `d` applied to a node whose parent has world transform `P`
//! is expressed locally as `P_linear^-1 * d`, which leaves rotation and scale untouched.

use std::collections::BTreeSet;

use thiserror::Error;
use visual_authoring_core_math::{Affine2, Rect, Vec2};
use visual_authoring_document::{Command, Document, NodeId};
use visual_authoring_scene::ComputedScene;

/// Geometric axis a single arrange operation acts on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Edge or center an alignment snaps its targets to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlignMode {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

impl AlignMode {
    #[must_use]
    pub const fn axis(self) -> Axis {
        match self {
            Self::Left | Self::HorizontalCenter | Self::Right => Axis::Horizontal,
            Self::Top | Self::VerticalCenter | Self::Bottom => Axis::Vertical,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "align_left",
            Self::HorizontalCenter => "align_horizontal_center",
            Self::Right => "align_right",
            Self::Top => "align_top",
            Self::VerticalCenter => "align_vertical_center",
            Self::Bottom => "align_bottom",
        }
    }

    fn target_value(self, reference: Rect) -> f64 {
        match self {
            Self::Left => reference.min.x,
            Self::HorizontalCenter => reference.center().x,
            Self::Right => reference.max.x,
            Self::Top => reference.min.y,
            Self::VerticalCenter => reference.center().y,
            Self::Bottom => reference.max.y,
        }
    }

    fn current_value(self, bounds: Rect) -> f64 {
        self.target_value(bounds)
    }
}

/// Axis along which equal spacing is produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
}

impl DistributeAxis {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "distribute_horizontal",
            Self::Vertical => "distribute_vertical",
        }
    }

    const fn axis(self) -> Axis {
        match self {
            Self::Horizontal => Axis::Horizontal,
            Self::Vertical => Axis::Vertical,
        }
    }
}

/// Minimum target count for an alignment.
pub const MINIMUM_ALIGN_TARGETS: usize = 2;
/// Minimum target count for a distribution.
pub const MINIMUM_DISTRIBUTE_TARGETS: usize = 3;

/// Smallest world-space movement an arrange operation still emits a command for.
const ARRANGE_EPSILON: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum ArrangeError {
    #[error("{operation} requires at least {required} targets but received {actual}")]
    InsufficientTargets {
        operation: &'static str,
        required: usize,
        actual: usize,
    },
    #[error("target {0} appears more than once")]
    DuplicateTarget(NodeId),
    #[error("target {0} is missing from the document")]
    MissingDocumentNode(NodeId),
    #[error("target {0} is missing from the computed scene")]
    MissingSceneNode(NodeId),
    #[error("target {0} is not attached to the document root")]
    DetachedTarget(NodeId),
    #[error("the document root {0} cannot be arranged")]
    RootTarget(NodeId),
    #[error("target {0} has no resolved world bounds")]
    UnresolvedBounds(NodeId),
    #[error("target {descendant} is inside target {ancestor}")]
    NestedTargets {
        ancestor: NodeId,
        descendant: NodeId,
    },
    #[error("the parent of target {target} has no invertible world transform")]
    NonInvertibleParent { target: NodeId },
    #[error("arranging target {0} would produce a non-finite transform")]
    NonFiniteResult(NodeId),
}

/// One planned movement expressed as the node's next parent-local transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrangeMove {
    pub target: NodeId,
    pub local_transform: Affine2,
    pub world_delta: Vec2,
}

/// Validated, immutable result of alignment or distribution planning.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangePlan {
    operation: &'static str,
    reference: Rect,
    moves: Vec<ArrangeMove>,
    targets_examined: u64,
    unchanged_targets: u64,
}

impl ArrangePlan {
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// World-space bounds the operation aligned or distributed within.
    #[must_use]
    pub const fn reference(&self) -> Rect {
        self.reference
    }

    #[must_use]
    pub fn moves(&self) -> &[ArrangeMove] {
        &self.moves
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Number of targets whose geometry was read while planning.
    #[must_use]
    pub const fn targets_examined(&self) -> u64 {
        self.targets_examined
    }

    /// Targets that were already in place and therefore produce no command.
    #[must_use]
    pub const fn unchanged_targets(&self) -> u64 {
        self.unchanged_targets
    }

    /// Typed commands for this plan, in stable target order.
    #[must_use]
    pub fn commands(&self) -> Vec<Command> {
        self.moves
            .iter()
            .map(|entry| Command::SetLocalTransform {
                target: entry.target,
                transform: entry.local_transform,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
struct TargetGeometry {
    id: NodeId,
    bounds: Rect,
    local_transform: Affine2,
    parent_world: Affine2,
}

impl TargetGeometry {
    fn size(self, axis: Axis) -> f64 {
        match axis {
            Axis::Horizontal => self.bounds.width(),
            Axis::Vertical => self.bounds.height(),
        }
    }

    fn minimum(self, axis: Axis) -> f64 {
        match axis {
            Axis::Horizontal => self.bounds.min.x,
            Axis::Vertical => self.bounds.min.y,
        }
    }

    fn maximum(self, axis: Axis) -> f64 {
        match axis {
            Axis::Horizontal => self.bounds.max.x,
            Axis::Vertical => self.bounds.max.y,
        }
    }
}

/// Plans an alignment of `targets` against their shared selection bounds.
pub fn plan_align(
    document: &Document,
    scene: &ComputedScene,
    targets: &[NodeId],
    mode: AlignMode,
) -> Result<ArrangePlan, ArrangeError> {
    let geometry = collect_targets(
        document,
        scene,
        targets,
        mode.label(),
        MINIMUM_ALIGN_TARGETS,
    )?;
    let reference = union_bounds(&geometry);
    let target_value = mode.target_value(reference);
    let axis = mode.axis();
    let mut deltas = Vec::with_capacity(geometry.len());
    for entry in &geometry {
        let offset = target_value - mode.current_value(entry.bounds);
        deltas.push(axis_delta(axis, offset));
    }
    build_plan(mode.label(), reference, &geometry, &deltas)
}

/// Plans equal edge-to-edge spacing between `targets` along one axis.
///
/// The extreme targets keep their positions; the interior targets are placed so that every gap
/// is identical. Overlapping input produces a negative gap rather than a rejected request, which
/// keeps the operation total and predictable.
pub fn plan_distribute(
    document: &Document,
    scene: &ComputedScene,
    targets: &[NodeId],
    axis: DistributeAxis,
) -> Result<ArrangePlan, ArrangeError> {
    let geometry = collect_targets(
        document,
        scene,
        targets,
        axis.label(),
        MINIMUM_DISTRIBUTE_TARGETS,
    )?;
    let reference = union_bounds(&geometry);
    let geometric_axis = axis.axis();

    let mut order = (0..geometry.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        let left_entry = geometry[*left];
        let right_entry = geometry[*right];
        left_entry
            .minimum(geometric_axis)
            .partial_cmp(&right_entry.minimum(geometric_axis))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_entry.id.cmp(&right_entry.id))
    });

    let span = geometry
        .iter()
        .map(|entry| entry.maximum(geometric_axis))
        .fold(f64::NEG_INFINITY, f64::max)
        - geometry
            .iter()
            .map(|entry| entry.minimum(geometric_axis))
            .fold(f64::INFINITY, f64::min);
    let occupied = geometry
        .iter()
        .map(|entry| entry.size(geometric_axis))
        .sum::<f64>();
    #[allow(clippy::cast_precision_loss)]
    let gaps = (geometry.len() - 1) as f64;
    let gap = (span - occupied) / gaps;

    let mut deltas = vec![Vec2::ZERO; geometry.len()];
    let mut cursor = geometry[order[0]].minimum(geometric_axis);
    for index in order {
        let entry = geometry[index];
        deltas[index] = axis_delta(geometric_axis, cursor - entry.minimum(geometric_axis));
        cursor += entry.size(geometric_axis) + gap;
    }
    build_plan(axis.label(), reference, &geometry, &deltas)
}

const fn axis_delta(axis: Axis, offset: f64) -> Vec2 {
    match axis {
        Axis::Horizontal => Vec2::new(offset, 0.0),
        Axis::Vertical => Vec2::new(0.0, offset),
    }
}

fn union_bounds(geometry: &[TargetGeometry]) -> Rect {
    let mut bounds = geometry[0].bounds;
    for entry in &geometry[1..] {
        bounds.min.x = bounds.min.x.min(entry.bounds.min.x);
        bounds.min.y = bounds.min.y.min(entry.bounds.min.y);
        bounds.max.x = bounds.max.x.max(entry.bounds.max.x);
        bounds.max.y = bounds.max.y.max(entry.bounds.max.y);
    }
    bounds
}

fn build_plan(
    operation: &'static str,
    reference: Rect,
    geometry: &[TargetGeometry],
    deltas: &[Vec2],
) -> Result<ArrangePlan, ArrangeError> {
    let mut moves = Vec::new();
    let mut unchanged = 0_u64;
    for (entry, delta) in geometry.iter().zip(deltas.iter()) {
        if delta.x.abs() <= ARRANGE_EPSILON && delta.y.abs() <= ARRANGE_EPSILON {
            unchanged += 1;
            continue;
        }
        let local_delta = world_delta_to_local(entry.parent_world, *delta)
            .ok_or(ArrangeError::NonInvertibleParent { target: entry.id })?;
        let local_transform = translated(entry.local_transform, local_delta);
        if !local_transform.is_finite() {
            return Err(ArrangeError::NonFiniteResult(entry.id));
        }
        moves.push(ArrangeMove {
            target: entry.id,
            local_transform,
            world_delta: *delta,
        });
    }
    Ok(ArrangePlan {
        operation,
        reference,
        moves,
        targets_examined: geometry.len() as u64,
        unchanged_targets: unchanged,
    })
}

/// Expresses a world-space translation in the coordinate space of `parent_world`.
pub fn world_delta_to_local(parent_world: Affine2, delta: Vec2) -> Option<Vec2> {
    let inverse = parent_world.inverse()?;
    let local = inverse.transform_vector(delta);
    local.is_finite().then_some(local)
}

/// Applies a parent-local translation without disturbing rotation or scale.
pub const fn translated(local: Affine2, delta: Vec2) -> Affine2 {
    Affine2::from_components(
        local.m11,
        local.m12,
        local.m21,
        local.m22,
        local.tx + delta.x,
        local.ty + delta.y,
    )
}

fn collect_targets(
    document: &Document,
    scene: &ComputedScene,
    targets: &[NodeId],
    operation: &'static str,
    required: usize,
) -> Result<Vec<TargetGeometry>, ArrangeError> {
    if targets.len() < required {
        return Err(ArrangeError::InsufficientTargets {
            operation,
            required,
            actual: targets.len(),
        });
    }
    let mut unique = BTreeSet::new();
    for id in targets {
        if !unique.insert(*id) {
            return Err(ArrangeError::DuplicateTarget(*id));
        }
    }
    let mut geometry = Vec::with_capacity(targets.len());
    for id in targets {
        let id = *id;
        if id == document.root_id() {
            return Err(ArrangeError::RootTarget(id));
        }
        let node = document
            .node(id)
            .ok_or(ArrangeError::MissingDocumentNode(id))?;
        let scene_node = scene.node(id).ok_or(ArrangeError::MissingSceneNode(id))?;
        if !scene_node.attached() {
            return Err(ArrangeError::DetachedTarget(id));
        }
        let bounds = scene_node
            .subtree_world_bounds()
            .or_else(|| scene_node.own_world_bounds())
            .filter(|bounds| Rect::is_finite(*bounds))
            .ok_or(ArrangeError::UnresolvedBounds(id))?;
        let parent = node.parent().ok_or(ArrangeError::RootTarget(id))?;
        reject_nested(document, id, parent, &unique)?;
        let parent_world = scene
            .node(parent)
            .ok_or(ArrangeError::MissingSceneNode(parent))?
            .world_transform()
            .filter(|transform| Affine2::is_finite(*transform))
            .ok_or(ArrangeError::NonInvertibleParent { target: id })?;
        geometry.push(TargetGeometry {
            id,
            bounds,
            local_transform: node.local_transform(),
            parent_world,
        });
    }
    Ok(geometry)
}

/// Rejects a selection that contains both a container and something inside it, because such a
/// target would otherwise be moved twice by one operation.
fn reject_nested(
    document: &Document,
    target: NodeId,
    parent: NodeId,
    selected: &BTreeSet<NodeId>,
) -> Result<(), ArrangeError> {
    let mut current = Some(parent);
    let mut visited = BTreeSet::new();
    while let Some(id) = current {
        if !visited.insert(id) {
            return Err(ArrangeError::MissingDocumentNode(id));
        }
        if selected.contains(&id) {
            return Err(ArrangeError::NestedTargets {
                ancestor: id,
                descendant: target,
            });
        }
        current = document
            .node(id)
            .ok_or(ArrangeError::MissingDocumentNode(id))?
            .parent();
    }
    Ok(())
}
