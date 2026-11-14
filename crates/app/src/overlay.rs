//! The debug overlay: development controls drawn with `egui` over the world.
//!
//! The overlay never acts on the simulation itself. It shows what it is given
//! and returns what the developer asked for, which the window then applies.

use std::time::Duration;

use egui::{Align2, ClippedPrimitive, TexturesDelta, ViewportId};
use expanse_core::time::Tick;
use expanse_render::UiFrame;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::pacer::Speed;

/// The weight of the newest frame in the averaged frame time. Small enough
/// that the displayed rate is readable, large enough to follow a change
/// within a second.
const FRAME_TIME_SMOOTHING: f32 = 0.05;

/// The speeds the overlay offers, with their labels, in display order.
const SPEED_CHOICES: [(Speed, &str); 4] = [
    (Speed::Paused, "Pause"),
    (Speed::Normal, "1×"),
    (Speed::Fast, "2×"),
    (Speed::Faster, "3×"),
];

/// What the overlay shows about the running game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayStatus {
    /// The tick of the most recent snapshot.
    pub tick: Tick,
    /// The selected simulation speed.
    pub speed: Speed,
}

/// A change the developer asked for through the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayRequest {
    /// Run the simulation at this speed.
    SetSpeed(Speed),
    /// Run one tick while paused.
    Step,
}

/// What the overlay produced for one frame.
pub struct OverlayFrame {
    /// The changes asked for during the frame, in order.
    pub requests: Vec<OverlayRequest>,
    primitives: Vec<ClippedPrimitive>,
    textures_delta: TexturesDelta,
    pixels_per_point: f32,
}

impl OverlayFrame {
    /// What to paint over the world.
    #[must_use]
    pub fn ui(&self) -> UiFrame<'_> {
        UiFrame {
            primitives: &self.primitives,
            textures_delta: &self.textures_delta,
            pixels_per_point: self.pixels_per_point,
        }
    }
}

/// Development controls and readings, hidden until toggled.
///
/// `egui` runs every frame, even while the overlay is hidden, so that it keeps
/// track of held buttons and the cursor and is up to date when shown.
pub struct DebugOverlay {
    visible: bool,
    context: egui::Context,
    window_input: egui_winit::State,
    average_frame_seconds: Option<f32>,
}

impl DebugOverlay {
    /// Creates a hidden overlay for `window`.
    #[must_use]
    pub fn new(window: &Window) -> Self {
        let context = egui::Context::default();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "HiDPI scale factors fit in f32"
        )]
        let window_input = egui_winit::State::new(
            context.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        Self {
            visible: false,
            context,
            window_input,
            average_frame_seconds: None,
        }
    }

    /// Shows the overlay if it is hidden, and hides it otherwise.
    pub const fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Passes `event` to the overlay. Returns whether the overlay consumed it,
    /// so that it does not also act on the world. A hidden overlay consumes
    /// nothing.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.window_input.on_window_event(window, event);
        self.visible && response.consumed
    }

    /// Runs the overlay for one frame that lasted `frame_time`, showing
    /// `status`.
    pub fn run(
        &mut self,
        window: &Window,
        frame_time: Duration,
        status: OverlayStatus,
    ) -> OverlayFrame {
        self.record_frame_time(frame_time);
        let raw_input = self.window_input.take_egui_input(window);
        let visible = self.visible;
        let average_frame_seconds = self.average_frame_seconds;
        let mut requests = Vec::new();
        let output = self.context.run_pass(raw_input, |context| {
            // `egui` may run a pass again and discard the first one, so only
            // the requests of the last pass count.
            requests.clear();
            if visible {
                show(context, status, average_frame_seconds, &mut requests);
            }
        });
        self.window_input
            .handle_platform_output(window, output.platform_output);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        OverlayFrame {
            requests,
            primitives,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }
    }

    fn record_frame_time(&mut self, frame_time: Duration) {
        let seconds = frame_time.as_secs_f32();
        if seconds <= 0.0 {
            return;
        }
        self.average_frame_seconds = Some(match self.average_frame_seconds {
            Some(average) => average + (seconds - average) * FRAME_TIME_SMOOTHING,
            None => seconds,
        });
    }
}

fn show(
    context: &egui::Context,
    status: OverlayStatus,
    average_frame_seconds: Option<f32>,
    requests: &mut Vec<OverlayRequest>,
) {
    egui::Window::new("Debug")
        .anchor(Align2::LEFT_TOP, [8.0, 8.0])
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            ui.label(format!("Tick {}", status.tick.count()));
            if let Some(seconds) = average_frame_seconds {
                ui.label(format!(
                    "{:.0} fps ({:.1} ms)",
                    seconds.recip(),
                    seconds * 1000.0
                ));
            }
            ui.separator();
            ui.horizontal(|ui| {
                for (speed, label) in SPEED_CHOICES {
                    let selected = status.speed == speed;
                    if ui.selectable_label(selected, label).clicked() && !selected {
                        requests.push(OverlayRequest::SetSpeed(speed));
                    }
                }
            });
            let paused = status.speed == Speed::Paused;
            if ui.add_enabled(paused, egui::Button::new("Step")).clicked() {
                requests.push(OverlayRequest::Step);
            }
        });
}
