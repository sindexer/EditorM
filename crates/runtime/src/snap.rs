//! Edge and center snapping for direct manipulation.
//!
//! Snapping never mutates the document. It answers one question: given the world bounds a
//! dragged selection *would* occupy, which small correction puts one of its edges or its center
//! exactly on an edge or center of a nearby object?
//!
//! Candidates come from the scene's spatial index, queried as two thin bands: one covering the
//! proposed horizontal extent grown by the threshold, one covering the vertical extent. Each band
//! is clipped to the visible world viewport, so an object anywhere on screen can still produce an
//! alignment guide while the examined candidate count stays proportional to what is near the
//! dragged bounds, not to the document size.

use std::collections::BTreeSet;

use thiserror::Error;
use visual_authoring_core_math::{Rect, Vec2};
use visual_authoring_document::{Document, NodeId};
use visual_authoring_scene::{ComputedScene, SceneError};

/// Axis a snap correction acts on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapAxis {
    /// A vertical guide line at a constant world `x`.
    Vertical,
    /// A horizontal guide line at a constant world `y`.
    Horizontal,
}

impl SnapAxis {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// Which feature of a rectangle produced the match.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SnapFeature {
    Minimum,
    Center,
    Maximum,
}

impl SnapFeature {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Minimum => "minimum",
            Self::Center => "center",
            Self::Maximum => "maximum",
        }
    }

    /// Edge matches rank before center matches when two corrections are equally close.
    const fn priority(self) -> u8 {
        match self {
            Self::Minimum | Self::Maximum => 0,
            Self::Center => 1,
        }
    }
}

/// One accepted snap, including the guide segment the editor draws for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    pub axis: SnapAxis,
    pub position: f64,
    pub start: f64,
    pub end: f64,
    pub moving_feature: SnapFeature,
    pub target_feature: SnapFeature,
    pub target: NodeId,
    pub correction: f64,
}

/// Result of one snap query.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapResolution {
    correction: Vec2,
    guides: Vec<SnapGuide>,
    candidates_examined: u64,
    candidates_considered: u64,
}

impl SnapResolution {
    /// World-space correction to add to the requested translation.
    #[must_use]
    pub const fn correction(&self) -> Vec2 {
        self.correction
    }

    #[must_use]
    pub fn guides(&self) -> &[SnapGuide] {
        &self.guides
    }

    #[must_use]
    pub fn snapped(&self) -> bool {
        !self.guides.is_empty()
    }

    /// Entries the spatial index returned for the grown query rectangle.
    #[must_use]
    pub const fn candidates_examined(&self) -> u64 {
        self.candidates_examined
    }

    /// Entries that survived visibility and ancestry filtering.
    #[must_use]
    pub const fn candidates_considered(&self) -> u64 {
        self.candidates_considered
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SnapError {
    #[error("snap threshold {0} is not a finite, non-negative distance")]
    InvalidThreshold(f64),
    #[error("proposed bounds are not finite")]
    InvalidBounds,
    #[error("scene query failed: {0}")]
    Scene(#[from] SceneError),
}

#[derive(Clone, Copy)]
struct Correction {
    delta: f64,
    guide: SnapGuide,
}

/// Resolves the snap correction for `proposed` bounds while `moving` is being dragged.
///
/// `moving` and everything inside it is excluded from the candidate set, because a dragged
/// object must not snap to itself or to its own children. `viewport` bounds the search to what
/// a person can actually see lining up.
pub fn resolve(
    document: &Document,
    scene: &mut ComputedScene,
    proposed: Rect,
    moving: &[NodeId],
    threshold: f64,
    viewport: Rect,
) -> Result<SnapResolution, SnapError> {
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(SnapError::InvalidThreshold(threshold));
    }
    if !proposed.is_finite() || !viewport.is_finite() {
        return Err(SnapError::InvalidBounds);
    }
    if threshold == 0.0 {
        return Ok(SnapResolution::default());
    }

    let moving_set = moving.iter().copied().collect::<BTreeSet<_>>();
    let mut resolution = SnapResolution::default();
    let mut vertical: Option<Correction> = None;
    let mut horizontal: Option<Correction> = None;

    let vertical_band = Rect::from_min_max(
        Vec2::new(proposed.min.x - threshold, viewport.min.y),
        Vec2::new(proposed.max.x + threshold, viewport.max.y),
    );
    accumulate_band(
        document,
        scene,
        SnapAxis::Vertical,
        vertical_band,
        proposed,
        &moving_set,
        threshold,
        &mut vertical,
        &mut resolution,
    )?;

    let horizontal_band = Rect::from_min_max(
        Vec2::new(viewport.min.x, proposed.min.y - threshold),
        Vec2::new(viewport.max.x, proposed.max.y + threshold),
    );
    accumulate_band(
        document,
        scene,
        SnapAxis::Horizontal,
        horizontal_band,
        proposed,
        &moving_set,
        threshold,
        &mut horizontal,
        &mut resolution,
    )?;

    if let Some(entry) = vertical {
        resolution.correction.x = entry.delta;
        resolution.guides.push(entry.guide);
    }
    if let Some(entry) = horizontal {
        resolution.correction.y = entry.delta;
        resolution.guides.push(entry.guide);
    }
    Ok(resolution)
}

#[allow(clippy::too_many_arguments)]
fn accumulate_band(
    document: &Document,
    scene: &mut ComputedScene,
    axis: SnapAxis,
    band: Rect,
    proposed: Rect,
    moving: &BTreeSet<NodeId>,
    threshold: f64,
    best: &mut Option<Correction>,
    resolution: &mut SnapResolution,
) -> Result<(), SnapError> {
    let candidates = scene.query_rect_candidates(band)?;
    resolution.candidates_examined += candidates.ids().len() as u64;
    for id in candidates.ids() {
        let id = *id;
        if !is_snap_target(document, scene, id, moving) {
            continue;
        }
        let Some(bounds) = scene
            .node(id)
            .and_then(visual_authoring_scene::SceneNode::own_world_bounds)
            .filter(|bounds| Rect::is_finite(*bounds))
        else {
            continue;
        };
        resolution.candidates_considered += 1;
        best_axis_correction(axis, proposed, bounds, id, threshold, best);
    }
    Ok(())
}

const FEATURES: [SnapFeature; 3] = [
    SnapFeature::Minimum,
    SnapFeature::Center,
    SnapFeature::Maximum,
];

fn best_axis_correction(
    axis: SnapAxis,
    moving: Rect,
    target: Rect,
    target_id: NodeId,
    threshold: f64,
    best: &mut Option<Correction>,
) {
    for moving_feature in FEATURES {
        let moving_value = feature_value(axis, moving, moving_feature);
        for target_feature in FEATURES {
            let target_value = feature_value(axis, target, target_feature);
            let delta = target_value - moving_value;
            if delta.abs() > threshold {
                continue;
            }
            let candidate = Correction {
                delta,
                guide: SnapGuide {
                    axis,
                    position: target_value,
                    start: cross_minimum(axis, moving).min(cross_minimum(axis, target)),
                    end: cross_maximum(axis, moving).max(cross_maximum(axis, target)),
                    moving_feature,
                    target_feature,
                    target: target_id,
                    correction: delta,
                },
            };
            if is_better(&candidate, best.as_ref()) {
                *best = Some(candidate);
            }
        }
    }
}

/// Deterministic preference order: smaller correction, then edges over centers, then the lower
/// guide position, then stable node identity.
fn is_better(candidate: &Correction, current: Option<&Correction>) -> bool {
    let Some(current) = current else {
        return true;
    };
    let candidate_key = (
        candidate.delta.abs(),
        candidate.guide.moving_feature.priority() + candidate.guide.target_feature.priority(),
        candidate.guide.position,
    );
    let current_key = (
        current.delta.abs(),
        current.guide.moving_feature.priority() + current.guide.target_feature.priority(),
        current.guide.position,
    );
    match candidate_key.partial_cmp(&current_key) {
        Some(std::cmp::Ordering::Less) => true,
        Some(std::cmp::Ordering::Greater) | None => false,
        Some(std::cmp::Ordering::Equal) => candidate.guide.target < current.guide.target,
    }
}

const fn feature_value(axis: SnapAxis, bounds: Rect, feature: SnapFeature) -> f64 {
    let (minimum, maximum) = match axis {
        SnapAxis::Vertical => (bounds.min.x, bounds.max.x),
        SnapAxis::Horizontal => (bounds.min.y, bounds.max.y),
    };
    match feature {
        SnapFeature::Minimum => minimum,
        SnapFeature::Center => (minimum + maximum) * 0.5,
        SnapFeature::Maximum => maximum,
    }
}

const fn cross_minimum(axis: SnapAxis, bounds: Rect) -> f64 {
    match axis {
        SnapAxis::Vertical => bounds.min.y,
        SnapAxis::Horizontal => bounds.min.x,
    }
}

const fn cross_maximum(axis: SnapAxis, bounds: Rect) -> f64 {
    match axis {
        SnapAxis::Vertical => bounds.max.y,
        SnapAxis::Horizontal => bounds.max.x,
    }
}

/// A candidate must be attached, visible, and outside the dragged subtrees. Locked objects
/// stay valid snap targets: they are visible geometry a person still aligns against.
fn is_snap_target(
    document: &Document,
    scene: &ComputedScene,
    id: NodeId,
    moving: &BTreeSet<NodeId>,
) -> bool {
    if id == document.root_id() || moving.contains(&id) {
        return false;
    }
    let Some(scene_node) = scene.node(id) else {
        return false;
    };
    if !scene_node.attached() || !scene_node.effective_visible() {
        return false;
    }
    let mut current = document
        .node(id)
        .and_then(visual_authoring_document::Node::parent);
    let mut depth = 0_usize;
    while let Some(parent) = current {
        if moving.contains(&parent) {
            return false;
        }
        depth += 1;
        if depth > document.len() {
            return false;
        }
        current = document
            .node(parent)
            .and_then(visual_authoring_document::Node::parent);
    }
    true
}
