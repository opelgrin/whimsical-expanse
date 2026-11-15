//! `winit` window lifecycle, event loop, and real-time frame presentation.

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use expanse_render::{
    Camera, CameraConfig, Interpolation, InvalidCameraConfig, RenderError, Renderer, ScreenPoint,
    Viewport,
};
use expanse_sim::World;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::camera_controls::{CameraControls, CameraControlsConfig};
use crate::input::{Button, InputAction, InputConfig, InputEvent, InputState, ScrollAmount};
use crate::overlay::{DebugOverlay, OverlayRequest, OverlayStatus};
use crate::pacer::{Speed, TickPacer};
use crate::simulation_thread::SimulationHandle;

/// Configuration for opening the game window and running the simulation.
pub struct AppConfig {
    /// Window title shown in the OS title bar.
    pub title: String,
    /// Initial inner window width in logical pixels.
    pub window_width: f64,
    /// Initial inner window height in logical pixels.
    pub window_height: f64,
    /// Initial simulation speed.
    pub initial_speed: Speed,
    /// Initial camera state and limits.
    pub camera: CameraConfig,
    /// Key and mouse bindings, and how input devices are read.
    pub input: InputConfig,
    /// Camera movement and zoom tuning.
    pub camera_controls: CameraControlsConfig,
}

/// Errors that can occur while creating or running the application window.
#[derive(Debug)]
pub enum AppError {
    /// Invalid camera configuration.
    InvalidCamera(InvalidCameraConfig),
    /// Failed to create or run the platform event loop.
    EventLoop(EventLoopError),
    /// Failed to create the OS window.
    CreateWindow(OsError),
    /// Failed to initialize or present with `wgpu`.
    Render(RenderError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCamera(err) => write!(f, "invalid camera configuration: {err}"),
            Self::EventLoop(err) => write!(f, "event loop error: {err}"),
            Self::CreateWindow(err) => write!(f, "failed to create window: {err}"),
            Self::Render(err) => write!(f, "render error: {err}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCamera(err) => Some(err),
            Self::EventLoop(err) => Some(err),
            Self::CreateWindow(err) => Some(err),
            Self::Render(err) => Some(err),
        }
    }
}

/// Opens the game window, spawns the simulation thread for `world`, and runs
/// until the window is closed or a fatal error stops it. The camera uses the dimensions of `world` for
/// its bounds and the displayed grid.
///
/// # Errors
///
/// Returns [`AppError`] if the camera configuration is invalid, the window or
/// event loop cannot be created, or GPU initialization/presentation fails.
pub fn run(world: World, pacer: TickPacer, config: AppConfig) -> Result<(), AppError> {
    let camera = camera_for_world(&world, config.camera)?;
    let event_loop = EventLoop::new().map_err(AppError::EventLoop)?;

    let simulation = SimulationHandle::spawn(world, pacer, config.initial_speed);
    let mut app = App {
        title: config.title,
        initial_logical_size: LogicalSize::new(config.window_width, config.window_height),
        camera,
        input: InputState::new(config.input),
        camera_controls: CameraControls::new(config.camera_controls),
        speed: SpeedSelection::new(config.initial_speed),
        occluded: false,
        simulation: Some(simulation),
        graphics: None,
        last_frame: now(),
        fatal_error: None,
    };

    event_loop.run_app(&mut app).map_err(AppError::EventLoop)?;

    if let Some(simulation) = app.simulation.take() {
        simulation.shutdown();
    }
    if let Some(err) = app.fatal_error {
        return Err(err);
    }
    Ok(())
}

fn camera_for_world(world: &World, config: CameraConfig) -> Result<Camera, AppError> {
    let (width, height) = world.dimensions();
    Camera::new(config, width, height).map_err(AppError::InvalidCamera)
}

struct Graphics {
    window: Arc<Window>,
    renderer: Renderer<'static>,
    overlay: DebugOverlay,
}

/// The selected simulation speed, and the speed that ending a pause resumes.
///
/// Pausing is open to the player and the debug overlay alike, so whichever
/// paused, the pause key resumes the speed that was running before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpeedSelection {
    current: Speed,
    resume: Speed,
}

impl SpeedSelection {
    const fn new(initial: Speed) -> Self {
        Self {
            current: initial,
            resume: match initial {
                Speed::Paused => Speed::Normal,
                active => active,
            },
        }
    }

    fn select(&mut self, speed: Speed) {
        if speed == Speed::Paused && self.current != Speed::Paused {
            self.resume = self.current;
        }
        self.current = speed;
    }

    fn toggle_pause(&mut self) {
        let next = if self.current == Speed::Paused {
            self.resume
        } else {
            Speed::Paused
        };
        self.select(next);
    }
}

struct App {
    title: String,
    initial_logical_size: LogicalSize<f64>,
    camera: Camera,
    input: InputState,
    camera_controls: CameraControls,
    speed: SpeedSelection,
    occluded: bool,
    simulation: Option<SimulationHandle>,
    graphics: Option<Graphics>,
    last_frame: Instant,
    fatal_error: Option<AppError>,
}

impl App {
    fn change_speed(&mut self, change: impl FnOnce(&mut SpeedSelection)) {
        change(&mut self.speed);
        if let Some(simulation) = &self.simulation {
            simulation.set_speed(self.speed.current);
        }
    }

    fn apply_overlay_request(&mut self, request: OverlayRequest) {
        match request {
            OverlayRequest::SetSpeed(speed) => {
                self.change_speed(|selection| selection.select(speed));
            }
            OverlayRequest::Step => {
                if let Some(simulation) = &self.simulation {
                    simulation.step();
                }
            }
        }
    }

    fn current_viewport(window: &Window) -> (Viewport, f32) {
        let scale_factor = window.scale_factor();
        let logical = window.inner_size().to_logical::<f64>(scale_factor);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "window logical dimensions and HiDPI scale factors fit in f32"
        )]
        (
            Viewport::new(logical.width as f32, logical.height as f32),
            scale_factor as f32,
        )
    }

    /// Applies one frame of input: the continuous controls first, then the
    /// actions pressed since the last frame, in order.
    fn apply_input(&mut self, dt: Duration, viewport: Viewport) {
        self.camera_controls
            .update(&self.input, dt, viewport, &mut self.camera);
        let pressed = self.input.pressed_actions().to_vec();
        for action in pressed {
            match action {
                InputAction::TogglePause => self.change_speed(SpeedSelection::toggle_pause),
                InputAction::ToggleOverlay => {
                    if let Some(graphics) = &mut self.graphics {
                        graphics.overlay.toggle();
                    }
                }
                InputAction::PanUp
                | InputAction::PanDown
                | InputAction::PanLeft
                | InputAction::PanRight
                | InputAction::DragPan => {}
            }
        }
        self.input.end_frame();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let frame_time = now();
        let dt = frame_time.saturating_duration_since(self.last_frame);
        self.last_frame = frame_time;

        let Some(graphics) = &self.graphics else {
            return;
        };
        let (viewport, scale_factor) = Self::current_viewport(&graphics.window);
        self.apply_input(dt, viewport);

        let (Some(graphics), Some(simulation)) = (&mut self.graphics, &self.simulation) else {
            return;
        };
        if viewport.width <= 0.0 || viewport.height <= 0.0 || self.occluded {
            return;
        }
        let snapshots = simulation.latest_snapshots();
        let overlay_frame = graphics.overlay.run(
            &graphics.window,
            dt,
            OverlayStatus {
                tick: snapshots.current.tick,
                speed: self.speed.current,
            },
        );
        let interpolation = Interpolation {
            previous: &snapshots.previous,
            current: &snapshots.current,
            progress: snapshots.progress(frame_time),
        };
        if let Err(err) = graphics.renderer.render(
            &self.camera,
            viewport,
            scale_factor,
            &interpolation,
            &overlay_frame.ui(),
        ) {
            self.fatal_error = Some(AppError::Render(err));
            event_loop.exit();
            return;
        }
        // Chain the next vsync-paced frame while the window is visible.
        graphics.window.request_redraw();
        for request in overlay_frame.requests {
            self.apply_overlay_request(request);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(self.initial_logical_size);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.fatal_error = Some(AppError::CreateWindow(err));
                event_loop.exit();
                return;
            }
        };
        let physical = window.inner_size();
        let renderer = match Renderer::new(
            event_loop.owned_display_handle(),
            Arc::clone(&window),
            physical.width,
            physical.height,
        ) {
            Ok(renderer) => renderer,
            Err(err) => {
                self.fatal_error = Some(AppError::Render(err));
                event_loop.exit();
                return;
            }
        };
        self.last_frame = now();
        window.request_redraw();
        let overlay = DebugOverlay::new(&window);
        self.graphics = Some(Graphics {
            window,
            renderer,
            overlay,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.graphics.as_ref().map(|g| Arc::clone(&g.window)) else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        if let Some(input_event) = input_event(&window, &event) {
            let consumed_by_ui = self
                .graphics
                .as_mut()
                .is_some_and(|graphics| graphics.overlay.on_window_event(&window, &event));
            self.input.handle(input_event, consumed_by_ui);
        } else if let Some(graphics) = &mut self.graphics {
            // The overlay also follows window changes, such as resizing and
            // focus gained, that the input state ignores.
            let _consumed = graphics.overlay.on_window_event(&window, &event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(physical) => {
                if let Some(graphics) = &mut self.graphics {
                    graphics.renderer.resize(physical.width, physical.height);
                    if physical.width > 0 && physical.height > 0 && !self.occluded {
                        self.last_frame = now();
                        graphics.window.request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let physical = window.inner_size();
                if let Some(graphics) = &mut self.graphics {
                    graphics.renderer.resize(physical.width, physical.height);
                    if physical.width > 0 && physical.height > 0 && !self.occluded {
                        graphics.window.request_redraw();
                    }
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if !occluded {
                    self.last_frame = now();
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
            }
            _ => {}
        }
    }
}

/// Translates a window event into what the input state needs, or `None` if
/// it does not concern input.
fn input_event(window: &Window, event: &WindowEvent) -> Option<InputEvent> {
    let input_event = match event {
        WindowEvent::Focused(false) => InputEvent::FocusLost,
        WindowEvent::CursorLeft { .. } => InputEvent::CursorLeft,
        WindowEvent::CursorMoved { position, .. } => {
            let logical = position.to_logical::<f64>(window.scale_factor());
            #[expect(
                clippy::cast_possible_truncation,
                reason = "cursor logical coordinates fit in f32"
            )]
            InputEvent::CursorMoved(ScreenPoint::new(logical.x as f32, logical.y as f32))
        }
        WindowEvent::MouseInput { state, button, .. } => InputEvent::Button {
            button: Button::Mouse(*button),
            pressed: *state == ElementState::Pressed,
        },
        WindowEvent::MouseWheel { delta, .. } => InputEvent::Scrolled(match delta {
            MouseScrollDelta::LineDelta(_, y) => ScrollAmount::Lines(*y),
            MouseScrollDelta::PixelDelta(pos) => {
                let logical_y = pos.to_logical::<f64>(window.scale_factor()).y;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "scroll delta in logical pixels fits in f32"
                )]
                ScrollAmount::Pixels(logical_y as f32)
            }
        }),
        WindowEvent::KeyboardInput { event, .. } => {
            let PhysicalKey::Code(code) = event.physical_key else {
                return None;
            };
            InputEvent::Button {
                button: Button::Key(code),
                pressed: event.state == ElementState::Pressed,
            }
        }
        _ => return None,
    };
    Some(input_event)
}

#[expect(
    clippy::disallowed_methods,
    reason = "frame timing in presentation is allowed to read the wall clock"
)]
fn now() -> Instant {
    Instant::now()
}

#[cfg(test)]
mod tests {
    use expanse_render::WorldPoint;
    use expanse_render::camera::VisibleTileRange;
    use expanse_sim::WorldConfig;
    use expanse_terrain::CHUNK_SIZE;

    use super::*;

    /// Catches the pause key resuming a stale speed after the overlay changed
    /// it, or resuming into a pause, which would leave the game stuck paused.
    #[test]
    fn resuming_restores_the_speed_last_running_whoever_paused() {
        let mut speed = SpeedSelection::new(Speed::Paused);
        speed.toggle_pause();
        assert_eq!(speed.current, Speed::Normal);

        speed.select(Speed::Fast);
        speed.select(Speed::Paused);
        speed.toggle_pause();
        assert_eq!(speed.current, Speed::Fast);

        speed.toggle_pause();
        speed.select(Speed::Faster);
        speed.select(Speed::Paused);
        speed.select(Speed::Paused);
        speed.toggle_pause();
        assert_eq!(speed.current, Speed::Faster);
    }

    /// Catches camera bounds that differ from the simulation grid, including
    /// swapped dimensions. The camera also determines the grid drawn on screen.
    #[test]
    fn camera_uses_the_world_grid_for_panning_and_visible_tiles() {
        let world = World::new(WorldConfig {
            seed: 1,
            world_width_chunks: 2,
            world_height_chunks: 1,
            ticks_per_step: 2,
        })
        .expect("valid world");
        let (width, height) = (2 * CHUNK_SIZE, CHUNK_SIZE);
        let mut camera = camera_for_world(
            &world,
            CameraConfig {
                initial_centre: WorldPoint::new(1.0, 1.0),
                initial_pixels_per_tile: 10.0,
                min_pixels_per_tile: 5.0,
                max_pixels_per_tile: 20.0,
            },
        )
        .expect("valid camera");

        camera.pan_by_screen_offset(10_000.0, 10_000.0);
        #[expect(clippy::cast_precision_loss, reason = "a small test world")]
        let far_corner = WorldPoint::new(width as f32, height as f32);
        assert_eq!(camera.centre(), far_corner);
        assert_eq!(
            camera.visible_tiles(Viewport::new(100_000.0, 100_000.0)),
            Some(VisibleTileRange {
                min_x: 0,
                max_x: width - 1,
                min_y: 0,
                max_y: height - 1,
            })
        );
    }
}
