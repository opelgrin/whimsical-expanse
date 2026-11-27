//! The `cook` tool: cooks the assets into the container the game loads.
//!
//! For now its only source of art is the placeholder recipes. The licensed
//! pack and our own art join as further sources in M1 item 7.

mod default_recipes;

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use expanse_assets::{Container, write_container};
use expanse_cook::cook;
use expanse_cook::placeholder::draw_placeholders;

fn main() -> ExitCode {
    let out = match parse_out(std::env::args().skip(1)) {
        Ok(out) => out,
        Err(message) => {
            report_error(&message);
            report_usage();
            return ExitCode::from(2);
        }
    };
    match run(&out) {
        Ok(container) => {
            report_summary(&container, &out);
            ExitCode::SUCCESS
        }
        Err(message) => {
            report_error(&message);
            ExitCode::FAILURE
        }
    }
}

fn run(out: &Path) -> Result<Container, String> {
    let sources = draw_placeholders(&default_recipes::recipes())
        .map_err(|err| format!("invalid placeholder recipe: {err}"))?;
    let container = cook(&sources).map_err(|err| format!("cannot cook: {err}"))?;
    write_atomically(&container, out)
        .map_err(|err| format!("cannot write `{}`: {err}", out.display()))?;
    Ok(container)
}

/// Writes beside `out`, then renames, so that the game never finds a
/// half-written container.
fn write_atomically(container: &Container, out: &Path) -> std::io::Result<()> {
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let partial = out.with_extension("partial");
    let mut writer = BufWriter::new(File::create(&partial)?);
    write_container(container, &mut writer)?;
    writer
        .into_inner()
        .map_err(std::io::IntoInnerError::into_error)?
        .sync_all()?;
    fs::rename(&partial, out)
}

fn parse_out(mut args: impl Iterator<Item = String>) -> Result<PathBuf, String> {
    let mut out = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--out" => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--out` needs a path".to_owned())?;
                out = Some(PathBuf::from(value));
            }
            _ => return Err(format!("unknown argument `{flag}`")),
        }
    }
    out.ok_or_else(|| "`--out` is required".to_owned())
}

#[expect(clippy::print_stdout, reason = "the tool reports what it cooked")]
fn report_summary(container: &Container, out: &Path) {
    let pages: Vec<String> = container
        .atlases()
        .iter()
        .map(|atlas| format!("{}×{}", atlas.width(), atlas.height()))
        .collect();
    println!(
        "cooked {} grounds and {} sprites from placeholders into {} atlas page(s) ({}): {}",
        container.grounds().len(),
        container.sprites().len(),
        pages.len(),
        pages.join(", "),
        out.display()
    );
}

#[expect(clippy::print_stderr, reason = "CLI errors go to stderr")]
fn report_error(message: &str) {
    eprintln!("error: {message}");
}

#[expect(clippy::print_stderr, reason = "usage help goes to stderr")]
fn report_usage() {
    eprintln!("usage: expanse-cook --out PATH");
}
