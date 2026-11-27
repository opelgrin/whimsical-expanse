//! `winit` window lifecycle, event loop, and real-time frame presentation.

use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use expanse_core::grid::TilePos;
use expanse_render::{
    Assets, AtlasTexture, Camera, CameraConfig, Interpolation, InvalidCameraConfig, MapPalette,
    RenderError, Renderer, ScreenPoint, TerrainDisplay, TerrainStyle, TerrainView, Viewport,
    WorldPoint,
};
use expanse_sim::{World, WorldConfig};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowId};

use crate::camera_controls::{CameraControls, CameraControlsConfig};
use crate::genesis::{Genesis, GenesisError};
use crate::input::{Button, InputAction, InputConfig, InputEvent, InputState, ScrollAmount};
use crate::inspector::inspect;
use crate::overlay::{
    AssetReading, DebugOverlay, GenerationSettings, OverlayRequest, OverlayStatus,
};
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
    /// Whether the window opens in borderless fullscreen. Leaving fullscreen
    /// gives the initial window size.
    pub fullscreen: bool,
    /// Initial simulation speed.
    pub initial_speed: Speed,
    /// Initial camera zoom and its limits. The camera opens on the world's
    /// first entity.
    pub camera: CameraConfig,
    /// Key and mouse bindings, and how input devices are read.
    pub input: InputConfig,
    /// Camera movement and zoom tuning.
    pub camera_controls: CameraControlsConfig,
}

/// Errors that can occur while creating or running the application window.
#[derive(Debug)]
pub enum AppError {
    /// The world could not be created.
    Genesis(GenesisError),
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
            Self::Genesis(err) => write!(f, "{err}"),
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
            Self::Genesis(err) => Some(err),
            Self::InvalidCamera(err) => Some(err),
            Self::EventLoop(err) => Some(err),
            Self::CreateWindow(err) => Some(err),
            Self::Render(err) => Some(err),
        }
    }
}

/// Creates a world from `genesis`, opens the game window, spawns the
/// simulation thread, and runs until the window is closed or a fatal error
/// stops it. The world is drawn with `assets`. The debug overlay can start
/// new worlds from other settings.
///
/// # Errors
///
/// Returns [`AppError`] if the world cannot be created, the camera
/// configuration is invalid, the window or event loop cannot be created, or
/// GPU initialization/presentation fails.
pub fn run(
    genesis: Genesis,
    assets: Assets,
    pacer: TickPacer,
    config: AppConfig,
) -> Result<(), AppError> {
    let world = genesis.create_world().map_err(AppError::Genesis)?;
    let camera = camera_for_world(&world, config.camera)?;
    let event_loop = EventLoop::new().map_err(AppError::EventLoop)?;

    let simulation = SimulationHandle::spawn(world, pacer.clone(), config.initial_speed);
    let mut app = App {
        palette: MapPalette::new(&genesis.biome_table),
        genesis,
        assets,
        pacer,
        display: TerrainDisplay {
            view: TerrainView::Terrain,
            obstacles: false,
        },
        generation_error: None,
        title: config.title,
        initial_logical_size: LogicalSize::new(config.window_width, config.window_height),
        initial_fullscreen: config.fullscreen,
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

/// A camera over `world`, centred on its first entity, wherever the surface
/// let it start.
fn camera_for_world(world: &World, config: CameraConfig) -> Result<Camera, AppError> {
    let snapshot = world.snapshot();
    let start = snapshot
        .entities
        .first()
        .map_or(TilePos::new(0, 0), |entity| entity.motion.from);
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile coordinates of a surface fit accurately in f32"
    )]
    let centre = WorldPoint::new(start.x as f32 + 0.5, start.y as f32 + 0.5);
    Camera::new(config, world.grid(), centre).map_err(AppError::InvalidCamera)
}

struct Graphics {
    window: Arc<Window>,
    renderer: Renderer<'static>,
    overlay: DebugOverlay,
    /// The atlas pages, uploaded with the renderer.
    atlases: Vec<AtlasTexture>,
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
    /// What the running world was created from.
    genesis: Genesis,
    /// The art the world is drawn with.
    assets: Assets,
    /// The pacer each new simulation starts with.
    pacer: TickPacer,
    palette: MapPalette,
    display: TerrainDisplay,
    /// Why the last regeneration failed, if it did.
    generation_error: Option<String>,
    title: String,
    initial_logical_size: LogicalSize<f64>,
    initial_fullscreen: bool,
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
            OverlayRequest::SetDisplay(display) => self.display = display,
            OverlayRequest::Regenerate(settings) => self.regenerate(*settings),
        }
    }

    /// Replaces the running world with one created from `settings`, at the
    /// same speed and seen through the same camera. Keeps the running world,
    /// and records why, if the new one cannot be created.
    ///
    /// Generation blocks the window for as long as it takes. It is a
    /// development action, and the simulation goes on meanwhile.
    fn regenerate(&mut self, settings: GenerationSettings) {
        let genesis = Genesis {
            world: WorldConfig {
                seed: settings.seed,
                ..self.genesis.world.clone()
            },
            surface: settings.surface,
            biome_table: self.genesis.biome_table.clone(),
        };
        match genesis.create_world() {
            Ok(world) => {
                self.camera.set_grid(world.grid());
                if let Some(simulation) = self.simulation.take() {
                    simulation.shutdown();
                }
                self.simulation = Some(SimulationHandle::spawn(
                    world,
                    self.pacer.clone(),
                    self.speed.current,
                ));
                self.genesis = genesis;
                self.generation_error = None;
            }
            Err(err) => self.generation_error = Some(err.to_string()),
        }
        // The time spent generating is not a frame to catch up on.
        self.last_frame = now();
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
                InputAction::ToggleFullscreen => {
                    if let Some(graphics) = &self.graphics {
                        toggle_fullscreen(&graphics.window);
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
        let inspected = self
            .input
            .cursor()
            .and_then(|cursor| self.camera.tile_at(cursor, viewport))
            .and_then(|tile| inspect(&snapshots.current.terrain, tile));
        let overlay_frame = graphics.overlay.run(
            &graphics.window,
            dt,
            OverlayStatus {
                tick: snapshots.current.tick,
                speed: self.speed.current,
                display: self.display,
                inspected: inspected.as_ref(),
                biome_table: &self.genesis.biome_table,
                generation_error: self.generation_error.as_deref(),
                assets: AssetReading {
                    atlases: &graphics.atlases,
                    ground_count: self.assets.container().grounds().len(),
                    sprite_count: self.assets.container().sprites().len(),
                },
            },
        );
        let interpolation = Interpolation {
            previous: &snapshots.previous,
            current: &snapshots.current,
            progress: snapshots.progress(frame_time),
        };
        let terrain_style = TerrainStyle {
            palette: &self.palette,
            display: self.display,
        };
        if let Err(err) = graphics.renderer.render(
            &self.camera,
            viewport,
            scale_factor,
            &interpolation,
            &self.assets,
            &terrain_style,
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
            .with_inner_size(self.initial_logical_size)
            .with_fullscreen(self.initial_fullscreen.then(borderless_fullscreen));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.fatal_error = Some(AppError::CreateWindow(err));
                event_loop.exit();
                return;
            }
        };
        let physical = window.inner_size();
        let mut renderer = match Renderer::new(
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
        renderer.upload_assets(&self.assets);
        let atlases = renderer.atlas_textures();
        self.last_frame = now();
        window.request_redraw();
        let overlay = DebugOverlay::new(
            &window,
            GenerationSettings {
                seed: self.genesis.world.seed,
                surface: self.genesis.surface.clone(),
            },
        );
        self.graphics = Some(Graphics {
            window,
            renderer,
            overlay,
            atlases,
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

/// Borderless fullscreen on the monitor the window is on.
///
/// Exclusive fullscreen, which changes the monitor's video mode, is not used:
/// Wayland does not support it, and the camera already scales the art to any
/// resolution.
const fn borderless_fullscreen() -> Fullscreen {
    Fullscreen::Borderless(None)
}

/// Switches `window` between borderless fullscreen and windowed.
///
/// The window, not the app, knows whether it is fullscreen, because the user
/// can also leave fullscreen through the OS, such as with the macOS title bar
/// button.
fn toggle_fullscreen(window: &Window) {
    let next = match window.fullscreen() {
        Some(_) => None,
        None => Some(borderless_fullscreen()),
    };
    window.set_fullscreen(next);
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
    use expanse_sim::WorldConfig;
    use expanse_terrain::{BiomeIndex, CHUNK_SIZE, Fields, Terrain, Tile};

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

    /// Catches a camera over another grid than the world's, such as one with
    /// swapped dimensions, which would wrap at the wrong column, and a camera
    /// that does not open on the wanderer.
    #[test]
    fn the_camera_opens_on_the_wanderer_and_wraps_with_the_world() {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        let surface = Terrain::new(2, 1, |_| Tile::new(fields, BiomeIndex(0), false))
            .expect("a terrain with tiles");
        let config = WorldConfig {
            seed: 1,
            ticks_per_step: 2,
        };
        let world = World::new(config, surface).expect("valid world");
        let start = world.snapshot().entities[0].motion.from;
        let pixels_per_tile = 10.0;
        let mut camera = camera_for_world(
            &world,
            CameraConfig {
                initial_pixels_per_tile: pixels_per_tile,
                min_pixels_per_tile: 5.0,
                max_pixels_per_tile: 20.0,
            },
        )
        .expect("valid camera");
        #[expect(clippy::cast_precision_loss, reason = "a small test world")]
        let (opening, width, height) = (
            WorldPoint::new(start.x as f32 + 0.5, start.y as f32 + 0.5),
            (2 * CHUNK_SIZE) as f32,
            CHUNK_SIZE as f32,
        );
        assert_eq!(camera.centre(), opening);

        // Once round the world, east, comes back to the same column.
        camera.pan_by_screen_offset(width * pixels_per_tile, 0.0);
        assert!((camera.centre().x - opening.x).abs() < 1e-3);
        camera.pan_by_screen_offset(0.0, 10_000.0);
        assert_eq!(camera.centre().y, height);
    }
}
