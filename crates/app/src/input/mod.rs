//! Player input: what each key and mouse button means, and what is held or
//! was pressed this frame.
//!
//! The window translates its events into calls on [`InputState`], which uses
//! [`Bindings`] to turn buttons into [`InputAction`]s. Consumers, such as the
//! camera controls, read the state once per frame and never see window events
//! or physical keys, so the controls are rebound in one place and every
//! consumer is tested without a window.

mod bindings;
mod state;

pub use bindings::{Bindings, Button, InputAction};
pub use state::{CursorMotion, InputConfig, InputState, ScrollAmount};
/// Mouse buttons, re-exported for the same reason as [`KeyCode`].
pub use winit::event::MouseButton;
/// Physical key codes, re-exported so callers can write bindings without
/// depending on `winit`.
pub use winit::keyboard::KeyCode;
