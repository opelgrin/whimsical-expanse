//! Entry point for the playable game window.
//!
//! Default configuration and tuning live here so that library crates (`sim`,
//! `render`, `app`) stay free of game defaults.

use std::process::ExitCode;
use std::time::Duration;

use expanse_app::input::CameraControlsConfig;
use expanse_app::pacer::{Speed, TickPacer};
use expanse_app::window::{self, AppConfig};
use expanse_render::{CameraConfig, WorldPoint};
use expanse_sim::{World, WorldConfig};

// Simulation defaults for M0.
const DEFAULT_SEED: u64 = 1;
const GRID_WIDTH: i32 = 32;
const GRID_HEIGHT: i32 = 20;
const TICKS_PER_STEP: u64 = 10;
const BASE_TICKS_PER_SECOND: u32 = 20;
const MAX_SIMULATION_LAG: Duration = Duration::from_millis(250);

// Window and camera defaults. The pixels-per-tile values are multiples of 16 px,
// the assumed base tile size; no document fixes that size yet.
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
    let seed = match parse_seed(std::env::args().skip(1)) {
        Ok(seed) => seed,
        Err(message) => {
            report_error(&message);
            report_usage();
            return ExitCode::from(2);
        }
    };

    let world = World::new(WorldConfig {
        seed,
        width: GRID_WIDTH,
        height: GRID_HEIGHT,
        ticks_per_step: TICKS_PER_STEP,
    })
    .expect("the default game world parameters are valid");

    let pacer = TickPacer::new(BASE_TICKS_PER_SECOND, MAX_SIMULATION_LAG);

    #[expect(
        clippy::cast_precision_loss,
        reason = "default grid dimensions fit accurately in f32"
    )]
    let initial_centre = WorldPoint::new(GRID_WIDTH as f32 * 0.5, GRID_HEIGHT as f32 * 0.5);

    let app_config = AppConfig {
        title: WINDOW_TITLE.to_owned(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        initial_speed: Speed::Normal,
        camera: CameraConfig {
            initial_centre,
            initial_pixels_per_tile: INITIAL_PIXELS_PER_TILE,
            min_pixels_per_tile: MIN_PIXELS_PER_TILE,
            max_pixels_per_tile: MAX_PIXELS_PER_TILE,
        },
        controls: CameraControlsConfig {
            pan_speed_pixels_per_second: PAN_SPEED_PIXELS_PER_SECOND,
            zoom_step_factor: ZOOM_STEP_FACTOR,
            pixels_per_scroll_line: PIXELS_PER_SCROLL_LINE,
        },
    };

    if let Err(err) = window::run(world, pacer, app_config) {
        report_error(&err.to_string());
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn parse_seed(mut args: impl Iterator<Item = String>) -> Result<u64, String> {
    let mut seed = DEFAULT_SEED;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--seed` needs a value".to_owned())?;
                seed = value
                    .parse()
                    .map_err(|_| format!("`--seed` expects a whole number, got `{value}`"))?;
            }
            _ => return Err(format!("unknown argument `{flag}`")),
        }
    }
    Ok(seed)
}

#[expect(clippy::print_stderr, reason = "CLI and startup errors go to stderr")]
fn report_error(message: &str) {
    eprintln!("error: {message}");
}

#[expect(clippy::print_stderr, reason = "usage help goes to stderr")]
fn report_usage() {
    eprintln!("usage: expanse-game [--seed N]");
}
