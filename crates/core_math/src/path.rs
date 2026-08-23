//! Shared path segment geometry.
//!
//! Every consumer of persistent path data — bounds, hit testing, tessellation, and the GPU
//! encoder — reads segments through this module. Nothing else is allowed to invent its own
//! interpretation of an anchor pair, so a curve can never be measured one way and drawn another.
//!
//! Segment rule, fixed here:
//!
//! * neither the outgoing handle of the current anchor nor the incoming handle of the next anchor
//!   is present: the segment is a straight line;
//! * otherwise the segment is a cubic Bezier with
//!   `P0 = current.position`, `P1 = current.handle_out.unwrap_or(P0)`,
//!   `P2 = next.handle_in.unwrap_or(P3)`, `P3 = next.position`.
//!
//! A closed path adds one final segment from the last anchor back to the first.
//!
//! Fill rule, fixed here: **even-odd, evaluated on the flattened outline**. The point-in-path test
//! and the triangulator both operate on the same flattened polygon, so the pixels a fill covers
//! and the region a click selects are the same region by construction. Self-intersecting outlines
//! are outside the Phase 2A fill contract: [`triangulate_fill`] reports what it could not resolve
//! instead of guessing.

use crate::{Rect, Vec2};

/// Smallest distance treated as a real separation between two points.
const POINT_EPSILON: f64 = 1.0e-12;

/// One evaluated segment between two anchors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    Line {
        start: Vec2,
        end: Vec2,
    },
    Cubic {
        start: Vec2,
        control_in: Vec2,
        control_out: Vec2,
        end: Vec2,
    },
}

impl PathSegment {
    /// Builds the segment between two anchors from the fixed rule documented above.
    #[must_use]
    pub fn between(
        start: Vec2,
        start_handle_out: Option<Vec2>,
        end_handle_in: Option<Vec2>,
        end: Vec2,
    ) -> Self {
        match (start_handle_out, end_handle_in) {
            (None, None) => Self::Line { start, end },
            (control_in, control_out) => Self::Cubic {
                start,
                control_in: control_in.unwrap_or(start),
                control_out: control_out.unwrap_or(end),
                end,
            },
        }
    }

    #[must_use]
    pub const fn start(self) -> Vec2 {
        match self {
            Self::Line { start, .. } | Self::Cubic { start, .. } => start,
        }
    }

    #[must_use]
    pub const fn end(self) -> Vec2 {
        match self {
            Self::Line { end, .. } | Self::Cubic { end, .. } => end,
        }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        match self {
            Self::Line { start, end } => start.is_finite() && end.is_finite(),
            Self::Cubic {
                start,
                control_in,
                control_out,
                end,
            } => {
                start.is_finite()
                    && control_in.is_finite()
                    && control_out.is_finite()
                    && end.is_finite()
            }
        }
    }

    /// Position at `t` in `[0, 1]`.
    #[must_use]
    pub fn evaluate(self, t: f64) -> Vec2 {
        match self {
            Self::Line { start, end } => start + (end - start) * t,
            Self::Cubic {
                start,
                control_in,
                control_out,
                end,
            } => {
                let inverse = 1.0 - t;
                let a = inverse * inverse * inverse;
                let b = 3.0 * inverse * inverse * t;
                let c = 3.0 * inverse * t * t;
                let d = t * t * t;
                Vec2::new(
                    start.x * a + control_in.x * b + control_out.x * c + end.x * d,
                    start.y * a + control_in.y * b + control_out.y * c + end.y * d,
                )
            }
        }
    }

    /// Exact bounds of the segment, including interior extrema of a cubic.
    ///
    /// The conservative control-point hull is never used here: a curve that bulges less than its
    /// handles suggest gets the tighter box it actually occupies.
    #[must_use]
    pub fn bounds(self) -> Option<Rect> {
        if !self.is_finite() {
            return None;
        }
        match self {
            Self::Line { start, end } => Rect::from_points([start, end]),
            Self::Cubic { .. } => {
                let mut points = vec![self.start(), self.end()];
                for t in self.extrema() {
                    points.push(self.evaluate(t));
                }
                Rect::from_points(points)
            }
        }
    }

    /// Parameters strictly inside `(0, 1)` where either axis reaches a local extremum.
    #[must_use]
    pub fn extrema(self) -> Vec<f64> {
        let Self::Cubic {
            start,
            control_in,
            control_out,
            end,
        } = self
        else {
            return Vec::new();
        };
        let mut roots = Vec::new();
        for (p0, p1, p2, p3) in [
            (start.x, control_in.x, control_out.x, end.x),
            (start.y, control_in.y, control_out.y, end.y),
        ] {
            // B'(t) = 3[(p1-p0)(1-t)^2 + 2(p2-p1)(1-t)t + (p3-p2)t^2], solved as a quadratic.
            let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
            let b = 2.0 * (p0 - 2.0 * p1 + p2);
            let c = p1 - p0;
            push_quadratic_roots(a, b, c, &mut roots);
        }
        roots.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        roots.dedup_by(|left, right| (*left - *right).abs() <= 1.0e-12);
        roots
    }

    /// Appends the flattened polyline for this segment, excluding its starting point.
    ///
    /// `tolerance` is a local-space distance. Flattening never depends on camera zoom, so a cached
    /// tessellation stays valid while the view moves.
    pub fn flatten_into(self, tolerance: f64, output: &mut Vec<Vec2>) {
        match self {
            Self::Line { end, .. } => output.push(end),
            Self::Cubic { .. } => {
                let steps = self.flatten_steps(tolerance);
                for step in 1..=steps {
                    #[allow(clippy::cast_precision_loss)]
                    let t = f64::from(step) / f64::from(steps);
                    output.push(self.evaluate(t));
                }
            }
        }
    }

    /// Subdivision count for the segment: derived from how far the control points deviate from the
    /// chord, clamped so a pathological curve cannot produce unbounded work.
    #[must_use]
    pub fn flatten_steps(self, tolerance: f64) -> u32 {
        const MAXIMUM_STEPS: u32 = 256;
        let Self::Cubic {
            start,
            control_in,
            control_out,
            end,
        } = self
        else {
            return 1;
        };
        let tolerance = if tolerance.is_finite() && tolerance > 0.0 {
            tolerance
        } else {
            POINT_EPSILON
        };
        // Wang's formula style bound on the deviation of the control polygon from the chord.
        let deviation = (control_in * 2.0 - start - control_out)
            .max_component_absolute()
            .max((control_out * 2.0 - control_in - end).max_component_absolute());
        if !deviation.is_finite() || deviation <= tolerance {
            return 1;
        }
        let steps = (3.0 * deviation / (4.0 * tolerance)).sqrt().ceil();
        if !steps.is_finite() || steps <= 1.0 {
            return 1;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = steps as u32;
        steps.clamp(1, MAXIMUM_STEPS)
    }

    /// Shortest distance from `point` to the segment, measured on the flattened polyline so it
    /// agrees with what the renderer draws.
    #[must_use]
    pub fn distance_to(self, point: Vec2, tolerance: f64) -> f64 {
        match self {
            Self::Line { start, end } => distance_to_line(point, start, end),
            Self::Cubic { .. } => {
                let mut polyline = vec![self.start()];
                self.flatten_into(tolerance, &mut polyline);
                polyline
                    .windows(2)
                    .map(|pair| distance_to_line(point, pair[0], pair[1]))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }
}

fn push_quadratic_roots(a: f64, b: f64, c: f64, output: &mut Vec<f64>) {
    let inside = |t: f64| t.is_finite() && t > 0.0 && t < 1.0;
    if a.abs() <= 1.0e-12 {
        if b.abs() > 1.0e-12 {
            let t = -c / b;
            if inside(t) {
                output.push(t);
            }
        }
        return;
    }
    let discriminant = b * b - 4.0 * a * c;
    if !discriminant.is_finite() || discriminant < 0.0 {
        return;
    }
    let root = discriminant.sqrt();
    for t in [(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)] {
        if inside(t) {
            output.push(t);
        }
    }
}

fn distance_to_line(point: Vec2, start: Vec2, end: Vec2) -> f64 {
    let direction = end - start;
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if length_squared <= POINT_EPSILON {
        return (point - start).length();
    }
    let offset = point - start;
    let t = ((offset.x * direction.x + offset.y * direction.y) / length_squared).clamp(0.0, 1.0);
    (point - (start + direction * t)).length()
}

/// A flattened path outline: the polyline the renderer draws and the hit test measures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlattenedPath {
    pub points: Vec<Vec2>,
    pub closed: bool,
}

impl FlattenedPath {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.len() < 2
    }

    /// Edges of the outline. A closed outline includes the wrap-around edge.
    pub fn edges(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        let count = self.points.len();
        let wrap = usize::from(self.closed && count > 2);
        (0..count.saturating_sub(1) + wrap)
            .map(move |index| (self.points[index], self.points[(index + 1) % count]))
    }

    /// Shortest distance from `point` to the outline.
    #[must_use]
    pub fn distance_to(&self, point: Vec2) -> f64 {
        self.edges()
            .map(|(start, end)| distance_to_line(point, start, end))
            .fold(f64::INFINITY, f64::min)
    }

    /// Even-odd containment, matching the fill rule this module fixes.
    ///
    /// Only a closed outline has an interior; an open path is never filled and never reports a
    /// point as inside.
    #[must_use]
    pub fn contains(&self, point: Vec2) -> bool {
        if !self.closed || self.points.len() < 3 || !point.is_finite() {
            return false;
        }
        let mut inside = false;
        for (start, end) in self.edges() {
            let crosses = (start.y > point.y) != (end.y > point.y);
            if !crosses {
                continue;
            }
            let span = end.y - start.y;
            if span.abs() <= POINT_EPSILON {
                continue;
            }
            let x = start.x + (point.y - start.y) / span * (end.x - start.x);
            if x > point.x {
                inside = !inside;
            }
        }
        inside
    }

    /// Whether any two non-adjacent edges of the outline cross.
    ///
    /// Phase 2A fills only outlines that do not intersect themselves, so this is the single check
    /// that decides both what the tessellator emits and what the hit test reports as interior.
    #[must_use]
    pub fn has_self_intersection(&self) -> bool {
        let edges = self.edges().collect::<Vec<_>>();
        for (first, (a_start, a_end)) in edges.iter().enumerate() {
            for (second, (b_start, b_end)) in edges.iter().enumerate().skip(first + 1) {
                let adjacent =
                    second == first + 1 || (first == 0 && second + 1 == edges.len() && self.closed);
                if adjacent {
                    continue;
                }
                if segments_cross(*a_start, *a_end, *b_start, *b_end) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether this outline encloses a fillable region under the Phase 2A fill contract.
    #[must_use]
    pub fn is_fillable(&self) -> bool {
        self.closed && self.points.len() >= 3 && !self.has_self_intersection()
    }

    /// Signed area, positive when the outline winds counter-clockwise in a +Y-down space.
    #[must_use]
    pub fn signed_area(&self) -> f64 {
        let mut total = 0.0;
        for (start, end) in self.edges() {
            total += start.x * end.y - end.x * start.y;
        }
        total * 0.5
    }
}

/// Why a fill could not be triangulated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillTriangulationError {
    /// The outline is open, degenerate, or has fewer than three usable points.
    NotFillable,
    /// Ear clipping stalled, which means the flattened outline intersects itself. Phase 2A does
    /// not fill self-intersecting outlines; it refuses rather than inventing a covered region that
    /// the hit test would disagree with.
    SelfIntersecting,
}

/// Ear-clipping triangulation of a closed, non-self-intersecting outline.
///
/// Returns indices into `outline.points`, three per triangle.
pub fn triangulate_fill(outline: &FlattenedPath) -> Result<Vec<u32>, FillTriangulationError> {
    if !outline.closed || outline.points.len() < 3 {
        return Err(FillTriangulationError::NotFillable);
    }
    if outline.has_self_intersection() {
        return Err(FillTriangulationError::SelfIntersecting);
    }
    let mut remaining: Vec<usize> = (0..outline.points.len()).collect();
    // Drop repeated points so a duplicated anchor cannot create a zero-area ear.
    remaining.retain(|index| {
        let previous = outline.points[(*index + outline.points.len() - 1) % outline.points.len()];
        (outline.points[*index] - previous).length() > POINT_EPSILON
    });
    if remaining.len() < 3 {
        return Err(FillTriangulationError::NotFillable);
    }
    let counter_clockwise = outline.signed_area() > 0.0;
    let mut triangles = Vec::with_capacity((remaining.len() - 2) * 3);
    let mut guard = remaining.len() * remaining.len() + 8;

    while remaining.len() > 3 {
        let count = remaining.len();
        let mut clipped = false;
        for position in 0..count {
            let previous = remaining[(position + count - 1) % count];
            let current = remaining[position];
            let next = remaining[(position + 1) % count];
            if is_ear(
                outline,
                &remaining,
                previous,
                current,
                next,
                counter_clockwise,
            ) {
                triangles.extend_from_slice(&[previous as u32, current as u32, next as u32]);
                remaining.remove(position);
                clipped = true;
                break;
            }
        }
        guard = guard.saturating_sub(1);
        if !clipped || guard == 0 {
            return Err(FillTriangulationError::SelfIntersecting);
        }
    }
    triangles.extend_from_slice(&[
        remaining[0] as u32,
        remaining[1] as u32,
        remaining[2] as u32,
    ]);
    Ok(triangles)
}

fn is_ear(
    outline: &FlattenedPath,
    remaining: &[usize],
    previous: usize,
    current: usize,
    next: usize,
    counter_clockwise: bool,
) -> bool {
    let a = outline.points[previous];
    let b = outline.points[current];
    let c = outline.points[next];
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if cross.abs() <= POINT_EPSILON {
        return false;
    }
    if (cross > 0.0) != counter_clockwise {
        return false;
    }
    remaining
        .iter()
        .filter(|index| **index != previous && **index != current && **index != next)
        .all(|index| !point_in_triangle(outline.points[*index], a, b, c))
}

/// Proper crossing test: shared endpoints and collinear touching do not count as an intersection.
fn segments_cross(a_start: Vec2, a_end: Vec2, b_start: Vec2, b_end: Vec2) -> bool {
    let orientation = |p: Vec2, q: Vec2, r: Vec2| {
        let value = (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
        if value > POINT_EPSILON {
            1
        } else if value < -POINT_EPSILON {
            -1
        } else {
            0
        }
    };
    let first = orientation(a_start, a_end, b_start);
    let second = orientation(a_start, a_end, b_end);
    let third = orientation(b_start, b_end, a_start);
    let fourth = orientation(b_start, b_end, a_end);
    first != 0 && second != 0 && third != 0 && fourth != 0 && first != second && third != fourth
}

fn point_in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let sign = |p: Vec2, q: Vec2, r: Vec2| (p.x - r.x) * (q.y - r.y) - (q.x - r.x) * (p.y - r.y);
    let first = sign(point, a, b);
    let second = sign(point, b, c);
    let third = sign(point, c, a);
    let has_negative = first < 0.0 || second < 0.0 || third < 0.0;
    let has_positive = first > 0.0 || second > 0.0 || third > 0.0;
    !(has_negative && has_positive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cubic(start: Vec2, control_in: Vec2, control_out: Vec2, end: Vec2) -> PathSegment {
        PathSegment::Cubic {
            start,
            control_in,
            control_out,
            end,
        }
    }

    #[test]
    fn missing_handles_produce_a_straight_segment() {
        let segment = PathSegment::between(Vec2::ZERO, None, None, Vec2::new(10.0, 0.0));
        assert_eq!(
            segment,
            PathSegment::Line {
                start: Vec2::ZERO,
                end: Vec2::new(10.0, 0.0)
            }
        );
        assert_eq!(segment.evaluate(0.5), Vec2::new(5.0, 0.0));
    }

    #[test]
    fn one_handle_promotes_the_segment_to_a_cubic_with_collapsed_controls() {
        let segment = PathSegment::between(
            Vec2::ZERO,
            Some(Vec2::new(0.0, 10.0)),
            None,
            Vec2::new(10.0, 0.0),
        );
        assert_eq!(
            segment,
            cubic(
                Vec2::ZERO,
                Vec2::new(0.0, 10.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 0.0),
            )
        );
    }

    #[test]
    fn cubic_bounds_use_extrema_rather_than_the_control_hull() {
        // Handles reach y = 12 but the curve only reaches y = 9.
        let segment = cubic(
            Vec2::ZERO,
            Vec2::new(0.0, 12.0),
            Vec2::new(10.0, 12.0),
            Vec2::new(10.0, 0.0),
        );
        let bounds = segment.bounds().unwrap();
        assert!((bounds.max.y - 9.0).abs() < 1.0e-9, "{bounds:?}");
        assert!((bounds.min.y - 0.0).abs() < 1.0e-9, "{bounds:?}");
        assert!((bounds.min.x - 0.0).abs() < 1.0e-9, "{bounds:?}");
        assert!((bounds.max.x - 10.0).abs() < 1.0e-9, "{bounds:?}");
    }

    #[test]
    fn a_straight_cubic_has_no_interior_extrema() {
        let segment = cubic(
            Vec2::ZERO,
            Vec2::new(3.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(9.0, 0.0),
        );
        assert!(segment.extrema().is_empty());
        assert_eq!(segment.flatten_steps(0.1), 1);
    }

    #[test]
    fn a_zero_length_segment_is_stable() {
        let segment = PathSegment::between(Vec2::ZERO, None, None, Vec2::ZERO);
        assert_eq!(
            segment.bounds().unwrap(),
            Rect::from_min_max(Vec2::ZERO, Vec2::ZERO)
        );
        assert_eq!(segment.distance_to(Vec2::new(3.0, 4.0), 0.1), 5.0);
    }

    #[test]
    fn non_finite_segments_have_no_bounds() {
        let segment = PathSegment::Line {
            start: Vec2::new(f64::NAN, 0.0),
            end: Vec2::ZERO,
        };
        assert!(segment.bounds().is_none());
        assert!(!segment.is_finite());
    }

    #[test]
    fn flattening_is_bounded_and_deterministic() {
        let segment = cubic(
            Vec2::ZERO,
            Vec2::new(0.0, 1.0e6),
            Vec2::new(1.0e6, 1.0e6),
            Vec2::new(1.0e6, 0.0),
        );
        assert_eq!(segment.flatten_steps(1.0e-9), 256);
        let mut first = vec![segment.start()];
        let mut second = vec![segment.start()];
        segment.flatten_into(0.25, &mut first);
        segment.flatten_into(0.25, &mut second);
        assert_eq!(first, second);
    }

    #[test]
    fn even_odd_containment_matches_the_flattened_outline() {
        let outline = FlattenedPath {
            points: vec![
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(0.0, 10.0),
            ],
            closed: true,
        };
        assert!(outline.contains(Vec2::new(5.0, 5.0)));
        assert!(!outline.contains(Vec2::new(-1.0, 5.0)));
        assert!(!outline.contains(Vec2::new(15.0, 5.0)));
        assert!((outline.distance_to(Vec2::new(5.0, 12.0)) - 2.0).abs() < 1.0e-9);
    }

    #[test]
    fn an_open_outline_has_no_interior() {
        let outline = FlattenedPath {
            points: vec![Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)],
            closed: false,
        };
        assert!(!outline.contains(Vec2::new(9.0, 5.0)));
        assert_eq!(
            triangulate_fill(&outline),
            Err(FillTriangulationError::NotFillable)
        );
    }

    #[test]
    fn ear_clipping_covers_a_simple_outline_exactly_once() {
        let outline = FlattenedPath {
            points: vec![
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(5.0, 4.0),
                Vec2::new(0.0, 10.0),
            ],
            closed: true,
        };
        let triangles = triangulate_fill(&outline).unwrap();
        assert_eq!(triangles.len(), (outline.points.len() - 2) * 3);
        let area: f64 = triangles
            .chunks_exact(3)
            .map(|triangle| {
                let a = outline.points[triangle[0] as usize];
                let b = outline.points[triangle[1] as usize];
                let c = outline.points[triangle[2] as usize];
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() * 0.5
            })
            .sum();
        assert!(
            (area - outline.signed_area().abs()).abs() < 1.0e-9,
            "{area}"
        );
    }

    #[test]
    fn a_simple_outline_reports_no_self_intersection() {
        let outline = FlattenedPath {
            points: vec![
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(0.0, 10.0),
            ],
            closed: true,
        };
        assert!(!outline.has_self_intersection());
        assert!(outline.is_fillable());
    }

    #[test]
    fn a_self_intersecting_outline_is_refused_rather_than_guessed() {
        let outline = FlattenedPath {
            points: vec![
                Vec2::ZERO,
                Vec2::new(10.0, 10.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(0.0, 10.0),
            ],
            closed: true,
        };
        assert!(outline.has_self_intersection());
        assert!(!outline.is_fillable());
        assert_eq!(
            triangulate_fill(&outline),
            Err(FillTriangulationError::SelfIntersecting)
        );
    }
}
