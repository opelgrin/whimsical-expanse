//! The 2D camera: mapping between world tile coordinates and logical screen
//! pixels, with panning and cursor-anchored zoom.
//!
//! Tile `(x, y)` spans `[x, x + 1] × [y, y + 1]` in world space, with `+x`
//! pointing east (right) and `+y` pointing south (down), matching top-down
//! screen coordinates.

use std::error::Error;
use std::fmt;

use bytemuck::{Pod, Zeroable};

/// A point in continuous world-tile coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint {
    pub x: f32,
    pub y: f32,
}

impl WorldPoint {
    /// Creates a point in world coordinates.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A point on the window in logical pixels, measured from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
}

impl ScreenPoint {
    /// Creates a point in logical screen coordinates.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Size of the drawing area in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    /// Creates a viewport of `width` by `height` logical pixels.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// The middle of the viewport in logical pixels.
    #[must_use]
    pub fn centre(self) -> ScreenPoint {
        ScreenPoint::new(self.width * 0.5, self.height * 0.5)
    }

    fn is_non_empty(self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

/// Inclusive range of tile coordinates visible in a viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleTileRange {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

/// Initial state and zoom limits of a [`Camera`].
///
/// Callers state every parameter explicitly so that tests never depend on
/// game tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
    /// Initial world position shown at the centre of the viewport.
    pub initial_centre: WorldPoint,
    /// Initial scale, in logical pixels per tile.
    pub initial_pixels_per_tile: f32,
    /// Smallest allowed scale (most zoomed out), in logical pixels per tile.
    pub min_pixels_per_tile: f32,
    /// Largest allowed scale (most zoomed in), in logical pixels per tile.
    pub max_pixels_per_tile: f32,
}

/// Why camera creation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidCameraConfig {
    /// World dimensions are zero or negative.
    EmptyWorld,
    /// Zoom bounds are not finite and positive with `min <= max`, or the
    /// initial scale is not finite.
    InvalidZoomBounds,
    /// Initial position is not finite.
    NonFiniteCentre,
}

impl fmt::Display for InvalidCameraConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyWorld => write!(f, "world dimensions must be at least 1x1 tiles"),
            Self::InvalidZoomBounds => write!(
                f,
                "zoom bounds must be finite, positive, and have min <= max"
            ),
            Self::NonFiniteCentre => write!(f, "camera centre must be finite"),
        }
    }
}

impl Error for InvalidCameraConfig {}

/// Projection parameters uploaded to the GPU for transforming world-tile
/// coordinates into normalized device coordinates `[-1, 1]`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct CameraUniform {
    /// Scale from world tiles to clip space: `(2 * px_per_tile / width, -2 * px_per_tile / height)`.
    pub scale: [f32; 2],
    /// World-space centre subtracted before scaling.
    pub centre: [f32; 2],
}

/// Pure 2D camera state and coordinate conversions.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    centre: WorldPoint,
    pixels_per_tile: f32,
    min_pixels_per_tile: f32,
    max_pixels_per_tile: f32,
    world_width: i32,
    world_height: i32,
}

impl Camera {
    /// Creates a camera from `config` and the world dimensions in tiles.
    /// Clamps the initial centre to the world and the zoom to its limits.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCameraConfig`] if the world is empty, if the zoom bounds
    /// are not finite and positive with `min <= max`, or if the initial scale or
    /// centre is not finite.
    pub fn new(
        config: CameraConfig,
        world_width: i32,
        world_height: i32,
    ) -> Result<Self, InvalidCameraConfig> {
        if world_width < 1 || world_height < 1 {
            return Err(InvalidCameraConfig::EmptyWorld);
        }
        if !config.min_pixels_per_tile.is_finite()
            || !config.max_pixels_per_tile.is_finite()
            || !config.initial_pixels_per_tile.is_finite()
            || config.min_pixels_per_tile <= 0.0
            || config.min_pixels_per_tile > config.max_pixels_per_tile
        {
            return Err(InvalidCameraConfig::InvalidZoomBounds);
        }
        if !config.initial_centre.x.is_finite() || !config.initial_centre.y.is_finite() {
            return Err(InvalidCameraConfig::NonFiniteCentre);
        }

        let mut camera = Self {
            centre: config.initial_centre,
            pixels_per_tile: config
                .initial_pixels_per_tile
                .clamp(config.min_pixels_per_tile, config.max_pixels_per_tile),
            min_pixels_per_tile: config.min_pixels_per_tile,
            max_pixels_per_tile: config.max_pixels_per_tile,
            world_width,
            world_height,
        };
        camera.clamp_centre();
        Ok(camera)
    }

    /// Current world coordinate at the centre of the viewport.
    #[must_use]
    pub const fn centre(&self) -> WorldPoint {
        self.centre
    }

    /// Current scale in logical pixels per tile.
    #[must_use]
    pub const fn pixels_per_tile(&self) -> f32 {
        self.pixels_per_tile
    }

    /// Converts a point on the window (in logical pixels) to continuous world
    /// tile coordinates.
    #[must_use]
    pub fn screen_to_world(&self, screen: ScreenPoint, viewport: Viewport) -> WorldPoint {
        let viewport_centre = viewport.centre();
        WorldPoint::new(
            self.centre.x + (screen.x - viewport_centre.x) / self.pixels_per_tile,
            self.centre.y + (screen.y - viewport_centre.y) / self.pixels_per_tile,
        )
    }

    /// Converts a world coordinate to logical screen pixels.
    #[must_use]
    pub fn world_to_screen(&self, world: WorldPoint, viewport: Viewport) -> ScreenPoint {
        let viewport_centre = viewport.centre();
        ScreenPoint::new(
            viewport_centre.x + (world.x - self.centre.x) * self.pixels_per_tile,
            viewport_centre.y + (world.y - self.centre.y) * self.pixels_per_tile,
        )
    }

    /// Moves the camera by `(dx, dy)` logical screen pixels.
    ///
    /// Positive `dx` moves the camera east (right); positive `dy` moves it
    /// south (down). For grab-and-drag panning where the world follows the
    /// cursor, pass the negated cursor delta.
    pub fn pan_by_screen_offset(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.centre.x += dx / self.pixels_per_tile;
        self.centre.y += dy / self.pixels_per_tile;
        self.clamp_centre();
    }

    /// Scales `pixels_per_tile` by `factor`, keeping the world point under
    /// `anchor` stationary on screen unless clamping intervenes.
    ///
    /// Non-finite or non-positive factors, or empty viewports, are ignored.
    pub fn zoom_by(&mut self, factor: f32, anchor: ScreenPoint, viewport: Viewport) {
        if !(factor.is_finite() && factor > 0.0 && viewport.is_non_empty()) {
            return;
        }
        let anchor_world = self.screen_to_world(anchor, viewport);
        self.pixels_per_tile = (self.pixels_per_tile * factor)
            .clamp(self.min_pixels_per_tile, self.max_pixels_per_tile);

        let viewport_centre = viewport.centre();
        self.centre = WorldPoint::new(
            anchor_world.x - (anchor.x - viewport_centre.x) / self.pixels_per_tile,
            anchor_world.y - (anchor.y - viewport_centre.y) / self.pixels_per_tile,
        );
        self.clamp_centre();
    }

    /// Computes the range of tiles inside this camera's world (`0..world_width`
    /// by `0..world_height`) that overlap `viewport`, or `None` if the viewport
    /// is empty.
    #[must_use]
    pub fn visible_tiles(&self, viewport: Viewport) -> Option<VisibleTileRange> {
        if !viewport.is_non_empty() {
            return None;
        }
        let top_left = self.screen_to_world(ScreenPoint::new(0.0, 0.0), viewport);
        let bottom_right =
            self.screen_to_world(ScreenPoint::new(viewport.width, viewport.height), viewport);

        #[expect(
            clippy::cast_possible_truncation,
            reason = "clamped to world bounds before casting to i32"
        )]
        let (min_x, max_x, min_y, max_y) = {
            let min_x = (top_left.x.floor() as i32).max(0);
            let max_x = ((bottom_right.x.ceil() as i32) - 1).min(self.world_width - 1);
            let min_y = (top_left.y.floor() as i32).max(0);
            let max_y = ((bottom_right.y.ceil() as i32) - 1).min(self.world_height - 1);
            (min_x, max_x, min_y, max_y)
        };

        Some(VisibleTileRange {
            min_x,
            max_x,
            min_y,
            max_y,
        })
    }

    /// Builds the clip-space projection uniform for `viewport`.
    ///
    /// Snaps the camera translation to physical pixels (`scale_factor` physical
    /// pixels per logical pixel, or 1 if it is smaller) so adjacent axis-aligned tiles share exact
    /// pixel edges without sub-pixel jitter when panning.
    #[must_use]
    pub fn uniform(&self, viewport: Viewport, scale_factor: f32) -> CameraUniform {
        if !viewport.is_non_empty() {
            return CameraUniform {
                scale: [0.0, 0.0],
                centre: [self.centre.x, self.centre.y],
            };
        }
        let physical_px_per_tile = self.pixels_per_tile * scale_factor.max(1.0);
        let centre = [
            (self.centre.x * physical_px_per_tile).round() / physical_px_per_tile,
            (self.centre.y * physical_px_per_tile).round() / physical_px_per_tile,
        ];
        CameraUniform {
            scale: [
                2.0 * self.pixels_per_tile / viewport.width,
                -2.0 * self.pixels_per_tile / viewport.height,
            ],
            centre,
        }
    }

    fn clamp_centre(&mut self) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "world dimensions in tiles fit comfortably in f32"
        )]
        let (max_x, max_y) = (self.world_width as f32, self.world_height as f32);
        self.centre.x = self.centre.x.clamp(0.0, max_x);
        self.centre.y = self.centre.y.clamp(0.0, max_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_camera() -> Camera {
        Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(10.0, 8.0),
                initial_pixels_per_tile: 40.0,
                min_pixels_per_tile: 10.0,
                max_pixels_per_tile: 160.0,
            },
            30,
            20,
        )
        .expect("valid camera config")
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}"
        );
    }

    /// Catches sign errors, swapped axes, or scaling in the wrong direction
    /// between screen and world coordinates.
    #[test]
    fn screen_and_world_conversions_match_worked_coordinates_and_invert() {
        let camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);

        // Viewport centre is (400, 300), showing world (10, 8) at 40 px/tile.
        // Screen (520, 220) is +120 px east (+3 tiles) and -80 px north (-2 tiles) -> world (13, 6).
        let world = camera.screen_to_world(ScreenPoint::new(520.0, 220.0), viewport);
        assert_close(world.x, 13.0);
        assert_close(world.y, 6.0);

        let screen = camera.world_to_screen(WorldPoint::new(13.0, 6.0), viewport);
        assert_close(screen.x, 520.0);
        assert_close(screen.y, 220.0);
    }

    /// Catches zooming around the viewport centre instead of keeping the world
    /// point under the cursor fixed on screen.
    #[test]
    fn zooming_keeps_the_world_point_under_the_anchor_fixed() {
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let cursor = ScreenPoint::new(600.0, 140.0);

        let before = camera.screen_to_world(cursor, viewport);
        camera.zoom_by(2.0, cursor, viewport);
        let after = camera.screen_to_world(cursor, viewport);

        assert_close(camera.pixels_per_tile(), 80.0);
        assert_close(after.x, before.x);
        assert_close(after.y, before.y);
    }

    /// Catches zoom escaping `[min, max]`, drifting the anchor when zoom hits
    /// its limit, or producing NaNs on invalid factors or zero-sized viewports.
    #[test]
    fn zooming_clamps_to_limits_and_ignores_invalid_inputs() {
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let cursor = ScreenPoint::new(520.0, 260.0);
        let before = camera.screen_to_world(cursor, viewport);

        // 40 * 10 = 400, clamped to max 160 (an effective 4x zoom).
        camera.zoom_by(10.0, cursor, viewport);
        assert_close(camera.pixels_per_tile(), 160.0);
        let at_max = camera.screen_to_world(cursor, viewport);
        assert_close(at_max.x, before.x);
        assert_close(at_max.y, before.y);

        // 160 * 0.01 = 1.6, clamped to min 10.
        camera.zoom_by(0.01, viewport.centre(), viewport);
        assert_close(camera.pixels_per_tile(), 10.0);

        // Zoom in slightly above min so a zero factor that bypassed the guard
        // would observably drop `pixels_per_tile` back down to 10.
        camera.zoom_by(2.0, cursor, viewport);
        assert_close(camera.pixels_per_tile(), 20.0);

        let state = camera.clone();
        camera.zoom_by(f32::NAN, cursor, viewport);
        camera.zoom_by(0.0, cursor, viewport);
        camera.zoom_by(-2.0, cursor, viewport);
        camera.zoom_by(2.0, cursor, Viewport::new(0.0, 600.0));
        camera.zoom_by(2.0, cursor, Viewport::new(800.0, 0.0));
        assert_eq!(camera, state);
    }

    /// Catches panning scaling with zoom in the wrong direction, failing to
    /// clamp at world edges, or poisoning the camera with non-finite offsets.
    #[test]
    fn panning_by_screen_pixels_scales_with_zoom_and_clamps_to_world_bounds() {
        let mut camera = test_camera();
        // At 40 px/tile, +80 px east and -40 px north moves centre from (10, 8) to (12, 7).
        camera.pan_by_screen_offset(80.0, -40.0);
        assert_close(camera.centre().x, 12.0);
        assert_close(camera.centre().y, 7.0);

        camera.pan_by_screen_offset(f32::NAN, 10.0);
        camera.pan_by_screen_offset(10.0, f32::INFINITY);
        assert_close(camera.centre().x, 12.0);
        assert_close(camera.centre().y, 7.0);

        camera.pan_by_screen_offset(-100_000.0, 100_000.0);
        assert_close(camera.centre().x, 0.0);
        assert_close(camera.centre().y, 20.0);
    }

    /// Catches off-by-one culling that drops partially visible edge tiles,
    /// includes tiles strictly outside an exact boundary, or draws into an
    /// empty viewport.
    #[test]
    fn visible_tiles_include_partial_edge_tiles_and_clamp_to_the_grid() {
        let camera = Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(5.0, 4.0),
                initial_pixels_per_tile: 40.0,
                min_pixels_per_tile: 10.0,
                max_pixels_per_tile: 160.0,
            },
            6,
            5,
        )
        .expect("valid camera config");

        // Viewport 240x160 at 40 px/tile spans exactly 6.0 x 4.0 tiles around (5.0, 4.0):
        // x in [2.0, 8.0] -> overlaps tiles 2..=5 (clamped from 7 to world_width - 1 = 5).
        // y in [2.0, 6.0] -> overlaps tiles 2..=4 (clamped from 5 to world_height - 1 = 4).
        assert_eq!(
            camera.visible_tiles(Viewport::new(240.0, 160.0)),
            Some(VisibleTileRange {
                min_x: 2,
                max_x: 5,
                min_y: 2,
                max_y: 4,
            })
        );

        // Partial edge tiles: 200x100 spans 5.0 x 2.5 tiles around (5.0, 4.0):
        // x in [2.5, 7.5] -> min_x = 2, max_x = 5.
        // y in [2.75, 5.25] -> min_y = 2, max_y = 4.
        assert_eq!(
            camera.visible_tiles(Viewport::new(200.0, 100.0)),
            Some(VisibleTileRange {
                min_x: 2,
                max_x: 5,
                min_y: 2,
                max_y: 4,
            })
        );

        // Exact boundary inside the grid: camera at (3.0, 2.0) on a 10x10 world,
        // viewport 160x80 at 40 px/tile spans 4.0 x 2.0 tiles -> x in [1.0, 5.0], y in [1.0, 3.0].
        // Tile 5 covers [5.0, 6.0] (outside), so visible tiles are 1..=4 and 1..=2.
        let inner = Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(3.0, 2.0),
                initial_pixels_per_tile: 40.0,
                min_pixels_per_tile: 10.0,
                max_pixels_per_tile: 160.0,
            },
            10,
            10,
        )
        .expect("valid camera config");
        assert_eq!(
            inner.visible_tiles(Viewport::new(160.0, 80.0)),
            Some(VisibleTileRange {
                min_x: 1,
                max_x: 4,
                min_y: 1,
                max_y: 2,
            })
        );

        assert_eq!(camera.visible_tiles(Viewport::new(0.0, 100.0)), None);
        assert_eq!(camera.visible_tiles(Viewport::new(100.0, 0.0)), None);
    }

    /// Catches inverted clip-space Y, wrong scale, high-DPI pixel-snapping math
    /// errors, or division by zero on minimized windows.
    #[test]
    fn gpu_uniform_maps_world_points_to_clip_space_and_snaps_to_physical_pixels() {
        let camera = Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(10.03, 8.0),
                initial_pixels_per_tile: 10.0,
                min_pixels_per_tile: 5.0,
                max_pixels_per_tile: 160.0,
            },
            30,
            20,
        )
        .expect("valid camera config");
        let viewport = Viewport::new(800.0, 400.0);

        // At 10 logical px/tile and HiDPI scale_factor 2.0 -> 20 physical px/tile (0.05 tile steps).
        // 10.03 * 20 = 200.6 -> rounds to 201 / 20 = 10.05.
        let uniform = camera.uniform(viewport, 2.0);
        assert_close(uniform.centre[0], 10.05);
        assert_close(uniform.centre[1], 8.0);

        // +40 tiles (+400 logical px) east of snapped centre -> +1.0 NDC X.
        let ndc_x = ((uniform.centre[0] + 40.0) - uniform.centre[0]) * uniform.scale[0];
        // -20 tiles (-200 logical px, top edge: world Y = -12.0) north of centre (8.0) -> +1.0 NDC Y.
        let ndc_y = (-12.0 - uniform.centre[1]) * uniform.scale[1];
        assert_close(ndc_x, 1.0);
        assert_close(ndc_y, 1.0);

        assert_eq!(
            camera.uniform(Viewport::new(0.0, 400.0), 2.0).scale,
            [0.0, 0.0]
        );
        assert_eq!(
            camera.uniform(Viewport::new(800.0, 0.0), 2.0).scale,
            [0.0, 0.0]
        );
    }

    /// Catches invalid configs that would cause division by zero or NaNs.
    #[test]
    fn rejects_invalid_camera_configs() {
        let valid = CameraConfig {
            initial_centre: WorldPoint::new(0.0, 0.0),
            initial_pixels_per_tile: 32.0,
            min_pixels_per_tile: 8.0,
            max_pixels_per_tile: 64.0,
        };
        assert_eq!(
            Camera::new(valid, 0, 10),
            Err(InvalidCameraConfig::EmptyWorld)
        );
        assert_eq!(
            Camera::new(
                CameraConfig {
                    min_pixels_per_tile: 0.0,
                    ..valid
                },
                10,
                10,
            ),
            Err(InvalidCameraConfig::InvalidZoomBounds)
        );
        assert_eq!(
            Camera::new(
                CameraConfig {
                    min_pixels_per_tile: 64.0,
                    max_pixels_per_tile: 32.0,
                    ..valid
                },
                10,
                10,
            ),
            Err(InvalidCameraConfig::InvalidZoomBounds)
        );
    }
}
