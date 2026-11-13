//! The mapping from physical buttons to input actions.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// What the player means by pressing a button, independent of which button it
/// is.
///
/// Input actions are presentation only. A future action that changes the world
/// becomes a player command, recorded with the tick it applies at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// Pan the camera north while held.
    PanUp,
    /// Pan the camera south while held.
    PanDown,
    /// Pan the camera west while held.
    PanLeft,
    /// Pan the camera east while held.
    PanRight,
    /// Grab the world and drag it with the cursor while held.
    DragPan,
    /// Pause the simulation, or resume it at its previous speed.
    TogglePause,
}

/// A physical key or mouse button.
///
/// Keys are physical positions, so a binding to `KeyW` stays at the same place
/// on AZERTY and other layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Which buttons trigger which input actions.
///
/// An action may have any number of buttons, and a button may trigger several
/// actions. Callers state every binding explicitly so tests never rely on game
/// defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings {
    pairs: Vec<(Button, InputAction)>,
}

impl Bindings {
    /// Creates bindings where no button does anything.
    #[must_use]
    pub const fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Returns these bindings with `button` also triggering `action`. Binding
    /// the same pair twice has no further effect.
    #[must_use]
    pub fn with(mut self, button: Button, action: InputAction) -> Self {
        if !self.pairs.contains(&(button, action)) {
            self.pairs.push((button, action));
        }
        self
    }

    /// Whether `button` triggers `action`.
    pub(super) fn is_bound(&self, button: Button, action: InputAction) -> bool {
        self.pairs.contains(&(button, action))
    }

    /// The actions `button` triggers, in the order they were bound.
    pub(super) fn actions_for(&self, button: Button) -> impl Iterator<Item = InputAction> + '_ {
        self.pairs
            .iter()
            .filter(move |(bound, _)| *bound == button)
            .map(|&(_, action)| action)
    }
}
