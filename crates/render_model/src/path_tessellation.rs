//! Derived path tessellation.
//!
//! Persistent documents store anchors; the GPU needs triangles. This module is the only place
//! that converts one into the other, and its output is *derived* state: it is cached per node and
//! recomputed only when that node's geometry or the appearance that changes its silhouette
//! changes. Camera movement never reaches this module, so panning and zooming can never
//! re-tessellate anything.
//!
//! Antialiasing is a one-pixel feather rather than MSAA. Every triangle carries an outward normal
//! and a coverage value: interior vertices have a zero normal and full coverage, feather vertices
//! carry a unit outward normal and zero coverage, and the vertex shader expands feather vertices
//! by half a pixel in screen space. Because the expansion happens on the GPU in screen space, one
//! cached tessellation stays correct at every zoom level.

use std::sync::Arc;

use visual_authoring_core_math::{triangulate_fill, FlattenedPath, Rect, Vec2};
use visual_authoring_document::{Appearance, PathGeometry};

/// Which colour a path vertex takes.
pub const PATH_VERTEX_KIND_FILL: u32 = 0;
/// Which colour a path vertex takes.
pub const PATH_VERTEX_KIND_STROKE: u32 = 1;

/// One tessellated vertex in the path's local coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathVertex {
    pub position: Vec2,
    /// Outward direction for the one-pixel feather, or zero for an interior vertex.
    pub normal: Vec2,
    /// 1.0 for solid interior, 0.0 at the outer edge of the feather.
    pub coverage: f64,
    pub kind: u32,
}

/// Triangles for one path, in local space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathTessellation {
    /// Triangle list; every three vertices are one triangle.
    pub vertices: Vec<PathVertex>,
    pub fill_vertex_count: usize,
    pub stroke_vertex_count: usize,
    /// Whether the outline enclosed a fillable region under the shared fill rule.
    pub fillable: bool,
    pub local_bounds: Option<Rect>,
}

impl PathTessellation {
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

/// The inputs that change a path's triangles. Anything outside this set — colour, opacity, the
/// camera, the node's transform — reuses the cached tessellation.
#[derive(Clone, Debug, PartialEq)]
pub struct PathTessellationKey {
    pub geometry: PathGeometry,
    pub stroke_width: f64,
    pub fill_visible: bool,
    pub stroke_visible: bool,
}

impl PathTessellationKey {
    #[must_use]
    pub fn new(geometry: &PathGeometry, appearance: Appearance) -> Self {
        Self {
            geometry: geometry.clone(),
            stroke_width: appearance.stroke.width,
            fill_visible: appearance.fill.a > 0.0,
            stroke_visible: appearance.stroke.width > 0.0 && appearance.stroke.color.a > 0.0,
        }
    }
}

/// Builds the triangles for one path.
#[must_use]
pub fn tessellate(key: &PathTessellationKey) -> Arc<PathTessellation> {
    let outline = key.geometry.flatten(key.geometry.flatten_tolerance());
    let mut tessellation = PathTessellation {
        local_bounds: key.geometry.bounds(),
        fillable: outline.is_fillable(),
        ..PathTessellation::default()
    };
    if outline.is_empty() {
        return Arc::new(tessellation);
    }

    if key.fill_visible && tessellation.fillable {
        append_fill(&outline, &mut tessellation);
    }
    if key.stroke_visible {
        append_stroke(&outline, key.stroke_width * 0.5, &mut tessellation);
    }
    Arc::new(tessellation)
}

fn append_fill(outline: &FlattenedPath, tessellation: &mut PathTessellation) {
    let Ok(indices) = triangulate_fill(outline) else {
        return;
    };
    let before = tessellation.vertices.len();
    for index in indices {
        tessellation.vertices.push(PathVertex {
            position: outline.points[index as usize],
            normal: Vec2::ZERO,
            coverage: 1.0,
            kind: PATH_VERTEX_KIND_FILL,
        });
    }
    // Feather ring: one quad per outline edge, solid on the outline and transparent one pixel out.
    let outward = outward_sign(outline);
    for (start, end) in outline.edges() {
        let Some(normal) = edge_normal(start, end, outward) else {
            continue;
        };
        push_feather_quad(tessellation, start, end, normal, PATH_VERTEX_KIND_FILL);
    }
    tessellation.fill_vertex_count = tessellation.vertices.len() - before;
}

fn append_stroke(outline: &FlattenedPath, half_width: f64, tessellation: &mut PathTessellation) {
    if !(half_width.is_finite() && half_width > 0.0) {
        return;
    }
    let before = tessellation.vertices.len();
    for (start, end) in outline.edges() {
        let Some(normal) = edge_normal(start, end, 1.0) else {
            continue;
        };
        let offset = normal * half_width;
        let inner_start = start - offset;
        let inner_end = end - offset;
        let outer_start = start + offset;
        let outer_end = end + offset;
        // Solid core.
        push_quad(
            tessellation,
            [inner_start, inner_end, outer_end, outer_start],
            PATH_VERTEX_KIND_STROKE,
        );
        // One-pixel feather on both sides of the core.
        push_feather_quad(
            tessellation,
            outer_start,
            outer_end,
            normal,
            PATH_VERTEX_KIND_STROKE,
        );
        push_feather_quad(
            tessellation,
            inner_end,
            inner_start,
            normal * -1.0,
            PATH_VERTEX_KIND_STROKE,
        );
    }
    tessellation.stroke_vertex_count = tessellation.vertices.len() - before;
}

/// Which perpendicular of an edge points away from a closed outline's interior.
fn outward_sign(outline: &FlattenedPath) -> f64 {
    if !outline.closed {
        return 1.0;
    }
    // In this +Y-down space a positively signed area traverses the outline so that the
    // (dy, -dx) perpendicular already points away from the interior.
    if outline.signed_area() > 0.0 {
        1.0
    } else {
        -1.0
    }
}

fn edge_normal(start: Vec2, end: Vec2, sign: f64) -> Option<Vec2> {
    let direction = end - start;
    let length = direction.length();
    if !length.is_finite() || length <= 1.0e-12 {
        return None;
    }
    Some(Vec2::new(direction.y / length, -direction.x / length) * sign)
}

/// Emits a quad that is solid along `start`-`end` and fades to nothing one pixel along `normal`.
fn push_feather_quad(
    tessellation: &mut PathTessellation,
    start: Vec2,
    end: Vec2,
    normal: Vec2,
    kind: u32,
) {
    let solid_start = PathVertex {
        position: start,
        normal: Vec2::ZERO,
        coverage: 1.0,
        kind,
    };
    let solid_end = PathVertex {
        position: end,
        normal: Vec2::ZERO,
        coverage: 1.0,
        kind,
    };
    let faded_start = PathVertex {
        position: start,
        normal,
        coverage: 0.0,
        kind,
    };
    let faded_end = PathVertex {
        position: end,
        normal,
        coverage: 0.0,
        kind,
    };
    tessellation
        .vertices
        .extend_from_slice(&[solid_start, solid_end, faded_end]);
    tessellation
        .vertices
        .extend_from_slice(&[solid_start, faded_end, faded_start]);
}

fn push_quad(tessellation: &mut PathTessellation, corners: [Vec2; 4], kind: u32) {
    let vertex = |position: Vec2| PathVertex {
        position,
        normal: Vec2::ZERO,
        coverage: 1.0,
        kind,
    };
    tessellation.vertices.extend_from_slice(&[
        vertex(corners[0]),
        vertex(corners[1]),
        vertex(corners[2]),
        vertex(corners[0]),
        vertex(corners[2]),
        vertex(corners[3]),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use visual_authoring_document::{ColorRgba, PathAnchor, PathAnchorId, Stroke};

    fn square(closed: bool) -> PathGeometry {
        PathGeometry {
            closed,
            anchors: vec![
                PathAnchor::new(PathAnchorId::new(), Vec2::ZERO),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(10.0, 0.0)),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(10.0, 10.0)),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(0.0, 10.0)),
            ],
        }
    }

    fn appearance(fill_alpha: f64, stroke_width: f64) -> Appearance {
        Appearance {
            fill: ColorRgba::new(0.2, 0.4, 0.8, fill_alpha),
            stroke: Stroke {
                color: ColorRgba::new(0.0, 0.0, 0.0, 1.0),
                width: stroke_width,
            },
            ..Appearance::default()
        }
    }

    #[test]
    fn a_closed_filled_path_produces_interior_triangles_and_a_feather() {
        let key = PathTessellationKey::new(&square(true), appearance(1.0, 0.0));
        let tessellation = tessellate(&key);
        assert!(tessellation.fillable);
        // Two interior triangles plus one feather quad per edge.
        assert_eq!(tessellation.fill_vertex_count, 2 * 3 + 4 * 6);
        assert_eq!(tessellation.stroke_vertex_count, 0);
        assert!(tessellation
            .vertices
            .iter()
            .all(|vertex| vertex.kind == PATH_VERTEX_KIND_FILL));
    }

    #[test]
    fn an_open_path_is_stroked_but_never_filled() {
        let key = PathTessellationKey::new(&square(false), appearance(1.0, 2.0));
        let tessellation = tessellate(&key);
        assert!(!tessellation.fillable);
        assert_eq!(tessellation.fill_vertex_count, 0);
        // Three edges, each a solid quad plus two feather quads.
        assert_eq!(tessellation.stroke_vertex_count, 3 * (6 + 6 + 6));
    }

    #[test]
    fn a_transparent_fill_and_zero_stroke_produce_no_triangles() {
        let key = PathTessellationKey::new(&square(true), appearance(0.0, 0.0));
        let tessellation = tessellate(&key);
        assert!(tessellation.is_empty());
    }

    #[test]
    fn feather_vertices_carry_a_unit_normal_and_zero_coverage() {
        let key = PathTessellationKey::new(&square(true), appearance(1.0, 0.0));
        let tessellation = tessellate(&key);
        for vertex in &tessellation.vertices {
            if vertex.coverage == 0.0 {
                assert!((vertex.normal.length() - 1.0).abs() < 1.0e-9);
            } else {
                assert_eq!(vertex.normal, Vec2::ZERO);
            }
        }
    }

    #[test]
    fn the_fill_feather_points_away_from_the_interior() {
        let key = PathTessellationKey::new(&square(true), appearance(1.0, 0.0));
        let tessellation = tessellate(&key);
        let outline = square(true).flatten(square(true).flatten_tolerance());
        for vertex in tessellation
            .vertices
            .iter()
            .filter(|vertex| vertex.coverage == 0.0)
        {
            let probe = vertex.position + vertex.normal * 0.25;
            assert!(
                !outline.contains(probe),
                "feather at {:?} pointed inward",
                vertex.position
            );
        }
    }

    #[test]
    fn appearance_that_cannot_change_the_silhouette_reuses_one_key() {
        let geometry = square(true);
        let first = PathTessellationKey::new(&geometry, appearance(1.0, 2.0));
        let mut different_colour = appearance(1.0, 2.0);
        different_colour.fill = ColorRgba::new(0.9, 0.1, 0.1, 1.0);
        different_colour.opacity = 0.5;
        let second = PathTessellationKey::new(&geometry, different_colour);
        assert_eq!(first, second);

        let mut wider = appearance(1.0, 4.0);
        wider.stroke.width = 4.0;
        assert_ne!(first, PathTessellationKey::new(&geometry, wider));
    }

    #[test]
    fn a_self_intersecting_closed_path_is_stroked_but_not_filled() {
        let bowtie = PathGeometry {
            closed: true,
            anchors: vec![
                PathAnchor::new(PathAnchorId::new(), Vec2::ZERO),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(10.0, 10.0)),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(10.0, 0.0)),
                PathAnchor::new(PathAnchorId::new(), Vec2::new(0.0, 10.0)),
            ],
        };
        let tessellation = tessellate(&PathTessellationKey::new(&bowtie, appearance(1.0, 2.0)));
        assert!(!tessellation.fillable);
        assert_eq!(tessellation.fill_vertex_count, 0);
        assert!(tessellation.stroke_vertex_count > 0);
    }
}
