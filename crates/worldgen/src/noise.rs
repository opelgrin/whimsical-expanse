//! Seamless gradient noise, written in-house so that a seed gives the same
//! world on every platform.
//!
//! The noise is three-dimensional and sampled on a cylinder: a tile's column
//! becomes an angle around it, and its row the height along it. The east and
//! west edges therefore meet without a seam, and features keep the same size
//! everywhere, since the cylinder's circumference is the surface's width.
//!
//! Only operations that IEEE 754 rounds exactly are used, with sine and
//! cosine from `libm`.

use std::f64::consts::TAU;

use expanse_core::rng::stream_key;
use rayon::prelude::*;

use crate::params::NoiseParams;
use crate::space::Space;

/// A noise with its own stream of randomness.
pub(crate) struct Noise {
    params: NoiseParams,
    /// One seed per octave.
    octave_seeds: Vec<u64>,
    /// The sum of the octaves' amplitudes, which brings the sum back to the
    /// range of one octave.
    total_amplitude: f64,
}

impl Noise {
    /// The noise of the stream `seed`, with `params`.
    pub(crate) fn new(seed: u64, params: NoiseParams) -> Self {
        let octave_seeds = (0..params.octaves)
            .map(|octave| stream_key(seed, &[u64::from(octave)]))
            .collect();
        let mut total_amplitude = 0.0;
        let mut amplitude = 1.0;
        for _ in 0..params.octaves {
            total_amplitude += amplitude;
            amplitude *= params.persistence;
        }
        Self {
            params,
            octave_seeds,
            total_amplitude,
        }
    }

    /// The noise at the centre of the tile at row-major `index`, between −1
    /// and 1, and 0 on average.
    pub(crate) fn at(&self, space: &Space, index: usize) -> f64 {
        let (circle_x, circle_y, row) = space.cylinder_point(index);
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0 / self.params.scale_tiles;
        for &seed in &self.octave_seeds {
            let point = [circle_x * frequency, circle_y * frequency, row * frequency];
            sum += amplitude * gradient_noise(seed, point);
            amplitude *= self.params.persistence;
            frequency *= 2.0;
        }
        (sum / self.total_amplitude).clamp(-1.0, 1.0)
    }

    /// The noise at every tile, in row-major order.
    pub(crate) fn field(&self, space: &Space) -> Vec<f64> {
        (0..space.grid().tile_count())
            .into_par_iter()
            .map(|index| self.at(space, index))
            .collect()
    }
}

/// The point on a circle whose circumference is `width`, at `column` of it.
pub(crate) fn circle_point(column: f64, width: f64) -> (f64, f64) {
    let radius = width / TAU;
    let (sin, cos) = libm::sincos(column / width * TAU);
    (radius * cos, radius * sin)
}

/// The gradients of the lattice: the midpoints of a cube's edges.
const GRADIENTS: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

/// Gradient noise (Perlin's improved noise) at `point`, about −1 to 1: a
/// random gradient at each point of the integer lattice, blended smoothly
/// between them.
fn gradient_noise(seed: u64, point: [f64; 3]) -> f64 {
    let cell = point.map(f64::floor);
    let offset = [point[0] - cell[0], point[1] - cell[1], point[2] - cell[2]];
    let cell = cell.map(lattice_coordinate);
    let mut corners = [0.0; 8];
    for (corner, value) in corners.iter_mut().enumerate() {
        let step = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
        let lattice =
            [0, 1, 2].map(|axis| (cell[axis] + i64::from(step[axis] == 1)).cast_unsigned());
        let hash = lattice_hash(seed, lattice);
        let gradient = GRADIENTS[usize::try_from(hash % 12).expect("below 12")];
        *value = (0..3)
            .map(|axis| gradient[axis] * (offset[axis] - f64::from(u8::from(step[axis] == 1))))
            .sum();
    }
    let fade = offset.map(|t| t * t * t * (t * (t * 6.0 - 15.0) + 10.0));
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x = [0, 2, 4, 6].map(|corner| lerp(corners[corner], corners[corner + 1], fade[0]));
    let y = [lerp(x[0], x[1], fade[1]), lerp(x[2], x[3], fade[1])];
    lerp(y[0], y[1], fade[2])
}

/// Random bits for a point of the lattice. The coordinates are spread by
/// large odd multipliers before one round of mixing: enough to choose a
/// gradient, and cheaper than a stream key over each coordinate.
fn lattice_hash(seed: u64, lattice: [u64; 3]) -> u64 {
    let spread = lattice[0].wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ lattice[1].wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ lattice[2].wrapping_mul(0x1656_67b1_9e37_79f9);
    stream_key(seed, &[spread])
}

/// A lattice coordinate from a whole number. Surfaces are far smaller than
/// the range of `i64`.
fn lattice_coordinate(whole: f64) -> i64 {
    debug_assert!(whole.abs() < 1e15 && whole.fract() == 0.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a whole number far within the range of i64"
    )]
    let coordinate = whole as i64;
    coordinate
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a lattice whose gradients do not meet at its points, which
    /// would show as a grid of creases: the noise is zero at every lattice
    /// point and continuous across cell faces. Values worked out by hand from
    /// the definition.
    #[test]
    fn the_noise_is_zero_on_the_lattice_and_continuous_across_cells() {
        for seed in 0..20 {
            for point in [[0.0, 0.0, 0.0], [3.0, -7.0, 12.0], [-1.0, 5.0, -2.0]] {
                assert_eq!(gradient_noise(seed, point), 0.0);
            }
            let below = gradient_noise(seed, [2.5, 1.0 - 1e-9, 0.25]);
            let above = gradient_noise(seed, [2.5, 1.0 + 1e-9, 0.25]);
            assert!((below - above).abs() < 1e-6, "seed {seed}");
        }
    }
}
