//! Entry point for the playable game window.
//!
//! Default configuration, tuning and content live here so that library crates
//! (`sim`, `worldgen`, `render`, `app`) stay free of game defaults.

mod content;
mod generation;

use std::num::NonZero;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use expanse_app::assets::load_assets;
use expanse_app::camera_controls::CameraControlsConfig;
use expanse_app::genesis::Genesis;
use expanse_app::input::{Bindings, Button, InputAction, InputConfig, KeyCode, MouseButton};
use expanse_app::pacer::{Speed, TickPacer};
use expanse_app::window::{self, AppConfig};
use expanse_render::{AssetContent, CameraConfig};
use expanse_sim::WorldConfig;

// Simulation defaults. The surface's size and generation parameters are in
// `generation`.
const DEFAULT_SEED: u64 = 1;
const TICKS_PER_STEP: u64 = 10;
const BASE_TICKS_PER_SECOND: u32 = 20;
const MAX_SIMULATION_LAG: Duration = Duration::from_millis(250);

/// Where `just cook` writes the cooked assets, relative to the repository
/// root, from which `just game` runs. Where a shipped game keeps them is
/// decided with packaging.
const DEFAULT_ASSETS: &str = "cooked/assets.bin";

// Window and camera defaults. The pixels-per-tile values are multiples of the
// 16 px tile. The smallest zoom is for development: the game's will be larger.
const WINDOW_TITLE: &str = "Whimsical Expanse";
const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 720.0;
const INITIAL_PIXELS_PER_TILE: f32 = 48.0; // 3x of 16 px
const MIN_PIXELS_PER_TILE: f32 = 8.0; // 0.5x of 16 px
const MAX_PIXELS_PER_TILE: f32 = 192.0; // 12x of 16 px
const PAN_SPEED_PIXELS_PER_SECOND: f32 = 600.0;
const ZOOM_STEP_FACTOR: f32 = 1.15;
const PIXELS_PER_SCROLL_LINE: f32 = 40.0;

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            report_error(&message);
            report_usage();
            return ExitCode::from(2);
        }
    };

    configure_thread_pool();
    let genesis = Genesis {
        world: WorldConfig {
            seed: args.seed,
            ticks_per_step: TICKS_PER_STEP,
        },
        surface: generation::surface_params(),
        biome_table: content::biome_table(),
    };
    let asset_content = AssetContent {
        drawing_order: content::drawing_order(),
        wanderer_sprite: content::wanderer_sprite(),
    };
    let assets = match load_assets(&args.assets, &genesis.biome_table, &asset_content) {
        Ok(assets) => assets,
        Err(err) => {
            report_error(&err.to_string());
            return ExitCode::FAILURE;
        }
    };
    let pacer = TickPacer::new(BASE_TICKS_PER_SECOND, MAX_SIMULATION_LAG);

    let app_config = AppConfig {
        title: WINDOW_TITLE.to_owned(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        initial_speed: Speed::Normal,
        camera: CameraConfig {
            initial_pixels_per_tile: INITIAL_PIXELS_PER_TILE,
            min_pixels_per_tile: MIN_PIXELS_PER_TILE,
            max_pixels_per_tile: MAX_PIXELS_PER_TILE,
        },
        input: InputConfig {
            bindings: default_bindings(),
            pixels_per_scroll_line: PIXELS_PER_SCROLL_LINE,
        },
        camera_controls: CameraControlsConfig {
            pan_speed_pixels_per_second: PAN_SPEED_PIXELS_PER_SECOND,
            zoom_step_factor: ZOOM_STEP_FACTOR,
        },
    };

    if let Err(err) = window::run(genesis, assets, pacer, app_config) {
        report_error(&err.to_string());
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Sizes the pool that runs parallel work, leaving one core to the window, so
/// that drawing stays smooth while the simulation runs a parallel phase.
fn configure_thread_pool() {
    let cores = std::thread::available_parallelism().map_or(1, NonZero::get);
    #[expect(
        clippy::disallowed_methods,
        reason = "the game binary is where the pool is sized"
    )]
    rayon::ThreadPoolBuilder::new()
        .num_threads(cores.saturating_sub(1).max(1))
        .build_global()
        .expect("the thread pool is configured once, before any parallel work");
}

/// Physical key positions, so `WASD` is `ZQSD` on AZERTY, plus the arrow keys.
fn default_bindings() -> Bindings {
    Bindings::new()
        .with(Button::Key(KeyCode::KeyW), InputAction::PanUp)
        .with(Button::Key(KeyCode::KeyS), InputAction::PanDown)
        .with(Button::Key(KeyCode::KeyA), InputAction::PanLeft)
        .with(Button::Key(KeyCode::KeyD), InputAction::PanRight)
        .with(Button::Key(KeyCode::ArrowUp), InputAction::PanUp)
        .with(Button::Key(KeyCode::ArrowDown), InputAction::PanDown)
        .with(Button::Key(KeyCode::ArrowLeft), InputAction::PanLeft)
        .with(Button::Key(KeyCode::ArrowRight), InputAction::PanRight)
        .with(Button::Mouse(MouseButton::Middle), InputAction::DragPan)
        .with(Button::Key(KeyCode::Escape), InputAction::TogglePause)
        .with(Button::Key(KeyCode::F3), InputAction::ToggleOverlay)
}

/// What the command line chose.
struct Args {
    seed: u64,
    /// The cooked assets to load.
    assets: PathBuf,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        seed: DEFAULT_SEED,
        assets: PathBuf::from(DEFAULT_ASSETS),
    };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("`{flag}` needs a value"));
        match flag.as_str() {
            "--seed" => {
                let value = value()?;
                parsed.seed = value
                    .parse()
                    .map_err(|_| format!("`--seed` expects a whole number, got `{value}`"))?;
            }
            "--assets" => parsed.assets = PathBuf::from(value()?),
            _ => return Err(format!("unknown argument `{flag}`")),
        }
    }
    Ok(parsed)
}

#[expect(clippy::print_stderr, reason = "CLI and startup errors go to stderr")]
fn report_error(message: &str) {
    eprintln!("error: {message}");
}

#[expect(clippy::print_stderr, reason = "usage help goes to stderr")]
fn report_usage() {
    eprintln!("usage: expanse-game [--seed N] [--assets PATH]");
}
