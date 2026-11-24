//! Stand-in: replaced when the player chooses the parameters as a game starts.
//!
//! The default parameters of the surface.

use expanse_worldgen::{FieldParams, NoiseParams, SurfaceParams, TemperatureParams};

/// Width of the surface, in chunks.
pub const WIDTH_CHUNKS: u16 = 64;
/// Height of the surface, in chunks.
pub const HEIGHT_CHUNKS: u16 = 32;

/// The default surface: a band from cold to subtropical latitudes.
pub const fn surface_params() -> SurfaceParams {
    SurfaceParams {
        width_chunks: WIDTH_CHUNKS,
        height_chunks: HEIGHT_CHUNKS,
        north_latitude_degrees: 60.0,
        south_latitude_degrees: 20.0,
        water_share: 0.35,
        elevation: FieldParams {
            noise: NoiseParams {
                scale_tiles: 160.0,
                octaves: 6,
                persistence: 0.5,
            },
            smoothing_radius: 1,
        },
        temperature: TemperatureParams {
            equator_mean_celsius: 27.0,
            pole_mean_celsius: -18.0,
            equator_swing_celsius: 3.0,
            pole_swing_celsius: 40.0,
            cooling_at_highest_celsius: 25.0,
            noise: NoiseParams {
                scale_tiles: 200.0,
                octaves: 3,
                persistence: 0.5,
            },
            noise_strength_celsius: 3.0,
            smoothing_radius: 4,
        },
        moisture: FieldParams {
            noise: NoiseParams {
                scale_tiles: 120.0,
                octaves: 5,
                persistence: 0.5,
            },
            smoothing_radius: 1,
        },
        vegetation: FieldParams {
            noise: NoiseParams {
                scale_tiles: 90.0,
                octaves: 4,
                persistence: 0.5,
            },
            smoothing_radius: 1,
        },
        min_region_tiles: 24,
    }
}
