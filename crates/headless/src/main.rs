//! Runs the simulation without a window, as fast as possible, and prints the
//! final tick and state hash.
//!
//! Two runs with the same arguments must print the same line. CI checks this on
//! every platform.
//!
//! Usage: `expanse-headless [--seed N] [--ticks N]`

use std::process::ExitCode;

use expanse_sim::{World, WorldConfig};

/// Grid and movement parameters for headless runs. They only need to exercise
/// the simulation; they are not game tuning.
const WIDTH: i32 = 64;
const HEIGHT: i32 = 64;
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
    let config = WorldConfig {
        seed: options.seed,
        width: WIDTH,
        height: HEIGHT,
        ticks_per_step: TICKS_PER_STEP,
    };
    let mut world = World::new(config).expect("the headless parameters are valid");
    for _ in 0..options.ticks {
        world.advance();
    }
    report_result(world.tick().count(), world.state_hash());
    ExitCode::SUCCESS
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
