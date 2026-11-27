//! The mapping from physical buttons to input actions.

use winit::event::MouseButton;
use winit::keyboard::{KeyCode, ModifiersState};

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
    /// Show or hide the debug overlay.
    ToggleOverlay,
    /// Switch the window between borderless fullscreen and windowed.
    ToggleFullscreen,
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

/// A button together with the modifier keys that must be held when it goes
/// down, such as Ctrl+Cmd+F.
///
/// The modifiers must match exactly, so a shortcut with modifiers never also
/// triggers what the bare button does, and a bare binding such as `W` does not
/// fire while Cmd is held for a system shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chord {
    button: Button,
    modifiers: ModifiersState,
}

/// Which buttons trigger which input actions.
///
/// An action may have any number of buttons, and a button may trigger several
/// actions. Callers state every binding explicitly so tests never rely on game
/// defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings {
    pairs: Vec<(Chord, InputAction)>,
}

impl Bindings {
    /// Creates bindings where no button does anything.
    #[must_use]
    pub const fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Returns these bindings with `button`, pressed with no modifier key
    /// held, also triggering `action`. Binding the same pair twice has no
    /// further effect.
    #[must_use]
    pub fn with(self, button: Button, action: InputAction) -> Self {
        self.with_modifiers(button, ModifiersState::empty(), action)
    }

    /// Returns these bindings with `button`, pressed while exactly
    /// `modifiers` are held, also triggering `action`. On macOS,
    /// [`ModifiersState::SUPER`] is Cmd; elsewhere it is the Windows or Super
    /// key. Binding the same chord and action twice has no further effect.
    #[must_use]
    pub fn with_modifiers(
        mut self,
        button: Button,
        modifiers: ModifiersState,
        action: InputAction,
    ) -> Self {
        let pair = (Chord { button, modifiers }, action);
        if !self.pairs.contains(&pair) {
            self.pairs.push(pair);
        }
        self
    }

    /// Whether `button`, pressed while `modifiers` were held, triggers
    /// `action`.
    pub(super) fn is_bound(
        &self,
        button: Button,
        modifiers: ModifiersState,
        action: InputAction,
    ) -> bool {
        self.pairs.contains(&(Chord { button, modifiers }, action))
    }

    /// The actions `button`, pressed while `modifiers` are held, triggers, in
    /// the order they were bound.
    pub(super) fn actions_for(
        &self,
        button: Button,
        modifiers: ModifiersState,
    ) -> impl Iterator<Item = InputAction> + '_ {
        let chord = Chord { button, modifiers };
        self.pairs
            .iter()
            .filter(move |(bound, _)| *bound == chord)
            .map(|&(_, action)| action)
    }
}
