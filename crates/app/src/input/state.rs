//! What the player is doing with the keyboard and mouse, gathered between
//! frames.

use expanse_render::ScreenPoint;

use super::bindings::{Bindings, Button, InputAction};

/// Configuration of [`InputState`].
#[derive(Debug, Clone, PartialEq)]
pub struct InputConfig {
    /// Which buttons trigger which input actions.
    pub bindings: Bindings,
    /// Logical scroll pixels treated as one discrete scroll line, so smooth
    /// trackpads and notched wheels scroll alike. Zero or negative ignores
    /// pixel scrolling.
    pub pixels_per_scroll_line: f32,
}

/// Unit of a mouse-wheel or trackpad scroll event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAmount {
    /// Discrete wheel lines (positive scrolls up).
    Lines(f32),
    /// Smooth scrolling in logical pixels (positive scrolls up).
    Pixels(f32),
}

/// How far the cursor moved, in logical pixels (`+x` right, `+y` down).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CursorMotion {
    pub dx: f32,
    pub dy: f32,
}

/// Held buttons, the cursor, and what happened since the last frame.
///
/// The window reports events as they arrive. Consumers read the state during
/// the frame, then the window calls [`end_frame`](Self::end_frame), which
/// forgets presses, cursor motion and scrolling but keeps what is still held.
#[derive(Debug, Clone)]
pub struct InputState {
    config: InputConfig,
    held: Vec<Button>,
    pressed: Vec<InputAction>,
    cursor: Option<ScreenPoint>,
    cursor_motion: CursorMotion,
    scroll_lines: f32,
}

impl InputState {
    /// Creates a state with nothing held, configured by `config`.
    #[must_use]
    pub const fn new(config: InputConfig) -> Self {
        Self {
            config,
            held: Vec::new(),
            pressed: Vec::new(),
            cursor: None,
            cursor_motion: CursorMotion { dx: 0.0, dy: 0.0 },
            scroll_lines: 0.0,
        }
    }

    /// Records a press or release of `button`. A press of a button already
    /// held, such as a key repeated by the operating system, is not a new
    /// press.
    pub fn button_changed(&mut self, button: Button, pressed: bool) {
        let was_held = self.held.contains(&button);
        if pressed && !was_held {
            self.held.push(button);
            self.pressed
                .extend(self.config.bindings.actions_for(button));
        } else if !pressed && was_held {
            self.held.retain(|&held| held != button);
        }
    }

    /// Records the cursor at `position`, in logical pixels.
    pub fn cursor_moved(&mut self, position: ScreenPoint) {
        if let Some(previous) = self.cursor.replace(position) {
            self.cursor_motion.dx += position.x - previous.x;
            self.cursor_motion.dy += position.y - previous.y;
        }
    }

    /// Forgets the cursor when it leaves the window, so that a stale edge
    /// position is not used and re-entering elsewhere is not a jump.
    pub const fn cursor_left(&mut self) {
        self.cursor = None;
    }

    /// Records a scroll event. Non-finite amounts are ignored.
    pub fn scrolled(&mut self, amount: ScrollAmount) {
        let lines = match amount {
            ScrollAmount::Lines(lines) => lines,
            ScrollAmount::Pixels(pixels) => {
                if self.config.pixels_per_scroll_line <= 0.0 {
                    return;
                }
                pixels / self.config.pixels_per_scroll_line
            }
        };
        if lines.is_finite() {
            self.scroll_lines += lines;
        }
    }

    /// Releases every button and drops pending cursor motion, because the
    /// window gets no release events while it is out of focus.
    pub fn focus_lost(&mut self) {
        self.held.clear();
        self.cursor_motion = CursorMotion::default();
    }

    /// Whether any button bound to `action` is held.
    #[must_use]
    pub fn is_held(&self, action: InputAction) -> bool {
        self.held
            .iter()
            .any(|&button| self.config.bindings.is_bound(button, action))
    }

    /// The actions pressed since the last frame, in order. An action pressed
    /// twice appears twice.
    #[must_use]
    pub fn pressed_actions(&self) -> &[InputAction] {
        &self.pressed
    }

    /// The cursor position in logical pixels, or `None` if it is outside the
    /// window.
    #[must_use]
    pub const fn cursor(&self) -> Option<ScreenPoint> {
        self.cursor
    }

    /// How far the cursor moved since the last frame.
    #[must_use]
    pub const fn cursor_motion(&self) -> CursorMotion {
        self.cursor_motion
    }

    /// Scroll lines since the last frame (positive scrolls up).
    #[must_use]
    pub const fn scroll_lines(&self) -> f32 {
        self.scroll_lines
    }

    /// Forgets what happened during the frame: presses, cursor motion and
    /// scrolling. Held buttons and the cursor position remain.
    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.cursor_motion = CursorMotion::default();
        self.scroll_lines = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use winit::event::MouseButton;
    use winit::keyboard::KeyCode;

    use super::*;

    const W: Button = Button::Key(KeyCode::KeyW);
    const UP: Button = Button::Key(KeyCode::ArrowUp);
    const ESCAPE: Button = Button::Key(KeyCode::Escape);
    const MIDDLE: Button = Button::Mouse(MouseButton::Middle);

    fn input() -> InputState {
        InputState::new(InputConfig {
            bindings: Bindings::new()
                .with(W, InputAction::PanUp)
                .with(UP, InputAction::PanUp)
                .with(ESCAPE, InputAction::TogglePause)
                .with(MIDDLE, InputAction::DragPan),
            pixels_per_scroll_line: 40.0,
        })
    }

    /// Catches releasing one of two buttons bound to an action ending the
    /// action while the other is still down, and the operating system's key
    /// repeat counting as new presses (a held pause key toggling repeatedly,
    /// or one release not undoing several repeated presses).
    #[test]
    fn action_is_held_while_any_bound_button_is_and_repeats_are_not_presses() {
        let mut input = input();

        input.button_changed(W, true);
        input.button_changed(UP, true);
        input.button_changed(UP, false);
        assert!(input.is_held(InputAction::PanUp));
        input.button_changed(W, false);
        assert!(!input.is_held(InputAction::PanUp));

        input.end_frame();
        input.button_changed(ESCAPE, true);
        input.button_changed(ESCAPE, true);
        input.button_changed(ESCAPE, true);
        assert_eq!(input.pressed_actions(), [InputAction::TogglePause]);
        input.button_changed(ESCAPE, false);
        assert!(!input.is_held(InputAction::TogglePause));
    }

    /// Catches per-frame input that is never cleared (scrolling zooming on
    /// every frame, a single press toggling pause every frame), and clearing
    /// that also drops held buttons or the cursor, which would stop continuous
    /// panning after one frame.
    #[test]
    fn end_frame_forgets_what_happened_but_keeps_what_is_held() {
        let mut input = input();
        input.cursor_moved(ScreenPoint::new(10.0, 10.0));
        input.button_changed(W, true);
        input.button_changed(ESCAPE, true);
        input.cursor_moved(ScreenPoint::new(15.0, 7.0));
        input.cursor_moved(ScreenPoint::new(20.0, 4.0));
        // 1 line + 80 px at 40 px per line = 3 lines.
        input.scrolled(ScrollAmount::Lines(1.0));
        input.scrolled(ScrollAmount::Pixels(80.0));
        input.scrolled(ScrollAmount::Lines(f32::NAN));

        assert_eq!(input.cursor_motion(), CursorMotion { dx: 10.0, dy: -6.0 });
        assert!((input.scroll_lines() - 3.0).abs() < 1e-6);

        input.end_frame();
        assert_eq!(input.pressed_actions(), []);
        assert_eq!(input.cursor_motion(), CursorMotion::default());
        assert!(input.scroll_lines().abs() < f32::EPSILON);
        assert!(input.is_held(InputAction::PanUp));
        assert_eq!(input.cursor(), Some(ScreenPoint::new(20.0, 4.0)));
    }

    /// Catches stuck keys or a stuck drag after the window loses focus, and a
    /// cursor that re-enters the window elsewhere being read as one long
    /// motion.
    #[test]
    fn focus_loss_releases_buttons_and_reentry_is_not_motion() {
        let mut input = input();
        input.button_changed(W, true);
        input.button_changed(MIDDLE, true);
        input.cursor_moved(ScreenPoint::new(100.0, 100.0));
        input.cursor_moved(ScreenPoint::new(110.0, 100.0));

        input.focus_lost();
        assert!(!input.is_held(InputAction::PanUp));
        assert!(!input.is_held(InputAction::DragPan));
        assert_eq!(input.cursor_motion(), CursorMotion::default());

        input.cursor_left();
        input.cursor_moved(ScreenPoint::new(500.0, 400.0));
        assert_eq!(input.cursor_motion(), CursorMotion::default());
        assert_eq!(input.cursor(), Some(ScreenPoint::new(500.0, 400.0)));
    }
}
