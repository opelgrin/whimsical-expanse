//! The field passes: elevation, temperature, moisture and vegetation.
//!
//! Each pass reads what earlier passes wrote and writes one layer, quantised
//! to the terrain's scale. Unquantised values stay inside the generator.

use expanse_terrain::{SEA_LEVEL, temperature_from_celsius};
use rayon::prelude::*;

use crate::layer::{rank, smooth};
use crate::noise::Noise;
use crate::params::{FieldParams, TemperatureParams};
use crate::space::{Space, stream};

/// The elevation layer.
pub(crate) struct Elevation {
    /// Quantised: ranked below and above sea level.
    pub values: Vec<u16>,
    /// How high each tile is among the land tiles, from 0 at sea level to 1
    /// on the highest tile. 0 under water.
    pub land_height: Vec<f64>,
}

impl Elevation {
    /// Whether the tile at row-major `index` is under water.
    pub(crate) fn is_water(&self, index: usize) -> bool {
        self.values[index] < SEA_LEVEL
    }
}

/// Elevation: noise, smoothed, then ranked so that `water_share` of the
/// tiles lie below sea level.
pub(crate) fn elevation(space: &Space, params: &FieldParams, water_share: f64) -> Elevation {
    let grid = space.grid();
    let mut noise = Noise::new(space.stream(&[stream::ELEVATION]), params.noise).field(space);
    smooth(grid, &mut noise, params.smoothing_radius);

    let count = grid.tile_count();
    let mut order: Vec<(f64, usize)> = noise.par_iter().copied().zip(0..count).collect();
    order.par_sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let order: Vec<usize> = order.into_par_iter().map(|(_, index)| index).collect();
    let water_count = share_of(count, water_share);
    let (water, land) = order.split_at(water_count);

    let mut values = vec![0; count];
    let sea_level = u32::from(SEA_LEVEL);
    rank(&noise, water, 0, sea_level, &mut values);
    rank(&noise, land, sea_level, sea_level, &mut values);

    let mut land_height = vec![0.0; count];
    let highest_rank = land.len().saturating_sub(1).max(1);
    let highest_rank = f64::from(u32::try_from(highest_rank).expect("fewer than 2^32 tiles"));
    // Sequential: the land tiles are scattered over the layer. Each write is
    // cheap.
    for (rank, &index) in land.iter().enumerate() {
        land_height[index] =
            f64::from(u32::try_from(rank).expect("fewer than 2^32 tiles")) / highest_rank;
    }
    Elevation {
        values,
        land_height,
    }
}

/// The winter and summer temperatures, quantised.
pub(crate) struct Temperatures {
    pub winter: Vec<u16>,
    pub summer: Vec<u16>,
}

/// Temperature: the climate of the tile's latitude, cooled by altitude and
/// moved by noise, then smoothed.
///
/// The annual mean falls with the cosine of the latitude, which is flatter
/// near the equator than a straight line, and the swing grows with its sine.
pub(crate) fn temperature(
    space: &Space,
    params: &TemperatureParams,
    elevation: &Elevation,
) -> Temperatures {
    let grid = space.grid();
    let noise = Noise::new(space.stream(&[stream::TEMPERATURE]), params.noise).field(space);
    let (mut winter, mut summer): (Vec<f64>, Vec<f64>) = elevation
        .land_height
        .par_iter()
        .enumerate()
        .map(|(index, &land_height)| {
            let (sin, cos) = libm::sincos(space.latitude(index).degrees().to_radians());
            let mean = params.pole_mean_celsius
                + (params.equator_mean_celsius - params.pole_mean_celsius) * cos;
            let swing = params.equator_swing_celsius
                + (params.pole_swing_celsius - params.equator_swing_celsius) * sin.abs();
            // Continentality, a factor on the swing, is 1 until a pass
            // computes it from the distance to the sea.
            let continentality = 1.0;
            let half_swing = swing * continentality / 2.0;
            let shift = noise[index] * params.noise_strength_celsius
                - land_height * params.cooling_at_highest_celsius;
            (mean - half_swing + shift, mean + half_swing + shift)
        })
        .unzip();
    smooth(grid, &mut winter, params.smoothing_radius);
    smooth(grid, &mut summer, params.smoothing_radius);
    Temperatures {
        winter: winter
            .into_par_iter()
            .map(temperature_from_celsius)
            .collect(),
        summer: summer
            .into_par_iter()
            .map(temperature_from_celsius)
            .collect(),
    }
}

/// A field from noise, smoothed, then ranked among the land tiles, with the
/// water tiles ranked among themselves. Used for moisture and vegetation.
pub(crate) fn ranked_on_land(
    space: &Space,
    params: &FieldParams,
    label: u64,
    elevation: &Elevation,
) -> Vec<u16> {
    let grid = space.grid();
    let mut noise = Noise::new(space.stream(&[label]), params.noise).field(space);
    smooth(grid, &mut noise, params.smoothing_radius);
    let (water, land): (Vec<usize>, Vec<usize>) = (0..grid.tile_count())
        .into_par_iter()
        .partition(|&index| elevation.is_water(index));
    let mut values = vec![0; grid.tile_count()];
    let full_range = 1 << 16;
    rank(&noise, &water, 0, full_range, &mut values);
    rank(&noise, &land, 0, full_range, &mut values);
    values
}

/// The number of tiles, out of `count`, closest to `share` of them.
fn share_of(count: usize, share: f64) -> usize {
    let count_f64 = f64::from(u32::try_from(count).expect("fewer than 2^32 tiles"));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a share between 0 and 1 of the tile count, rounded"
    )]
    let tiles = (share * count_f64).round() as usize;
    tiles.min(count)
}
