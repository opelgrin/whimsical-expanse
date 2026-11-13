//! Camera controls: keyboard panning, grab-and-drag panning and
//! cursor-anchored zoom, driven by [`InputState`] once per frame.

use std::time::Duration;

use expanse_render::{Camera, Viewport};

use crate::input::{InputAction, InputState};

/// Tuning for camera movement and zoom.
///
/// Callers state every parameter explicitly so tests never rely on game
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraControlsConfig {
    /// Keyboard panning speed in logical screen pixels per second.
    pub pan_speed_pixels_per_second: f32,
    /// Scale multiplier per scroll line, e.g. `1.15`. Above `1.0`, scrolling
    /// up zooms in. Zero or negative disables zoom.
    pub zoom_step_factor: f32,
}

/// Moves a [`Camera`] according to the input of one frame.
#[derive(Debug, Clone)]
pub struct CameraControls {
    config: CameraControlsConfig,
}

impl CameraControls {
    /// Creates camera controls tuned by `config`.
    #[must_use]
    pub const fn new(config: CameraControlsConfig) -> Self {
        Self { config }
    }

    /// Applies one frame of `input`, lasting `dt`, to `camera`.
    ///
    /// Dragging applies before zooming, so a zoom anchors on the world point
    /// that is under the cursor once the drag has moved it.
    pub fn update(
        &self,
        input: &InputState,
        dt: Duration,
        viewport: Viewport,
        camera: &mut Camera,
    ) {
        Self::drag(input, camera);
        self.zoom(input, viewport, camera);
        self.pan_with_keys(input, dt, camera);
    }

    fn drag(input: &InputState, camera: &mut Camera) {
        if !input.is_held(InputAction::DragPan) {
            return;
        }
        // Dragging the cursor right pulls the world right, which moves the
        // camera centre left.
        let motion = input.cursor_motion();
        camera.pan_by_screen_offset(-motion.dx, -motion.dy);
    }

    fn zoom(&self, input: &InputState, viewport: Viewport, camera: &mut Camera) {
        let lines = input.scroll_lines();
        if self.config.zoom_step_factor <= 0.0 || lines == 0.0 {
            return;
        }
        let factor = self.config.zoom_step_factor.powf(lines);
        let anchor = input.cursor().unwrap_or_else(|| viewport.centre());
        camera.zoom_by(factor, anchor, viewport);
    }

    /// Diagonal movement is normalized so it moves at the same screen speed as
    /// cardinal movement.
    fn pan_with_keys(&self, input: &InputState, dt: Duration, camera: &mut Camera) {
        let x_axis = axis(
            input.is_held(InputAction::PanLeft),
            input.is_held(InputAction::PanRight),
        );
        let y_axis = axis(
            input.is_held(InputAction::PanUp),
            input.is_held(InputAction::PanDown),
        );
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
    use expanse_render::{CameraConfig, ScreenPoint, WorldPoint};

    use super::*;
    use crate::input::{Bindings, Button, InputConfig, KeyCode, MouseButton, ScrollAmount};

    const UP: Button = Button::Key(KeyCode::KeyW);
    const DOWN: Button = Button::Key(KeyCode::KeyS);
    const LEFT: Button = Button::Key(KeyCode::KeyA);
    const RIGHT: Button = Button::Key(KeyCode::KeyD);
    const DRAG: Button = Button::Mouse(MouseButton::Middle);

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

    fn controls() -> CameraControls {
        CameraControls::new(CameraControlsConfig {
            pan_speed_pixels_per_second: 500.0,
            zoom_step_factor: 2.0,
        })
    }

    fn input() -> InputState {
        InputState::new(InputConfig {
            bindings: Bindings::new()
                .with(UP, InputAction::PanUp)
                .with(DOWN, InputAction::PanDown)
                .with(LEFT, InputAction::PanLeft)
                .with(RIGHT, InputAction::PanRight)
                .with(DRAG, InputAction::DragPan),
            pixels_per_scroll_line: 40.0,
        })
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}"
        );
    }

    /// Catches wrong direction signs for any of the four pan actions, opposing
    /// directions not cancelling out, and diagonal panning moving `sqrt(2)`
    /// times faster than cardinal panning.
    #[test]
    fn keyboard_panning_moves_in_all_directions_and_normalizes_diagonals() {
        let dt = Duration::from_millis(500);
        let viewport = Viewport::new(800.0, 600.0);
        // 500 px/s * 0.5 s = 250 px; at 50 px/tile = 5 tiles.
        let controls = controls();
        let mut input = input();
        let mut camera = test_camera();

        input.button_changed(LEFT, true);
        controls.update(&input, dt, viewport, &mut camera);
        assert_close(camera.centre().x, 15.0);
        assert_close(camera.centre().y, 20.0);

        input.button_changed(LEFT, false);
        input.button_changed(UP, true);
        controls.update(&input, dt, viewport, &mut camera);
        assert_close(camera.centre().x, 15.0);
        assert_close(camera.centre().y, 15.0);

        input.button_changed(DOWN, true);
        controls.update(&input, dt, viewport, &mut camera);
        assert_close(camera.centre().y, 15.0);

        // Diagonal Right + Down moves a total Euclidean distance of 5 tiles.
        input.button_changed(UP, false);
        input.button_changed(RIGHT, true);
        let mut diagonal = test_camera();
        controls.update(&input, dt, viewport, &mut diagonal);
        let dx = diagonal.centre().x - 20.0;
        let dy = diagonal.centre().y - 20.0;
        assert!(dx > 0.0 && dy > 0.0);
        assert_close(dx.hypot(dy), 5.0);
    }

    /// Catches inverted drag direction (the grabbed world tile must stay under
    /// the cursor) and the cursor moving the camera when no drag is held.
    #[test]
    fn dragging_keeps_the_grabbed_world_point_under_the_cursor() {
        let controls = controls();
        let mut input = input();
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let dt = Duration::ZERO;

        let start = ScreenPoint::new(300.0, 250.0);
        let end = ScreenPoint::new(450.0, 350.0);
        input.cursor_moved(ScreenPoint::new(100.0, 100.0));
        input.cursor_moved(start);
        controls.update(&input, dt, viewport, &mut camera);
        input.end_frame();
        assert_eq!(camera, test_camera());

        let grabbed_world = camera.screen_to_world(start, viewport);
        input.button_changed(DRAG, true);
        input.cursor_moved(ScreenPoint::new(400.0, 280.0));
        input.cursor_moved(end);
        controls.update(&input, dt, viewport, &mut camera);
        let under_cursor_after = camera.screen_to_world(end, viewport);

        assert_close(under_cursor_after.x, grabbed_world.x);
        assert_close(under_cursor_after.y, grabbed_world.y);
    }

    /// Catches scrolling scaling linearly instead of exponentially, zooming
    /// around the viewport centre while the cursor is known (or around a stale
    /// cursor after it left), and a disabled zoom factor still zooming.
    #[test]
    fn scroll_zooms_exponentially_around_cursor_or_viewport_centre() {
        let controls = controls();
        let mut input = input();
        let mut camera = test_camera();
        let viewport = Viewport::new(800.0, 600.0);
        let dt = Duration::ZERO;
        let cursor = ScreenPoint::new(200.0, 150.0);
        input.cursor_moved(cursor);
        let anchor_before = camera.screen_to_world(cursor, viewport);

        // +2 lines; factor 2.0^2 = 4x (50 -> 200 px/tile).
        input.scrolled(ScrollAmount::Lines(2.0));
        controls.update(&input, dt, viewport, &mut camera);
        input.end_frame();
        assert_close(camera.pixels_per_tile(), 200.0);
        let anchor_after = camera.screen_to_world(cursor, viewport);
        assert_close(anchor_after.x, anchor_before.x);
        assert_close(anchor_after.y, anchor_before.y);

        // After the cursor leaves the window, zooming anchors at viewport centre.
        input.cursor_left();
        let centre_before = camera.centre();
        input.scrolled(ScrollAmount::Lines(-1.0));
        controls.update(&input, dt, viewport, &mut camera);
        assert_close(camera.pixels_per_tile(), 100.0);
        assert_close(camera.centre().x, centre_before.x);
        assert_close(camera.centre().y, centre_before.y);

        let before_disabled = camera.clone();
        let disabled = CameraControls::new(CameraControlsConfig {
            pan_speed_pixels_per_second: 500.0,
            zoom_step_factor: 0.0,
        });
        disabled.update(&input, dt, viewport, &mut camera);
        assert_eq!(camera, before_disabled);
    }
}
