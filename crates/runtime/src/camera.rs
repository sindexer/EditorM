use thiserror::Error;
use visual_authoring_core_math::{Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPoint(pub Vec2);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportPoint(pub Vec2);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevicePoint(pub Vec2);

#[derive(Clone, Debug, Error, PartialEq)]
pub enum CameraError {
    #[error("camera center must be finite")]
    InvalidCenter,
    #[error("camera zoom must be finite and greater than zero")]
    InvalidZoom,
    #[error("viewport dimensions must be finite and greater than zero")]
    InvalidViewport,
    #[error("device pixel ratio must be finite and greater than zero")]
    InvalidDevicePixelRatio,
    #[error("camera point or delta must be finite")]
    InvalidCoordinate,
    #[error("fit bounds must be finite, ordered, and non-empty")]
    InvalidFitBounds,
    #[error("fit padding must be finite, non-negative, and leave usable viewport space")]
    InvalidFitPadding,
    #[error("camera conversion produced a non-finite result")]
    NonFiniteResult,
}

/// Editor-session camera. It is not a document node and is never serialized with Document.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    center: WorldPoint,
    zoom: f64,
    viewport_size: Vec2,
    device_pixel_ratio: f64,
}

impl Camera {
    pub fn new(
        center: WorldPoint,
        zoom: f64,
        viewport_size: Vec2,
        device_pixel_ratio: f64,
    ) -> Result<Self, CameraError> {
        validate_center(center)?;
        validate_zoom(zoom)?;
        validate_viewport(viewport_size)?;
        validate_dpr(device_pixel_ratio)?;
        Ok(Self {
            center,
            zoom,
            viewport_size,
            device_pixel_ratio,
        })
    }

    #[must_use]
    pub const fn center(&self) -> WorldPoint {
        self.center
    }

    #[must_use]
    pub const fn zoom(&self) -> f64 {
        self.zoom
    }

    #[must_use]
    pub const fn viewport_size(&self) -> Vec2 {
        self.viewport_size
    }

    #[must_use]
    pub const fn device_pixel_ratio(&self) -> f64 {
        self.device_pixel_ratio
    }

    pub fn world_to_viewport(&self, point: WorldPoint) -> Result<ViewportPoint, CameraError> {
        if !point.0.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let result = (point.0 - self.center.0) * self.zoom + self.viewport_size * 0.5;
        if !result.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        Ok(ViewportPoint(result))
    }

    pub fn viewport_to_world(&self, point: ViewportPoint) -> Result<WorldPoint, CameraError> {
        if !point.0.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let result = self.center.0 + (point.0 - self.viewport_size * 0.5) * self.zoom.recip();
        if !result.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        Ok(WorldPoint(result))
    }

    pub fn world_viewport_bounds(&self) -> Result<Rect, CameraError> {
        let min = self.viewport_to_world(ViewportPoint(Vec2::ZERO))?.0;
        let max = self.viewport_to_world(ViewportPoint(self.viewport_size))?.0;
        let bounds = Rect::from_min_max(min, max);
        bounds
            .is_finite()
            .then_some(bounds)
            .ok_or(CameraError::NonFiniteResult)
    }

    pub fn viewport_to_device(&self, point: ViewportPoint) -> Result<DevicePoint, CameraError> {
        if !point.0.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let result = point.0 * self.device_pixel_ratio;
        if !result.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        Ok(DevicePoint(result))
    }

    pub fn pan(&mut self, viewport_delta: Vec2) -> Result<(), CameraError> {
        if !viewport_delta.is_finite() {
            return Err(CameraError::InvalidCoordinate);
        }
        let center = self.center.0 - viewport_delta * self.zoom.recip();
        if !center.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        self.center = WorldPoint(center);
        Ok(())
    }

    pub fn set_zoom(&mut self, zoom: f64) -> Result<(), CameraError> {
        validate_zoom(zoom)?;
        self.zoom = zoom;
        Ok(())
    }

    pub fn zoom_around(&mut self, pointer: ViewportPoint, zoom: f64) -> Result<(), CameraError> {
        validate_zoom(zoom)?;
        let world_under_pointer = self.viewport_to_world(pointer)?;
        let center = world_under_pointer.0 - (pointer.0 - self.viewport_size * 0.5) * zoom.recip();
        if !center.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        self.zoom = zoom;
        self.center = WorldPoint(center);
        Ok(())
    }

    pub fn fit_world_bounds(&mut self, bounds: Rect, padding: f64) -> Result<(), CameraError> {
        if !bounds.is_finite()
            || bounds.min.x > bounds.max.x
            || bounds.min.y > bounds.max.y
            || bounds.width() <= 0.0
            || bounds.height() <= 0.0
        {
            return Err(CameraError::InvalidFitBounds);
        }
        if !padding.is_finite() || padding < 0.0 {
            return Err(CameraError::InvalidFitPadding);
        }
        let available = self.viewport_size - Vec2::new(padding * 2.0, padding * 2.0);
        if available.x <= 0.0 || available.y <= 0.0 || !available.is_finite() {
            return Err(CameraError::InvalidFitPadding);
        }
        let zoom = (available.x / bounds.width()).min(available.y / bounds.height());
        validate_zoom(zoom)?;
        let center = bounds.center();
        if !center.is_finite() {
            return Err(CameraError::NonFiniteResult);
        }
        self.center = WorldPoint(center);
        self.zoom = zoom;
        Ok(())
    }

    pub fn resize_viewport(&mut self, viewport_size: Vec2) -> Result<(), CameraError> {
        validate_viewport(viewport_size)?;
        self.viewport_size = viewport_size;
        Ok(())
    }

    pub fn set_device_pixel_ratio(&mut self, dpr: f64) -> Result<(), CameraError> {
        validate_dpr(dpr)?;
        self.device_pixel_ratio = dpr;
        Ok(())
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new(WorldPoint(Vec2::ZERO), 1.0, Vec2::new(1_024.0, 768.0), 1.0)
            .expect("the default camera is valid")
    }
}

fn validate_center(center: WorldPoint) -> Result<(), CameraError> {
    center
        .0
        .is_finite()
        .then_some(())
        .ok_or(CameraError::InvalidCenter)
}

fn validate_zoom(zoom: f64) -> Result<(), CameraError> {
    (zoom.is_finite() && zoom > 0.0)
        .then_some(())
        .ok_or(CameraError::InvalidZoom)
}

fn validate_viewport(viewport: Vec2) -> Result<(), CameraError> {
    (viewport.is_finite() && viewport.x > 0.0 && viewport.y > 0.0)
        .then_some(())
        .ok_or(CameraError::InvalidViewport)
}

fn validate_dpr(dpr: f64) -> Result<(), CameraError> {
    (dpr.is_finite() && dpr > 0.0)
        .then_some(())
        .ok_or(CameraError::InvalidDevicePixelRatio)
}
