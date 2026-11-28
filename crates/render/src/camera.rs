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
    /// Width and height of one tile in art pixels (for example `16`).
    ///
    /// Zoom snaps to exact physical pixel multiples of one art pixel
    /// (`1×, 2×, 3×, …`), plus `0.5×` when the minimum allows going below `1×`,
    /// so every art pixel maps to an exact block of physical screen pixels.
    pub art_pixels_per_tile: u32,
}

/// Why camera creation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidCameraConfig {
    /// Zoom bounds are not finite and positive with `min <= max`, or the
    /// initial scale is not finite.
    InvalidZoomBounds,
    /// Art pixels per tile is zero.
    InvalidTileSize,
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
            Self::InvalidTileSize => write!(f, "art pixels per tile must be positive"),
            Self::NonFiniteCentre => write!(f, "camera centre must be finite"),
        }
    }
}

impl Error for InvalidCameraConfig {}

/// A crisp physical scale relative to one art pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PixelScale {
    /// Half-size (`0.5×`), matching the atlas's single mipmap level.
    Half,
    /// Exact integer multiple (`n × n` physical pixels per art pixel, `n >= 1`).
    Integer(u32),
}

impl PixelScale {
    const fn index(self) -> i64 {
        match self {
            Self::Half => 0,
            Self::Integer(n) => n as i64,
        }
    }

    fn from_index(index: i64) -> Self {
        if index <= 0 {
            Self::Half
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "index is positive and capped at u16::MAX before casting"
            )]
            Self::Integer(index.min(i64::from(u16::MAX)) as u32)
        }
    }

    fn step(self, steps: i32, min: Self, max: Self) -> Self {
        let target = self
            .index()
            .saturating_add(i64::from(steps))
            .clamp(min.index(), max.index());
        Self::from_index(target)
    }

    fn physical_pixels_per_tile(self, art_pixels_per_tile: u32) -> f32 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "tile art pixel counts are small integers, exact in f32"
        )]
        let art = art_pixels_per_tile as f32;
        match self {
            Self::Half => art * 0.5,
            Self::Integer(n) => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "integer pixel scales are capped at u16::MAX, exact in f32"
                )]
                let factor = n as f32;
                art * factor
            }
        }
    }
}

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
    pixel_scale: PixelScale,
    scale_factor: f32,
    art_pixels_per_tile: u32,
    min_pixels_per_tile: f32,
    max_pixels_per_tile: f32,
    grid: Grid,
}

impl Camera {
    /// Creates a camera over `grid`, the world's shape, centred on `centre`.
    /// Wraps the centre across the seam, clamps it between the north and
    /// south edges, and snaps the initial zoom to the nearest crisp physical
    /// scale at `scale_factor = 1.0`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCameraConfig`] if the zoom bounds are not finite and
    /// positive with `min <= max`, the art tile size is zero, or the initial
    /// scale or the centre is not finite.
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
        if config.art_pixels_per_tile == 0 {
            return Err(InvalidCameraConfig::InvalidTileSize);
        }
        if !centre.x.is_finite() || !centre.y.is_finite() {
            return Err(InvalidCameraConfig::NonFiniteCentre);
        }

        let scale_factor = 1.0;
        let (min_scale, max_scale) = scale_bounds(
            config.min_pixels_per_tile,
            config.max_pixels_per_tile,
            config.art_pixels_per_tile,
            scale_factor,
        );
        let initial_clamped = config
            .initial_pixels_per_tile
            .clamp(config.min_pixels_per_tile, config.max_pixels_per_tile);
        let pixel_scale = snap_scale(
            initial_clamped * scale_factor,
            config.art_pixels_per_tile,
            min_scale,
            max_scale,
        );
        let mut camera = Self {
            centre,
            pixel_scale,
            scale_factor,
            art_pixels_per_tile: config.art_pixels_per_tile,
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

    /// Updates the display's physical-to-logical pixel ratio, re-snapping zoom
    /// to the nearest crisp physical pixel scale that keeps roughly the same
    /// world area visible.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        if !scale_factor.is_finite() {
            return;
        }
        let effective = scale_factor.max(1.0);
        if (effective - self.scale_factor).abs() < 1e-6 {
            return;
        }
        let target_physical = self.pixels_per_tile() * effective;
        self.scale_factor = effective;
        let (min_scale, max_scale) = scale_bounds(
            self.min_pixels_per_tile,
            self.max_pixels_per_tile,
            self.art_pixels_per_tile,
            effective,
        );
        self.pixel_scale = snap_scale(
            target_physical,
            self.art_pixels_per_tile,
            min_scale,
            max_scale,
        );
    }

    /// Current scale in physical pixels per tile.
    #[must_use]
    pub fn physical_pixels_per_tile(&self) -> f32 {
        self.pixel_scale
            .physical_pixels_per_tile(self.art_pixels_per_tile)
    }

    /// Current scale in logical pixels per tile.
    #[must_use]
    pub fn pixels_per_tile(&self) -> f32 {
        self.physical_pixels_per_tile() / self.scale_factor
    }

    /// Converts a point on the window (in logical pixels) to continuous world
    /// tile coordinates.
    #[must_use]
    pub fn screen_to_world(&self, screen: ScreenPoint, viewport: Viewport) -> WorldPoint {
        let viewport_centre = viewport.centre();
        let pixels_per_tile = self.pixels_per_tile();
        WorldPoint::new(
            self.centre.x + (screen.x - viewport_centre.x) / pixels_per_tile,
            self.centre.y + (screen.y - viewport_centre.y) / pixels_per_tile,
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
        let pixels_per_tile = self.pixels_per_tile();
        ScreenPoint::new(
            viewport_centre.x + (world.x - self.centre.x) * pixels_per_tile,
            viewport_centre.y + (world.y - self.centre.y) * pixels_per_tile,
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
        let pixels_per_tile = self.pixels_per_tile();
        self.centre.x += dx / pixels_per_tile;
        self.centre.y += dy / pixels_per_tile;
        self.wrap_centre();
    }

    /// Steps zoom by `steps` crisp physical pixel levels (positive zooms in,
    /// negative zooms out), keeping the world point under `anchor` stationary
    /// on screen.
    ///
    /// Zero steps, clamping at a bound without changing level, or empty
    /// viewports leave the camera unchanged.
    pub fn step_zoom(&mut self, steps: i32, anchor: ScreenPoint, viewport: Viewport) {
        if steps == 0 || !viewport.is_non_empty() {
            return;
        }
        let (min_scale, max_scale) = scale_bounds(
            self.min_pixels_per_tile,
            self.max_pixels_per_tile,
            self.art_pixels_per_tile,
            self.scale_factor,
        );
        let next = self.pixel_scale.step(steps, min_scale, max_scale);
        if next == self.pixel_scale {
            return;
        }
        let anchor_world = self.screen_to_world(anchor, viewport);
        self.pixel_scale = next;

        let pixels_per_tile = self.pixels_per_tile();
        let viewport_centre = viewport.centre();
        self.centre = WorldPoint::new(
            anchor_world.x - (anchor.x - viewport_centre.x) / pixels_per_tile,
            anchor_world.y - (anchor.y - viewport_centre.y) / pixels_per_tile,
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
    /// Snaps the camera translation so world tile edges land on exact physical
    /// pixel boundaries (`scale_factor` physical pixels per logical pixel, or 1
    /// if it is smaller), accounting for the half-pixel centre offset of odd
    /// physical viewport dimensions so adjacent axis-aligned tiles share exact
    /// pixel edges without sub-pixel jitter or half-pixel blur.
    #[must_use]
    pub fn uniform(&self, viewport: Viewport, scale_factor: f32) -> CameraUniform {
        if !viewport.is_non_empty() {
            return CameraUniform {
                scale: [0.0, 0.0],
                centre: [self.centre.x, self.centre.y],
            };
        }
        let effective_scale = scale_factor.max(1.0);
        let physical_px_per_tile = if (effective_scale - self.scale_factor).abs() < 1e-6 {
            self.physical_pixels_per_tile()
        } else {
            self.pixels_per_tile() * effective_scale
        };
        let physical_width = viewport.width * effective_scale;
        let physical_height = viewport.height * effective_scale;
        let centre = [
            snap_centre_axis(self.centre.x, physical_px_per_tile, physical_width),
            snap_centre_axis(self.centre.y, physical_px_per_tile, physical_height),
        ];
        CameraUniform {
            scale: [
                2.0 * physical_px_per_tile / physical_width,
                -2.0 * physical_px_per_tile / physical_height,
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

/// Snaps `centre` along one axis so integer world coordinates map to integer
/// physical pixel boundaries.
///
/// NDC `0` falls at `physical_extent / 2`: an integer pixel boundary when the
/// physical extent is even, or `integer + 0.5` when it is odd. Shifting the
/// rounding grid by `0.5` on odd extents cancels that half-pixel offset.
fn snap_centre_axis(centre: f32, physical_px_per_tile: f32, physical_extent: f32) -> f32 {
    let half_pixel = (physical_extent.round() * 0.5).fract();
    ((centre * physical_px_per_tile - half_pixel).round() + half_pixel) / physical_px_per_tile
}

/// Inclusive crisp physical scale bounds for the configured logical zoom
/// limits at `effective_scale`.
fn scale_bounds(
    min_logical: f32,
    max_logical: f32,
    art_pixels: u32,
    effective_scale: f32,
) -> (PixelScale, PixelScale) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile art pixel counts are small integers, exact in f32"
    )]
    let art = art_pixels as f32;
    let min_phys = min_logical * effective_scale;
    let max_phys = max_logical * effective_scale;

    let min_scale = if min_phys < art {
        PixelScale::Half
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "min_phys >= art > 0 and clamped to 1..=u16::MAX"
        )]
        let n = (min_phys / art).ceil().clamp(1.0, f32::from(u16::MAX)) as u32;
        PixelScale::Integer(n)
    };

    let max_scale = if max_phys < art {
        PixelScale::Half
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "max_phys >= art > 0 and clamped to 1..=u16::MAX"
        )]
        let n = (max_phys / art).floor().clamp(1.0, f32::from(u16::MAX)) as u32;
        PixelScale::Integer(n)
    };

    if min_scale <= max_scale {
        (min_scale, max_scale)
    } else {
        let midpoint = f32::midpoint(min_phys, max_phys);
        let snapped = snap_scale(
            midpoint,
            art_pixels,
            PixelScale::Half,
            PixelScale::Integer(u32::from(u16::MAX)),
        );
        (snapped, snapped)
    }
}

/// Snaps `target_phys` (physical pixels per tile) to the nearest crisp
/// [`PixelScale`] within `[min_scale, max_scale]`.
fn snap_scale(
    target_phys: f32,
    art_pixels: u32,
    min_scale: PixelScale,
    max_scale: PixelScale,
) -> PixelScale {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile art pixel counts are small integers, exact in f32"
    )]
    let art = art_pixels as f32;
    let raw = if target_phys < art * 0.75 {
        PixelScale::Half
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "target_phys >= 0.75 * art > 0 and clamped to 1..=u16::MAX"
        )]
        let n = (target_phys / art).round().clamp(1.0, f32::from(u16::MAX)) as u32;
        PixelScale::Integer(n)
    };
    raw.clamp(min_scale, max_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: CameraConfig = CameraConfig {
        initial_pixels_per_tile: 40.0,
        min_pixels_per_tile: 10.0,
        max_pixels_per_tile: 160.0,
        art_pixels_per_tile: 10,
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
    /// point under the cursor fixed on screen, or stepping by the wrong number
    /// of art-pixel multiples.
    #[test]
    fn zooming_keeps_the_world_point_under_the_anchor_fixed() {
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let cursor = ScreenPoint::new(600.0, 140.0);

        let before = camera.screen_to_world(cursor, viewport);
        // CONFIG has art_pixels_per_tile = 10, starting at 4x (40 px/tile).
        // +2 steps -> 6x (60 px/tile).
        camera.step_zoom(2, cursor, viewport);
        let after = camera.screen_to_world(cursor, viewport);

        assert_close(camera.pixels_per_tile(), 60.0);
        assert_close(after.x, before.x);
        assert_close(after.y, before.y);
    }

    /// Catches zoom escaping `[min, max]`, skipping the `0.5x` mipmap step,
    /// drifting the anchor when clamped, or acting on zero steps / empty
    /// viewports.
    #[test]
    fn zooming_steps_through_half_and_integer_scales_and_clamps_to_limits() {
        let config = CameraConfig {
            initial_pixels_per_tile: 48.0,
            min_pixels_per_tile: 8.0,
            max_pixels_per_tile: 96.0,
            art_pixels_per_tile: 16,
        };
        let mut camera = camera_at(config, grid(30, 20), 10.0, 8.0);
        let viewport = Viewport::new(800.0, 600.0);
        let cursor = ScreenPoint::new(520.0, 260.0);
        let before = camera.screen_to_world(cursor, viewport);

        // +10 steps from 3x (48) clamps at max 6x (96).
        camera.step_zoom(10, cursor, viewport);
        assert_close(camera.pixels_per_tile(), 96.0);
        let at_max = camera.screen_to_world(cursor, viewport);
        assert_close(at_max.x, before.x);
        assert_close(at_max.y, before.y);

        // Stepping past max does not drift the camera at all.
        let clamped_state = camera.clone();
        camera.step_zoom(1, cursor, viewport);
        assert_eq!(camera, clamped_state);

        // Step down to 1x (16), then one more step reaches 0.5x (8).
        camera.step_zoom(-5, viewport.centre(), viewport);
        assert_close(camera.pixels_per_tile(), 16.0);
        camera.step_zoom(-1, viewport.centre(), viewport);
        assert_close(camera.pixels_per_tile(), 8.0);
        // Stepping back up from 0.5x lands on 1x (16), then 2x (32).
        camera.step_zoom(1, cursor, viewport);
        assert_close(camera.pixels_per_tile(), 16.0);
        camera.step_zoom(1, cursor, viewport);
        assert_close(camera.pixels_per_tile(), 32.0);

        let state = camera.clone();
        camera.step_zoom(0, cursor, viewport);
        camera.step_zoom(1, cursor, Viewport::new(0.0, 600.0));
        camera.step_zoom(1, cursor, Viewport::new(800.0, 0.0));
        assert_eq!(camera, state);
    }

    /// Catches fractional `HiDPI` scale factors (such as Windows 125% or 150%)
    /// producing non-integer physical pixels per art pixel.
    #[test]
    fn scale_factor_changes_snap_to_exact_physical_art_pixel_multiples() {
        let config = CameraConfig {
            initial_pixels_per_tile: 48.0,
            min_pixels_per_tile: 8.0,
            max_pixels_per_tile: 192.0,
            art_pixels_per_tile: 16,
        };
        let mut camera = camera_at(config, grid(30, 20), 10.0, 8.0);

        // At 1.25x DPI: 48 * 1.25 = 60 -> snaps to 4x (64 physical px/tile, 51.2 logical).
        camera.set_scale_factor(1.25);
        assert_close(camera.physical_pixels_per_tile(), 64.0);
        assert_close(camera.pixels_per_tile(), 64.0 / 1.25);

        // At 1.5x DPI: 51.2 * 1.5 = 76.8 -> snaps to 5x (80 physical px/tile).
        camera.set_scale_factor(1.5);
        assert_close(camera.physical_pixels_per_tile(), 80.0);
        assert_close(camera.pixels_per_tile(), 80.0 / 1.5);

        // Stepping at 1.5x DPI moves by exact 16 physical px/tile (80 -> 96 = 6x).
        camera.step_zoom(
            1,
            ScreenPoint::new(400.0, 300.0),
            Viewport::new(800.0, 600.0),
        );
        assert_close(camera.physical_pixels_per_tile(), 96.0);
        assert_close(camera.pixels_per_tile(), 64.0);

        // At 2.0x DPI, min logical 8.0 -> 16.0 physical (1x), so 0.5x is excluded.
        camera.set_scale_factor(2.0);
        camera.step_zoom(
            -100,
            ScreenPoint::new(400.0, 300.0),
            Viewport::new(800.0, 600.0),
        );
        assert_close(camera.physical_pixels_per_tile(), 16.0);
        assert_close(camera.pixels_per_tile(), 8.0);
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
            art_pixels_per_tile: 10,
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

    /// Catches half-pixel misalignment on odd physical viewports, where
    /// snapping `centre * physical_px_per_tile` to an integer leaves every tile
    /// edge at `N + 0.5` in framebuffer pixels and blurs every art-pixel edge.
    #[test]
    fn gpu_uniform_aligns_tile_edges_to_integer_pixels_on_odd_viewports() {
        let config = CameraConfig {
            initial_pixels_per_tile: 16.0,
            min_pixels_per_tile: 8.0,
            max_pixels_per_tile: 160.0,
            art_pixels_per_tile: 16,
        };
        let camera = camera_at(config, grid(30, 20), 10.0, 8.0);
        let cases = [
            // Odd width and odd height at 1x DPI.
            (Viewport::new(801.0, 401.0), 1.0_f32),
            // Even width, odd height (catches swapped X/Y viewport extents).
            (Viewport::new(800.0, 401.0), 1.0_f32),
            // Odd physical width (801 px) and even physical height (450 px) at 1.25x DPI
            // -> 20 physical px/tile (integer).
            (Viewport::new(640.8, 360.0), 1.25_f32),
        ];

        for (viewport, scale_factor) in cases {
            let uniform = camera.uniform(viewport, scale_factor);
            let physical_w = (viewport.width * scale_factor).round();
            let physical_h = (viewport.height * scale_factor).round();
            let physical_px_per_tile = config.initial_pixels_per_tile * scale_factor;

            // Snapped centre must stay within half a physical pixel of the camera.
            assert!(
                (uniform.centre[0] - camera.centre().x).abs() * physical_px_per_tile <= 0.5 + 1e-4
            );
            assert!(
                (uniform.centre[1] - camera.centre().y).abs() * physical_px_per_tile <= 0.5 + 1e-4
            );

            // Tile edge (10, 8) must land on an integer physical pixel boundary.
            let ndc_x = (10.0 - uniform.centre[0]) * uniform.scale[0];
            let ndc_y = (8.0 - uniform.centre[1]) * uniform.scale[1];
            let fb_x = (ndc_x * 0.5 + 0.5) * physical_w;
            let fb_y = (0.5 - ndc_y * 0.5) * physical_h;
            assert_close(fb_x, fb_x.round());
            assert_close(fb_y, fb_y.round());
        }
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
            Camera::new(
                CameraConfig {
                    art_pixels_per_tile: 0,
                    ..CONFIG
                },
                grid,
                origin,
            ),
            Err(InvalidCameraConfig::InvalidTileSize)
        );
        assert_eq!(
            Camera::new(CONFIG, grid, WorldPoint::new(f32::NAN, 0.0)),
            Err(InvalidCameraConfig::NonFiniteCentre)
        );
    }
}
