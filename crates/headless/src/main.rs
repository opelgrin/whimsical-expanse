//! Runs the simulation without a window, as fast as possible, and prints the
//! final tick and state hash.
//!
//! Two runs with the same arguments must print the same line. CI checks this on
//! every platform.
//!
//! Usage: `expanse-headless [--seed N] [--ticks N]`

use std::process::ExitCode;

use expanse_core::grid::TileOffset;
use expanse_sim::{World, WorldConfig};
use expanse_terrain::{
    BiomeDefinition, BiomeObstacleDefinition, BiomeTable, FieldRanges, ObstacleKindDefinition,
};
use expanse_worldgen::{
    FieldParams, NoiseParams, SurfaceParams, TemperatureParams, generate_surface,
};

/// Movement parameters for headless runs. Like the surface and the biome
/// table below, they only need to exercise the simulation; they are not game
/// tuning, and the surface is small so that generation stays fast.
const TICKS_PER_STEP: u64 = 6;

struct Options {
    seed: u64,
    ticks: u64,
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            report_usage_error(&message);
            return ExitCode::from(2);
        }
    };
    let surface = generate_surface(options.seed, &surface_params(), &biome_table())
        .expect("the headless surface parameters are valid");
    let config = WorldConfig {
        seed: options.seed,
        ticks_per_step: TICKS_PER_STEP,
    };
    let mut world = World::new(config, surface).expect("the headless surface has open land");
    for _ in 0..options.ticks {
        world.advance();
    }
    report_result(world.tick().count(), world.state_hash());
    ExitCode::SUCCESS
}

const fn surface_params() -> SurfaceParams {
    let noise = NoiseParams {
        scale_tiles: 24.0,
        octaves: 4,
        persistence: 0.5,
    };
    let field = FieldParams {
        noise,
        smoothing_radius: 1,
    };
    SurfaceParams {
        width_chunks: 2,
        height_chunks: 2,
        north_latitude_degrees: 50.0,
        south_latitude_degrees: 10.0,
        water_share: 0.3,
        elevation: field,
        temperature: TemperatureParams {
            equator_mean_celsius: 27.0,
            pole_mean_celsius: -18.0,
            equator_swing_celsius: 3.0,
            pole_swing_celsius: 40.0,
            cooling_at_highest_celsius: 20.0,
            noise,
            noise_strength_celsius: 3.0,
            smoothing_radius: 2,
        },
        moisture: field,
        vegetation: field,
        min_region_tiles: 8,
    }
}

/// Water, and land in a wooded and an open variant, with obstacles of one
/// tile and of several.
fn biome_table() -> BiomeTable {
    let biome = |name: &str, ranges, impassable, obstacles: &[(&str, f64)]| BiomeDefinition {
        name: name.to_owned(),
        ranges,
        map_colour: [0, 0, 0],
        impassable,
        obstacles: obstacles
            .iter()
            .map(|&(kind, density)| BiomeObstacleDefinition {
                kind: kind.to_owned(),
                density,
                variation: None,
            })
            .collect(),
    };
    let kind = |name: &str, footprint: Vec<TileOffset>| ObstacleKindDefinition {
        name: name.to_owned(),
        footprint,
    };
    BiomeTable::new(
        vec![
            biome(
                "water",
                FieldRanges {
                    elevation: Some(-1.0..0.0),
                    ..FieldRanges::default()
                },
                true,
                &[],
            ),
            biome(
                "wooded",
                FieldRanges {
                    vegetation: Some(0.5..1.0),
                    ..FieldRanges::default()
                },
                false,
                &[("tree", 0.2), ("log", 0.02)],
            ),
            biome(
                "open",
                FieldRanges::default(),
                false,
                &[("rock", 0.02), ("log", 0.01)],
            ),
        ],
        vec![
            kind("tree", Vec::new()),
            kind("rock", Vec::new()),
            kind("log", vec![TileOffset::new(1, 0), TileOffset::new(2, 0)]),
        ],
    )
    .expect("the headless biome table is valid")
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        seed: 0,
        ticks: 10_000,
    };
    while let Some(flag) = args.next() {
        let target = match flag.as_str() {
            "--seed" => &mut options.seed,
            "--ticks" => &mut options.ticks,
            _ => return Err(format!("unknown argument `{flag}`")),
        };
        let value = args
            .next()
            .ok_or_else(|| format!("`{flag}` needs a value"))?;
        *target = value
            .parse()
            .map_err(|_| format!("`{flag}` expects a whole number, got `{value}`"))?;
    }
    Ok(options)
}

#[expect(
    clippy::print_stdout,
    reason = "the result line is this program's output"
)]
fn report_result(tick: u64, state_hash: u64) {
    println!("tick {tick} state {state_hash:016x}");
}

#[expect(clippy::print_stderr, reason = "usage errors go to the terminal")]
fn report_usage_error(message: &str) {
    eprintln!("error: {message}");
    eprintln!("usage: expanse-headless [--seed N] [--ticks N]");
}
