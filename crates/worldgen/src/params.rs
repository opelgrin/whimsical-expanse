//! The parameters of a generated space, and how they are checked.
//!
//! Every value the generator uses is a parameter. There are no defaults: the
//! game binary supplies them, and the player will choose some of them when a
//! game starts. Parameters use units a player understands: degrees of
//! latitude, degrees Celsius, tiles and shares of the surface.

use std::error::Error;
use std::fmt;

use expanse_terrain::{HIGHEST_CELSIUS, LOWEST_CELSIUS, Terrain};

/// Everything the surface is generated from, apart from the game's seed.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceParams {
    /// Width of the surface, in chunks.
    pub width_chunks: u16,
    /// Height of the surface, in chunks.
    pub height_chunks: u16,
    /// Latitude of the north edge, in degrees: positive north of the equator.
    pub north_latitude_degrees: f64,
    /// Latitude of the south edge, in degrees. Below the north edge's.
    pub south_latitude_degrees: f64,
    /// The share of tiles below sea level, from 0 to 1.
    pub water_share: f64,
    pub elevation: FieldParams,
    pub temperature: TemperatureParams,
    pub moisture: FieldParams,
    pub vegetation: FieldParams,
    /// The fewest tiles a region of one biome may have. Smaller regions take
    /// the biome around them.
    pub min_region_tiles: u32,
}

/// The parameters of a field that comes from noise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldParams {
    pub noise: NoiseParams,
    /// The radius, in tiles, over which the field is averaged. 0 for none.
    pub smoothing_radius: u16,
}

/// Smooth noise, as layers of detail called octaves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseParams {
    /// The size of the largest features, in tiles.
    pub scale_tiles: f64,
    /// The number of layers. Each has features half the size of the previous.
    pub octaves: u8,
    /// The strength of each layer relative to the previous one, above 0 and
    /// at most 1.
    pub persistence: f64,
}

/// The parameters of the winter and summer temperatures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemperatureParams {
    /// The annual mean temperature at the equator, at sea level.
    pub equator_mean_celsius: f64,
    /// The annual mean temperature at the poles, at sea level.
    pub pole_mean_celsius: f64,
    /// The difference between summer and winter at the equator.
    pub equator_swing_celsius: f64,
    /// The difference between summer and winter at the poles.
    pub pole_swing_celsius: f64,
    /// How much colder the highest tile is than sea level.
    pub cooling_at_highest_celsius: f64,
    pub noise: NoiseParams,
    /// How far the noise moves temperatures up or down, at most.
    pub noise_strength_celsius: f64,
    /// The radius, in tiles, over which temperatures are averaged. 0 for none.
    pub smoothing_radius: u16,
}

/// Why a set of parameters was rejected: the parameter, and what it must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidParams {
    pub parameter: String,
    pub requirement: &'static str,
}

impl InvalidParams {
    fn new(parameter: impl Into<String>, requirement: &'static str) -> Self {
        Self {
            parameter: parameter.into(),
            requirement,
        }
    }
}

impl fmt::Display for InvalidParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` {}", self.parameter, self.requirement)
    }
}

impl Error for InvalidParams {}

impl SurfaceParams {
    /// Checks that the parameters can generate a surface.
    pub(crate) fn check(&self) -> Result<(), InvalidParams> {
        let fail = |parameter: &str, requirement| Err(InvalidParams::new(parameter, requirement));
        if self.width_chunks == 0 {
            return fail("width_chunks", "must be at least 1");
        }
        if self.height_chunks == 0 {
            return fail("height_chunks", "must be at least 1");
        }
        let latitudes = -90.0..=90.0;
        if !latitudes.contains(&self.north_latitude_degrees) {
            return fail("north_latitude_degrees", "must be between -90 and 90");
        }
        if !latitudes.contains(&self.south_latitude_degrees) {
            return fail("south_latitude_degrees", "must be between -90 and 90");
        }
        if self.south_latitude_degrees >= self.north_latitude_degrees {
            return fail(
                "south_latitude_degrees",
                "must be below north_latitude_degrees",
            );
        }
        if !(0.0..=1.0).contains(&self.water_share) {
            return fail("water_share", "must be between 0 and 1");
        }
        let grid = Terrain::grid_for(self.width_chunks, self.height_chunks)
            .expect("both sizes are at least 1");
        let width_tiles = grid.width().unsigned_abs();
        let tile_count = grid.tile_count();
        let min_region_tiles = usize::try_from(self.min_region_tiles).expect("u32 fits in usize");
        if min_region_tiles == 0 || min_region_tiles > tile_count {
            return fail(
                "min_region_tiles",
                "must be at least 1 and at most the number of tiles",
            );
        }
        for (field, parameter) in [
            (&self.elevation, "elevation"),
            (&self.moisture, "moisture"),
            (&self.vegetation, "vegetation"),
        ] {
            check_noise(&field.noise, parameter)?;
            check_smoothing(field.smoothing_radius, width_tiles, parameter)?;
        }
        self.temperature.check(width_tiles)
    }
}

impl TemperatureParams {
    fn check(&self, width_tiles: u32) -> Result<(), InvalidParams> {
        let fail = |parameter: &str, requirement| Err(InvalidParams::new(parameter, requirement));
        let scale = LOWEST_CELSIUS..=HIGHEST_CELSIUS;
        let span = 0.0..=HIGHEST_CELSIUS - LOWEST_CELSIUS;
        let outside_scale = "must lie on the temperature scale, from -150 to 150 °C";
        let outside_span = "must be between 0 and 300 °C, the span of the temperature scale";
        if !scale.contains(&self.equator_mean_celsius) {
            return fail("temperature.equator_mean_celsius", outside_scale);
        }
        if !scale.contains(&self.pole_mean_celsius) {
            return fail("temperature.pole_mean_celsius", outside_scale);
        }
        for (value, parameter) in [
            (
                self.equator_swing_celsius,
                "temperature.equator_swing_celsius",
            ),
            (self.pole_swing_celsius, "temperature.pole_swing_celsius"),
            (
                self.cooling_at_highest_celsius,
                "temperature.cooling_at_highest_celsius",
            ),
            (
                self.noise_strength_celsius,
                "temperature.noise_strength_celsius",
            ),
        ] {
            if !span.contains(&value) {
                return fail(parameter, outside_span);
            }
        }
        check_noise(&self.noise, "temperature")?;
        check_smoothing(self.smoothing_radius, width_tiles, "temperature")
    }
}

fn check_noise(noise: &NoiseParams, field: &str) -> Result<(), InvalidParams> {
    let fail = |name, requirement| {
        Err(InvalidParams::new(
            format!("{field}.noise.{name}"),
            requirement,
        ))
    };
    if !(noise.scale_tiles >= 1.0 && noise.scale_tiles.is_finite()) {
        return fail("scale_tiles", "must be at least one tile");
    }
    if !(1..=MAX_OCTAVES).contains(&noise.octaves) {
        return fail("octaves", "must be between 1 and 16");
    }
    if !(noise.persistence > 0.0 && noise.persistence <= 1.0) {
        return fail("persistence", "must be above 0 and at most 1");
    }
    Ok(())
}

/// The most octaves a noise may have. Beyond it, features would be far
/// smaller than a tile.
const MAX_OCTAVES: u8 = 16;

fn check_smoothing(radius: u16, width_tiles: u32, field: &str) -> Result<(), InvalidParams> {
    if 2 * u32::from(radius) < width_tiles {
        return Ok(());
    }
    Err(InvalidParams::new(
        format!("{field}.smoothing_radius"),
        "must be less than half the surface width",
    ))
}
