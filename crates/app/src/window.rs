//! `winit` window lifecycle, event loop, and real-time frame presentation.

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use expanse_render::{
    Camera, CameraConfig, InvalidCameraConfig, RenderError, Renderer, ScreenPoint, Viewport,
};
use expanse_sim::World;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::input::{CameraController, CameraControlsConfig, PanBinding, PanKey, ScrollAmount};
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
    /// Keyboard and mouse camera controls tuning.
    pub controls: CameraControlsConfig,
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
/// until the window is closed. The camera uses the dimensions of `world` for
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
        controller: CameraController::new(config.controls),
        speed: config.initial_speed,
        unpaused_speed: match config.initial_speed {
            Speed::Paused => Speed::Normal,
            active => active,
        },
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
}

struct App {
    title: String,
    initial_logical_size: LogicalSize<f64>,
    camera: Camera,
    controller: CameraController,
    speed: Speed,
    unpaused_speed: Speed,
    occluded: bool,
    simulation: Option<SimulationHandle>,
    graphics: Option<Graphics>,
    last_frame: Instant,
    fatal_error: Option<AppError>,
}

impl App {
    fn toggle_pause(&mut self) {
        self.speed = if self.speed == Speed::Paused {
            self.unpaused_speed
        } else {
            self.unpaused_speed = self.speed;
            Speed::Paused
        };
        if let Some(simulation) = &self.simulation {
            simulation.set_speed(self.speed);
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

    fn handle_key(&mut self, key: PhysicalKey, state: ElementState, repeat: bool) {
        let pressed = state == ElementState::Pressed;
        let PhysicalKey::Code(code) = key else {
            return;
        };
        match code {
            KeyCode::KeyW => self
                .controller
                .set_pan_key(PanBinding::Primary(PanKey::Up), pressed),
            KeyCode::KeyS => self
                .controller
                .set_pan_key(PanBinding::Primary(PanKey::Down), pressed),
            KeyCode::KeyA => self
                .controller
                .set_pan_key(PanBinding::Primary(PanKey::Left), pressed),
            KeyCode::KeyD => self
                .controller
                .set_pan_key(PanBinding::Primary(PanKey::Right), pressed),
            KeyCode::ArrowUp => self
                .controller
                .set_pan_key(PanBinding::Secondary(PanKey::Up), pressed),
            KeyCode::ArrowDown => self
                .controller
                .set_pan_key(PanBinding::Secondary(PanKey::Down), pressed),
            KeyCode::ArrowLeft => self
                .controller
                .set_pan_key(PanBinding::Secondary(PanKey::Left), pressed),
            KeyCode::ArrowRight => self
                .controller
                .set_pan_key(PanBinding::Secondary(PanKey::Right), pressed),
            KeyCode::Escape if pressed && !repeat => self.toggle_pause(),
            _ => {}
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let current = now();
        let dt = current.saturating_duration_since(self.last_frame);
        self.last_frame = current;

        self.controller.advance(dt, &mut self.camera);

        let (Some(graphics), Some(simulation)) = (&mut self.graphics, &self.simulation) else {
            return;
        };
        let (viewport, scale_factor) = Self::current_viewport(&graphics.window);
        if viewport.width <= 0.0 || viewport.height <= 0.0 || self.occluded {
            return;
        }
        let snapshot = simulation.latest_snapshot();
        if let Err(err) = graphics
            .renderer
            .render(&self.camera, viewport, scale_factor, &snapshot)
        {
            self.fatal_error = Some(AppError::Render(err));
            event_loop.exit();
            return;
        }
        // Chain the next vsync-paced frame while the window is visible.
        graphics.window.request_redraw();
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
        self.graphics = Some(Graphics { window, renderer });
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
            WindowEvent::Focused(false) => {
                self.controller.clear_inputs();
            }
            WindowEvent::CursorLeft { .. } => {
                self.controller.cursor_left();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position.to_logical::<f64>(window.scale_factor());
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "cursor logical coordinates fit in f32"
                )]
                let point = ScreenPoint::new(logical.x as f32, logical.y as f32);
                self.controller.update_cursor(point, &mut self.camera);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => {
                self.controller.set_dragging(state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (viewport, _) = Self::current_viewport(&window);
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => ScrollAmount::Lines(y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        let logical_y = pos.to_logical::<f64>(window.scale_factor()).y;
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "scroll delta in logical pixels fits in f32"
                        )]
                        ScrollAmount::Pixels(logical_y as f32)
                    }
                };
                self.controller.zoom(scroll, viewport, &mut self.camera);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_key(event.physical_key, event.state, event.repeat);
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
            }
            _ => {}
        }
    }
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

    use super::*;

    /// Catches camera bounds that differ from the simulation grid, including
    /// swapped dimensions. The camera also determines the grid drawn on screen.
    #[test]
    fn camera_uses_the_world_grid_for_panning_and_visible_tiles() {
        let world = World::new(WorldConfig {
            seed: 1,
            width: 7,
            height: 3,
            ticks_per_step: 2,
        })
        .expect("valid world");
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

        camera.pan_by_screen_offset(1000.0, 1000.0);
        assert_eq!(camera.centre(), WorldPoint::new(7.0, 3.0));
        assert_eq!(
            camera.visible_tiles(Viewport::new(1000.0, 1000.0)),
            Some(VisibleTileRange {
                min_x: 0,
                max_x: 6,
                min_y: 0,
                max_y: 2,
            })
        );
    }
}
