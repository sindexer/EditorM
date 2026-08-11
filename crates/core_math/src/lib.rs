//! Pure, renderer-independent document math.
//!
//! Matrices use column-vector composition: `parent * local` means "apply the
//! local transform, then the parent transform". Document coordinates use +X
//! right and +Y down; consequently, positive rotation is visually clockwise.

use std::ops::{Add, Mul, Sub};

/// Default tolerance for deterministic floating-point assertions.
pub const DEFAULT_EPSILON: f64 = 1.0e-9;

/// A two-dimensional vector or point in document space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[must_use]
    pub fn approx_eq(self, other: Self, epsilon: f64) -> bool {
        approx_eq_f64(self.x, other.x, epsilon) && approx_eq_f64(self.y, other.y, epsilon)
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// An axis-aligned rectangle represented by minimum and maximum corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    #[must_use]
    pub const fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub const fn from_size(size: Vec2) -> Self {
        Self::from_min_max(Vec2::ZERO, size)
    }

    #[must_use]
    pub fn from_points(points: impl IntoIterator<Item = Vec2>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut min = first;
        let mut max = first;

        for point in points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }

        Some(Self { min, max })
    }

    #[must_use]
    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    #[must_use]
    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }

    #[must_use]
    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    #[must_use]
    pub fn corners(self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }

    #[must_use]
    pub fn approx_eq(self, other: Self, epsilon: f64) -> bool {
        self.min.approx_eq(other.min, epsilon) && self.max.approx_eq(other.max, epsilon)
    }
}

/// A 2D affine transform stored as a 2x3 matrix.
///
/// ```text
/// | m11 m12 tx |
/// | m21 m22 ty |
/// |  0   0   1 |
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    pub m11: f64,
    pub m12: f64,
    pub m21: f64,
    pub m22: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2 {
    pub const IDENTITY: Self = Self::from_components(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    #[must_use]
    pub const fn from_components(m11: f64, m12: f64, m21: f64, m22: f64, tx: f64, ty: f64) -> Self {
        Self {
            m11,
            m12,
            m21,
            m22,
            tx,
            ty,
        }
    }

    #[must_use]
    pub const fn translation(offset: Vec2) -> Self {
        Self::from_components(1.0, 0.0, 0.0, 1.0, offset.x, offset.y)
    }

    #[must_use]
    pub fn rotation(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::from_components(cos, -sin, sin, cos, 0.0, 0.0)
    }

    #[must_use]
    pub const fn scale(scale: Vec2) -> Self {
        Self::from_components(scale.x, 0.0, 0.0, scale.y, 0.0, 0.0)
    }

    #[must_use]
    pub fn determinant(self) -> f64 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        [self.m11, self.m12, self.m21, self.m22, self.tx, self.ty]
            .into_iter()
            .all(f64::is_finite)
    }

    /// Returns `None` for singular, near-singular, or non-finite matrices.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }

        let determinant = self.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }

        let linear_scale = self
            .m11
            .abs()
            .max(self.m12.abs())
            .max(self.m21.abs())
            .max(self.m22.abs());
        if !linear_scale.is_finite() || linear_scale == 0.0 {
            return None;
        }

        // Compare a normalized determinant so the scale-relative singularity
        // threshold cannot itself overflow for otherwise finite input.
        let normalized_determinant = (self.m11 / linear_scale) * (self.m22 / linear_scale)
            - (self.m12 / linear_scale) * (self.m21 / linear_scale);
        let singular_threshold = f64::EPSILON * 16.0;
        if !normalized_determinant.is_finite()
            || !singular_threshold.is_finite()
            || normalized_determinant.abs() <= singular_threshold
        {
            return None;
        }

        let inverse_determinant = determinant.recip();
        if !inverse_determinant.is_finite() {
            return None;
        }
        let m11 = self.m22 * inverse_determinant;
        let m12 = -self.m12 * inverse_determinant;
        let m21 = -self.m21 * inverse_determinant;
        let m22 = self.m11 * inverse_determinant;
        let tx = -(m11 * self.tx + m12 * self.ty);
        let ty = -(m21 * self.tx + m22 * self.ty);
        let inverse = Self::from_components(m11, m12, m21, m22, tx, ty);

        inverse.is_finite().then_some(inverse)
    }

    #[must_use]
    pub fn transform_point(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.m11 * point.x + self.m12 * point.y + self.tx,
            self.m21 * point.x + self.m22 * point.y + self.ty,
        )
    }

    #[must_use]
    pub fn transform_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(
            self.m11 * vector.x + self.m12 * vector.y,
            self.m21 * vector.x + self.m22 * vector.y,
        )
    }

    #[must_use]
    pub fn transform_rect(self, rect: Rect) -> Rect {
        Rect::from_points(rect.corners().map(|corner| self.transform_point(corner)))
            .expect("a rectangle always has four corners")
    }

    #[must_use]
    pub fn approx_eq(self, other: Self, epsilon: f64) -> bool {
        approx_eq_f64(self.m11, other.m11, epsilon)
            && approx_eq_f64(self.m12, other.m12, epsilon)
            && approx_eq_f64(self.m21, other.m21, epsilon)
            && approx_eq_f64(self.m22, other.m22, epsilon)
            && approx_eq_f64(self.tx, other.tx, epsilon)
            && approx_eq_f64(self.ty, other.ty, epsilon)
    }
}

impl Mul for Affine2 {
    type Output = Self;

    /// Composes transforms so `a * b` applies `b` first and then `a`.
    fn mul(self, rhs: Self) -> Self::Output {
        Self::from_components(
            self.m11 * rhs.m11 + self.m12 * rhs.m21,
            self.m11 * rhs.m12 + self.m12 * rhs.m22,
            self.m21 * rhs.m11 + self.m22 * rhs.m21,
            self.m21 * rhs.m12 + self.m22 * rhs.m22,
            self.m11 * rhs.tx + self.m12 * rhs.ty + self.tx,
            self.m21 * rhs.tx + self.m22 * rhs.ty + self.ty,
        )
    }
}

#[must_use]
pub fn approx_eq_f64(left: f64, right: f64, epsilon: f64) -> bool {
    if left == right {
        return true;
    }
    if !left.is_finite() || !right.is_finite() || epsilon < 0.0 {
        return false;
    }

    let scale = 1.0_f64.max(left.abs()).max(right.abs());
    (left - right).abs() <= epsilon * scale
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn identity_preserves_points_and_vectors() {
        let value = Vec2::new(12.5, -4.25);
        assert_eq!(Affine2::IDENTITY.transform_point(value), value);
        assert_eq!(Affine2::IDENTITY.transform_vector(value), value);
    }

    #[test]
    fn translation_does_not_affect_vectors() {
        let transform = Affine2::translation(Vec2::new(10.0, -3.0));
        assert_eq!(
            transform.transform_point(Vec2::new(2.0, 5.0)),
            Vec2::new(12.0, 2.0)
        );
        assert_eq!(
            transform.transform_vector(Vec2::new(2.0, 5.0)),
            Vec2::new(2.0, 5.0)
        );
    }

    #[test]
    fn uniform_scale_affects_both_axes_equally() {
        let transform = Affine2::scale(Vec2::new(3.5, 3.5));
        assert_eq!(
            transform.transform_point(Vec2::new(2.0, -4.0)),
            Vec2::new(7.0, -14.0)
        );
    }

    #[test]
    fn rotation_and_scale_compose_in_declared_order() {
        let transform = Affine2::translation(Vec2::new(3.0, 7.0))
            * Affine2::rotation(std::f64::consts::FRAC_PI_2)
            * Affine2::scale(Vec2::new(2.0, 4.0));
        let actual = transform.transform_point(Vec2::new(1.0, 2.0));
        assert!(actual.approx_eq(Vec2::new(-5.0, 9.0), DEFAULT_EPSILON));
    }

    #[test]
    fn negative_scale_is_invertible() {
        let transform = Affine2::translation(Vec2::new(4.0, 8.0))
            * Affine2::rotation(0.4)
            * Affine2::scale(Vec2::new(-2.0, 3.0));
        let inverse = transform
            .inverse()
            .expect("negative non-zero scale is invertible");
        assert!((inverse * transform).approx_eq(Affine2::IDENTITY, DEFAULT_EPSILON));
    }

    #[test]
    fn singular_matrices_fail_inversion_safely() {
        let singular = Affine2::scale(Vec2::new(1.0, 0.0));
        assert_eq!(singular.inverse(), None);
    }

    #[test]
    fn inverse_rejects_non_finite_determinant_without_panicking() {
        let finite = Affine2::from_components(f64::MAX, f64::MAX, f64::MAX, f64::MAX, 0.0, 0.0);
        let result = std::panic::catch_unwind(|| finite.inverse());
        assert_eq!(result.expect("inverse must not panic"), None);
    }

    #[test]
    fn inverse_rejects_non_finite_inverse_translation_without_panicking() {
        let finite = Affine2::from_components(1.0, -1.0, 0.0, 1.0, f64::MAX, f64::MAX);
        assert!(finite.determinant().is_finite());
        let result = std::panic::catch_unwind(|| finite.inverse());
        assert_eq!(result.expect("inverse must not panic"), None);
    }

    #[test]
    fn inverse_preserves_valid_large_magnitude_transforms() {
        let transform = Affine2::from_components(1.0e150, 0.0, 0.0, 2.0e150, 3.0e150, -4.0e150);
        let inverse = transform
            .inverse()
            .expect("large finite transform is invertible");
        assert!(inverse.is_finite());
        assert!((inverse * transform).approx_eq(Affine2::IDENTITY, 1.0e-8));
    }

    #[test]
    fn transformed_rect_contains_all_transformed_corners() {
        let rect = Rect::from_size(Vec2::new(10.0, 20.0));
        let transform = Affine2::translation(Vec2::new(4.0, -2.0)) * Affine2::rotation(0.25);
        let bounds = transform.transform_rect(rect);
        for point in rect.corners().map(|point| transform.transform_point(point)) {
            assert!(point.x >= bounds.min.x - DEFAULT_EPSILON);
            assert!(point.x <= bounds.max.x + DEFAULT_EPSILON);
            assert!(point.y >= bounds.min.y - DEFAULT_EPSILON);
            assert!(point.y <= bounds.max.y + DEFAULT_EPSILON);
        }
    }

    proptest! {
        #[test]
        fn finite_inputs_never_produce_a_non_finite_inverse(
            m11 in any::<f64>(),
            m12 in any::<f64>(),
            m21 in any::<f64>(),
            m22 in any::<f64>(),
            tx in any::<f64>(),
            ty in any::<f64>(),
        ) {
            let values = [m11, m12, m21, m22, tx, ty];
            prop_assume!(values.into_iter().all(f64::is_finite));
            let transform = Affine2::from_components(m11, m12, m21, m22, tx, ty);
            if let Some(inverse) = transform.inverse() {
                prop_assert!(inverse.is_finite());
            }
        }

        #[test]
        fn combined_affine_round_trip_is_stable(
            tx in -10_000.0_f64..10_000.0,
            ty in -10_000.0_f64..10_000.0,
            radians in -20.0_f64..20.0,
            sx in 0.1_f64..10.0,
            sy in 0.1_f64..10.0,
            px in -10_000.0_f64..10_000.0,
            py in -10_000.0_f64..10_000.0,
            flip_x in any::<bool>(),
            flip_y in any::<bool>(),
        ) {
            let sx = if flip_x { -sx } else { sx };
            let sy = if flip_y { -sy } else { sy };
            let transform = Affine2::translation(Vec2::new(tx, ty))
                * Affine2::rotation(radians)
                * Affine2::scale(Vec2::new(sx, sy));
            let inverse = transform.inverse().expect("generated transform is invertible");
            let point = Vec2::new(px, py);
            let round_trip = inverse.transform_point(transform.transform_point(point));
            prop_assert!(round_trip.approx_eq(point, 1.0e-8));
        }
    }
}
