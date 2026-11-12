//! Camera input state and controls.
//!
//! Kept independent of `winit` event types so the behaviour can be tested
//! without a window. Physical key positions (`WASD` / `ZQSD` on AZERTY, plus
//! arrow keys), middle-mouse drag, and mouse-wheel zoom drive a
//! [`expanse_render::Camera`].

use std::time::Duration;

use expanse_render::{Camera, ScreenPoint, Viewport};

/// Tuning for camera movement and zoom.
///
/// Callers state every parameter explicitly so tests never rely on game
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraControlsConfig {
    /// Keyboard panning speed in logical screen pixels per second.
    pub pan_speed_pixels_per_second: f32,
    /// Scale multiplier per discrete scroll notch, e.g. `1.15`. Above `1.0`
    /// zooms in and below it zooms out. Zero or negative disables zoom.
    pub zoom_step_factor: f32,
    /// Logical scroll pixels treated as one discrete scroll notch for smooth
    /// trackpads. Zero or negative ignores pixel scrolling.
    pub pixels_per_scroll_line: f32,
}

/// Cardinal camera pan keys (mapped from physical WASD and arrow keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanKey {
    Up,
    Down,
    Left,
    Right,
}

/// Unit of a mouse-wheel or trackpad scroll event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAmount {
    /// Discrete wheel notches (positive scrolls up / zooms in).
    Lines(f32),
    /// Smooth pixel scrolling in logical pixels (positive scrolls up / zooms in).
    Pixels(f32),
}

/// Distinguishes the two physical keys bound to each pan direction (for example
/// `KeyW` and `ArrowUp`), so that each binding is tracked on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanBinding {
    Primary(PanKey),
    Secondary(PanKey),
}

impl PanBinding {
    const fn slot(self) -> usize {
        match self {
            Self::Primary(PanKey::Up) => 0,
            Self::Primary(PanKey::Down) => 1,
            Self::Primary(PanKey::Left) => 2,
            Self::Primary(PanKey::Right) => 3,
            Self::Secondary(PanKey::Up) => 4,
            Self::Secondary(PanKey::Down) => 5,
            Self::Secondary(PanKey::Left) => 6,
            Self::Secondary(PanKey::Right) => 7,
        }
    }
}

/// Tracks held keys, middle-mouse dragging, and cursor position to drive a
/// [`Camera`].
#[derive(Debug, Clone)]
pub struct CameraController {
    config: CameraControlsConfig,
    held_pan_keys: [bool; 8],
    dragging: bool,
    cursor: Option<ScreenPoint>,
}

impl CameraController {
    /// Creates an idle camera controller with `config`.
    #[must_use]
    pub const fn new(config: CameraControlsConfig) -> Self {
        Self {
            config,
            held_pan_keys: [false; 8],
            dragging: false,
            cursor: None,
        }
    }

    /// Records a press or release of a pan key binding. Repeated press events
    /// while a key is already down are idempotent.
    pub const fn set_pan_key(&mut self, binding: PanBinding, pressed: bool) {
        self.held_pan_keys[binding.slot()] = pressed;
    }

    /// Starts or ends middle-mouse grab-and-drag panning.
    pub const fn set_dragging(&mut self, dragging: bool) {
        self.dragging = dragging;
    }

    /// Clears all held inputs (for example when the window loses focus).
    pub const fn clear_inputs(&mut self) {
        self.held_pan_keys = [false; 8];
        self.dragging = false;
    }

    /// Forgets the cursor position when the pointer leaves the window, so
    /// subsequent scroll wheel events zoom around the viewport centre instead
    /// of a stale edge coordinate.
    pub const fn cursor_left(&mut self) {
        self.cursor = None;
    }

    /// Updates the cursor position in logical pixels, panning `camera` 1:1 with
    /// the world if middle-mouse drag is active.
    pub fn update_cursor(&mut self, position: ScreenPoint, camera: &mut Camera) {
        let previous = self.cursor.replace(position);
        if let Some(prev) = previous
            && self.dragging
        {
            // Dragging the cursor right pulls the world right, which moves the
            // camera centre left (`-(position - prev)`).
            camera.pan_by_screen_offset(prev.x - position.x, prev.y - position.y);
        }
    }

    /// Zooms `camera` around the current cursor position (or viewport centre if
    /// no cursor position is known).
    pub fn zoom(&self, scroll: ScrollAmount, viewport: Viewport, camera: &mut Camera) {
        if self.config.zoom_step_factor <= 0.0 {
            return;
        }
        let lines = match scroll {
            ScrollAmount::Lines(lines) => lines,
            ScrollAmount::Pixels(pixels) => {
                if self.config.pixels_per_scroll_line <= 0.0 {
                    return;
                }
                pixels / self.config.pixels_per_scroll_line
            }
        };
        let factor = self.config.zoom_step_factor.powf(lines);
        let anchor = self.cursor.unwrap_or_else(|| viewport.centre());
        camera.zoom_by(factor, anchor, viewport);
    }

    /// Advances continuous keyboard panning over `dt`. Diagonal movement is
    /// normalized so it moves at the same screen speed as cardinal movement.
    pub fn advance(&self, dt: Duration, camera: &mut Camera) {
        let left = self.is_direction_held(PanKey::Left);
        let right = self.is_direction_held(PanKey::Right);
        let up = self.is_direction_held(PanKey::Up);
        let down = self.is_direction_held(PanKey::Down);

        let x_axis = axis(left, right);
        let y_axis = axis(up, down);
        if x_axis == 0.0 && y_axis == 0.0 {
            return;
        }
        let secs = dt.as_secs_f32();
        if secs <= 0.0 {
            return;
        }
        let length = x_axis.hypot(y_axis);
        let distance = self.config.pan_speed_pixels_per_second * secs;
        camera.pan_by_screen_offset(x_axis / length * distance, y_axis / length * distance);
    }

    /// A direction is held while either of its bindings is. Releasing one
    /// binding while the other stays down keeps panning.
    const fn is_direction_held(&self, dir: PanKey) -> bool {
        self.held_pan_keys[PanBinding::Primary(dir).slot()]
            || self.held_pan_keys[PanBinding::Secondary(dir).slot()]
    }
}

const fn axis(negative: bool, positive: bool) -> f32 {
    match (negative, positive) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use expanse_render::{CameraConfig, WorldPoint};

    use super::*;

    fn test_camera() -> Camera {
        Camera::new(
            CameraConfig {
                initial_centre: WorldPoint::new(20.0, 20.0),
                initial_pixels_per_tile: 50.0,
                min_pixels_per_tile: 10.0,
                max_pixels_per_tile: 200.0,
            },
            100,
            100,
        )
        .expect("valid camera")
    }

    fn controller() -> CameraController {
        CameraController::new(CameraControlsConfig {
            pan_speed_pixels_per_second: 500.0,
            zoom_step_factor: 2.0,
            pixels_per_scroll_line: 40.0,
        })
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}"
        );
    }

    /// Catches wrong direction signs for any of the four cardinal keys,
    /// diagonal keyboard panning moving `sqrt(2)` times faster than cardinal
    /// movement, or releasing one of two redundant bindings (`W` + `ArrowUp`)
    /// stopping movement prematurely.
    #[test]
    fn keyboard_panning_moves_in_all_directions_and_normalizes_diagonals() {
        let dt = Duration::from_millis(500);
        // 500 px/s * 0.5 s = 250 px; at 50 px/tile = 5 tiles.
        let mut ctrl = controller();
        let mut camera = test_camera();

        // Redundant bindings for Left: releasing Secondary keeps Primary active.
        ctrl.set_pan_key(PanBinding::Primary(PanKey::Left), true);
        ctrl.set_pan_key(PanBinding::Secondary(PanKey::Left), true);
        ctrl.set_pan_key(PanBinding::Secondary(PanKey::Left), false);
        ctrl.advance(dt, &mut camera);
        assert_close(camera.centre().x, 15.0);
        assert_close(camera.centre().y, 20.0);

        // Secondary binding alone for Up works too.
        ctrl.set_pan_key(PanBinding::Primary(PanKey::Left), false);
        ctrl.set_pan_key(PanBinding::Secondary(PanKey::Up), true);
        ctrl.advance(dt, &mut camera);
        assert_close(camera.centre().x, 15.0);
        assert_close(camera.centre().y, 15.0);

        // Opposing keys cancel out.
        ctrl.set_pan_key(PanBinding::Primary(PanKey::Down), true);
        ctrl.advance(dt, &mut camera);
        assert_close(camera.centre().y, 15.0);

        // Diagonal Right + Down moves total Euclidean distance of 5 tiles.
        ctrl.set_pan_key(PanBinding::Secondary(PanKey::Up), false);
        ctrl.set_pan_key(PanBinding::Secondary(PanKey::Right), true);
        let mut diagonal = test_camera();
        ctrl.advance(dt, &mut diagonal);
        let dx = diagonal.centre().x - 20.0;
        let dy = diagonal.centre().y - 20.0;
        assert!(dx > 0.0 && dy > 0.0);
        assert_close(dx.hypot(dy), 5.0);
    }

    /// Catches inverted drag direction: dragging the cursor right and down must
    /// keep the grabbed world tile directly under the cursor.
    #[test]
    fn middle_mouse_drag_keeps_the_grabbed_world_point_under_the_cursor() {
        let mut ctrl = controller();
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);

        let start = ScreenPoint::new(300.0, 250.0);
        let end = ScreenPoint::new(450.0, 350.0);
        ctrl.update_cursor(start, &mut camera);
        let grabbed_world = camera.screen_to_world(start, viewport);

        ctrl.set_dragging(true);
        ctrl.update_cursor(end, &mut camera);
        let under_cursor_after = camera.screen_to_world(end, viewport);

        assert_close(under_cursor_after.x, grabbed_world.x);
        assert_close(under_cursor_after.y, grabbed_world.y);
    }

    /// Catches pixel-scroll events scaling linearly instead of exponentially,
    /// zooming around a stale cursor after `cursor_left`, or poisoning state on
    /// zero/NaN scroll inputs.
    #[test]
    fn scroll_zooms_exponentially_around_cursor_or_viewport_centre() {
        let mut ctrl = controller();
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let cursor = ScreenPoint::new(200.0, 150.0);
        ctrl.update_cursor(cursor, &mut camera);
        let anchor_before = camera.screen_to_world(cursor, viewport);

        // 80 scroll pixels at 40 px/line = +2 lines; factor 2.0^2 = 4x (50 -> 200 px/tile).
        ctrl.zoom(ScrollAmount::Pixels(80.0), viewport, &mut camera);
        assert_close(camera.pixels_per_tile(), 200.0);
        let anchor_after = camera.screen_to_world(cursor, viewport);
        assert_close(anchor_after.x, anchor_before.x);
        assert_close(anchor_after.y, anchor_before.y);

        // After the cursor leaves the window, zooming anchors at viewport centre.
        ctrl.cursor_left();
        let centre_before = camera.centre();
        ctrl.zoom(ScrollAmount::Lines(-1.0), viewport, &mut camera);
        assert_close(camera.pixels_per_tile(), 100.0);
        assert_close(camera.centre().x, centre_before.x);
        assert_close(camera.centre().y, centre_before.y);

        // Zero, NaN, and non-positive config values leave the camera unchanged.
        let before_noop = camera.clone();
        ctrl.zoom(ScrollAmount::Lines(0.0), viewport, &mut camera);
        ctrl.zoom(ScrollAmount::Lines(f32::NAN), viewport, &mut camera);
        let invalid_zoom_ctrl = CameraController::new(CameraControlsConfig {
            pan_speed_pixels_per_second: 500.0,
            zoom_step_factor: 0.0,
            pixels_per_scroll_line: 0.0,
        });
        invalid_zoom_ctrl.zoom(ScrollAmount::Lines(1.0), viewport, &mut camera);
        invalid_zoom_ctrl.zoom(ScrollAmount::Pixels(40.0), viewport, &mut camera);
        assert_eq!(camera, before_noop);
    }

    /// Catches stuck keys or stuck dragging when window focus is lost.
    #[test]
    fn clearing_inputs_stops_panning_and_dragging() {
        let mut ctrl = controller();
        let mut camera = test_camera();
        ctrl.set_pan_key(PanBinding::Primary(PanKey::Left), true);
        ctrl.set_dragging(true);
        ctrl.update_cursor(ScreenPoint::new(100.0, 100.0), &mut camera);

        ctrl.clear_inputs();
        ctrl.advance(Duration::from_secs(1), &mut camera);
        ctrl.update_cursor(ScreenPoint::new(200.0, 200.0), &mut camera);
        assert_close(camera.centre().x, 20.0);
        assert_close(camera.centre().y, 20.0);
    }
}
