//! The 2D camera: mapping between world tile coordinates and logical screen
//! pixels, with panning and cursor-anchored zoom.
//!
//! Tile `(x, y)` spans `[x, x + 1] × [y, y + 1]` in world space, with `+x`
//! pointing east (right) and `+y` pointing south (down), matching top-down
//! screen coordinates.
//!
//! The world wraps east–west, so the camera does too. Its centre stays
//! canonical, with `x` in `0..width`, so that precision holds however far it
//! pans. A view that spans the seam shows world coordinates beyond it, such
//! as `x = -1` for the last column: those points name canonical tiles through
//! the [`Grid`].

use std::error::Error;
use std::fmt;

use bytemuck::{Pod, Zeroable};
use expanse_core::grid::{Grid, TilePos};

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
///
/// Rows lie between the north and south edges. Columns are not canonical:
/// where the view spans the seam, they continue beyond it, and a view wider
/// than the world covers some columns more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleTileRange {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

/// Initial zoom and zoom limits of a [`Camera`].
///
/// Callers state every parameter explicitly so that tests never depend on
/// game tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
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
    /// Zoom bounds are not finite and positive with `min <= max`, or the
    /// initial scale is not finite.
    InvalidZoomBounds,
    /// Initial position is not finite.
    NonFiniteCentre,
}

impl fmt::Display for InvalidCameraConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
    /// Canonical: `x` lies in `0..width` and `y` in `0..=height`.
    centre: WorldPoint,
    pixels_per_tile: f32,
    min_pixels_per_tile: f32,
    max_pixels_per_tile: f32,
    grid: Grid,
}

impl Camera {
    /// Creates a camera over `grid`, the world's shape, centred on `centre`.
    /// Wraps the centre across the seam, clamps it between the north and
    /// south edges, and clamps the zoom to its limits.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCameraConfig`] if the zoom bounds are not finite and
    /// positive with `min <= max`, or if the initial scale or the centre is
    /// not finite.
    pub fn new(
        config: CameraConfig,
        grid: Grid,
        centre: WorldPoint,
    ) -> Result<Self, InvalidCameraConfig> {
        if !config.min_pixels_per_tile.is_finite()
            || !config.max_pixels_per_tile.is_finite()
            || !config.initial_pixels_per_tile.is_finite()
            || config.min_pixels_per_tile <= 0.0
            || config.min_pixels_per_tile > config.max_pixels_per_tile
        {
            return Err(InvalidCameraConfig::InvalidZoomBounds);
        }
        if !centre.x.is_finite() || !centre.y.is_finite() {
            return Err(InvalidCameraConfig::NonFiniteCentre);
        }

        let mut camera = Self {
            centre,
            pixels_per_tile: config
                .initial_pixels_per_tile
                .clamp(config.min_pixels_per_tile, config.max_pixels_per_tile),
            min_pixels_per_tile: config.min_pixels_per_tile,
            max_pixels_per_tile: config.max_pixels_per_tile,
            grid,
        };
        camera.wrap_centre();
        Ok(camera)
    }

    /// Current world coordinate at the centre of the viewport. It is
    /// canonical: `x` lies in `0..width`.
    #[must_use]
    pub const fn centre(&self) -> WorldPoint {
        self.centre
    }

    /// Moves the camera over `grid`, a world of another shape, keeping its
    /// centre where it is if the world still has it.
    pub fn set_grid(&mut self, grid: Grid) {
        self.grid = grid;
        self.wrap_centre();
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

    /// The canonical tile under a point on the window, or `None` beyond the
    /// north and south edges.
    #[must_use]
    pub fn tile_at(&self, screen: ScreenPoint, viewport: Viewport) -> Option<TilePos> {
        let world = self.screen_to_world(screen, viewport);
        if !world.x.is_finite() || !world.y.is_finite() {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a point on the window lies within a few world widths of the canonical centre"
        )]
        let tile = TilePos::new(world.x.floor() as i32, world.y.floor() as i32);
        self.grid.canonical(tile)
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
        self.wrap_centre();
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
        self.wrap_centre();
    }

    /// Computes the range of tiles that overlap `viewport`, or `None` if the
    /// viewport is empty. Rows are clamped to the world's; columns are not
    /// (see [`VisibleTileRange`]).
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
            reason = "a viewport spans a bounded number of tiles around the canonical centre"
        )]
        let (min_x, max_x, min_y, max_y) = {
            let min_x = top_left.x.floor() as i32;
            let max_x = (bottom_right.x.ceil() as i32) - 1;
            let min_y = (top_left.y.floor() as i32).max(0);
            let max_y = ((bottom_right.y.ceil() as i32) - 1).min(self.grid.height() - 1);
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

    /// Wraps the centre across the seam and clamps it between the north and
    /// south edges.
    fn wrap_centre(&mut self) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "world dimensions in tiles fit comfortably in f32"
        )]
        let (width, height) = (self.grid.width() as f32, self.grid.height() as f32);
        let x = self.centre.x.rem_euclid(width);
        // A tiny negative `x` rounds up to `width` itself, which is not canonical.
        self.centre.x = if x < width { x } else { 0.0 };
        self.centre.y = self.centre.y.clamp(0.0, height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: CameraConfig = CameraConfig {
        initial_pixels_per_tile: 40.0,
        min_pixels_per_tile: 10.0,
        max_pixels_per_tile: 160.0,
    };

    fn grid(width: i32, height: i32) -> Grid {
        Grid::new(width, height).expect("a grid with tiles")
    }

    fn camera_at(config: CameraConfig, grid: Grid, x: f32, y: f32) -> Camera {
        Camera::new(config, grid, WorldPoint::new(x, y)).expect("valid camera config")
    }

    fn test_camera() -> Camera {
        camera_at(CONFIG, grid(30, 20), 10.0, 8.0)
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

    /// Catches a screen point that names a tile beyond the seam, or the wrong
    /// tile next to it, and rows beyond the edges that name a tile. The
    /// camera looks at the seam, and its view is wider than the world, so
    /// points on either side, and a whole world away, must all name
    /// canonical tiles. Values worked out by hand.
    #[test]
    fn a_screen_point_names_the_canonical_tile_on_either_side_of_the_seam() {
        let config = CameraConfig {
            initial_pixels_per_tile: 10.0,
            ..CONFIG
        };
        let camera = camera_at(config, grid(64, 12), 0.0, 5.5);
        // At 10 px/tile, the centre of the viewport (1000, 50) shows (0, 5.5).
        let viewport = Viewport::new(2000.0, 100.0);
        let tile = |x, y| camera.tile_at(ScreenPoint::new(x, y), viewport);

        assert_eq!(tile(995.0, 50.0), Some(TilePos::new(63, 5)));
        assert_eq!(tile(1005.0, 50.0), Some(TilePos::new(0, 5)));
        // 64.5 tiles west of the centre: half a tile west of the seam, a
        // world further.
        assert_eq!(tile(355.0, 50.0), Some(TilePos::new(63, 5)));
        assert_eq!(tile(1645.0, 50.0), Some(TilePos::new(0, 5)));
        // Half a tile beyond the north edge, and on the south edge's last row.
        assert_eq!(tile(1005.0, -10.0), None);
        assert_eq!(tile(1005.0, 110.0), Some(TilePos::new(0, 11)));
        assert_eq!(tile(1005.0, 125.0), None);
    }

    /// Catches panning scaling with zoom in the wrong direction, a centre
    /// that is clamped at the seam instead of crossing it, or that leaves the
    /// canonical columns and loses precision on a long pan. Rows are still
    /// clamped at the edges.
    #[test]
    fn panning_crosses_the_seam_both_ways_and_stops_at_the_edges() {
        let mut camera = camera_at(CONFIG, grid(30, 20), 0.5, 8.0);
        // At 40 px/tile, 40 px is one tile.
        camera.pan_by_screen_offset(-40.0, -40.0);
        assert_close(camera.centre().x, 29.5);
        assert_close(camera.centre().y, 7.0);
        camera.pan_by_screen_offset(80.0, 0.0);
        assert_close(camera.centre().x, 1.5);

        // 2,500 tiles west, many times round the world: 1.5 - 2,500 is 21.5
        // modulo 30.
        camera.pan_by_screen_offset(-100_000.0, 100_000.0);
        assert_close(camera.centre().x, 21.5);
        assert_close(camera.centre().y, 20.0);

        camera.pan_by_screen_offset(f32::NAN, 10.0);
        camera.pan_by_screen_offset(10.0, f32::INFINITY);
        assert_close(camera.centre().x, 21.5);
        assert_close(camera.centre().y, 20.0);
    }

    /// Catches a centre left just outside the canonical columns, which
    /// rounding makes possible for a tiny step west of the seam.
    #[test]
    fn the_centre_stays_canonical_after_a_tiny_step_west_of_the_seam() {
        let mut camera = camera_at(CONFIG, grid(30, 20), 0.0, 8.0);
        camera.pan_by_screen_offset(-1e-6, 0.0);
        let x = camera.centre().x;
        assert!((0.0..30.0).contains(&x), "{x}");
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

    /// Catches off-by-one culling that drops partially visible edge tiles,
    /// includes tiles strictly outside an exact boundary, or draws into an
    /// empty viewport, and columns clamped at the seam instead of continuing
    /// beyond it.
    #[test]
    fn visible_tiles_include_partial_edge_tiles_and_continue_across_the_seam() {
        let camera = camera_at(CONFIG, grid(6, 5), 5.0, 4.0);

        // Viewport 240x160 at 40 px/tile spans exactly 6.0 x 4.0 tiles around (5.0, 4.0):
        // x in [2.0, 8.0] -> overlaps tiles 2..=7, beyond the seam at 6.
        // y in [2.0, 6.0] -> overlaps tiles 2..=4 (clamped from 5 to height - 1 = 4).
        assert_eq!(
            camera.visible_tiles(Viewport::new(240.0, 160.0)),
            Some(VisibleTileRange {
                min_x: 2,
                max_x: 7,
                min_y: 2,
                max_y: 4,
            })
        );

        // Partial edge tiles: 200x100 spans 5.0 x 2.5 tiles around (5.0, 4.0):
        // x in [2.5, 7.5] -> min_x = 2, max_x = 7.
        // y in [2.75, 5.25] -> min_y = 2, max_y = 4.
        assert_eq!(
            camera.visible_tiles(Viewport::new(200.0, 100.0)),
            Some(VisibleTileRange {
                min_x: 2,
                max_x: 7,
                min_y: 2,
                max_y: 4,
            })
        );

        // Exact boundaries: camera at (1.0, 2.0) on a 10x10 world, viewport
        // 160x80 at 40 px/tile spans 4.0 x 2.0 tiles -> x in [-1.0, 3.0], y in
        // [1.0, 3.0]. Tile 3 covers [3.0, 4.0] (outside), so visible tiles are
        // -1..=2 and 1..=2.
        let inner = camera_at(CONFIG, grid(10, 10), 1.0, 2.0);
        assert_eq!(
            inner.visible_tiles(Viewport::new(160.0, 80.0)),
            Some(VisibleTileRange {
                min_x: -1,
                max_x: 2,
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
        let config = CameraConfig {
            initial_pixels_per_tile: 10.0,
            min_pixels_per_tile: 5.0,
            max_pixels_per_tile: 160.0,
        };
        let camera = camera_at(config, grid(30, 20), 10.03, 8.0);
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
        let grid = grid(10, 10);
        let origin = WorldPoint::new(0.0, 0.0);
        assert_eq!(
            Camera::new(
                CameraConfig {
                    min_pixels_per_tile: 0.0,
                    ..CONFIG
                },
                grid,
                origin,
            ),
            Err(InvalidCameraConfig::InvalidZoomBounds)
        );
        assert_eq!(
            Camera::new(
                CameraConfig {
                    min_pixels_per_tile: 64.0,
                    max_pixels_per_tile: 32.0,
                    ..CONFIG
                },
                grid,
                origin,
            ),
            Err(InvalidCameraConfig::InvalidZoomBounds)
        );
        assert_eq!(
            Camera::new(CONFIG, grid, WorldPoint::new(f32::NAN, 0.0)),
            Err(InvalidCameraConfig::NonFiniteCentre)
        );
    }
}
