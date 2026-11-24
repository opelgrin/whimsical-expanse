//! World generation: builds a space's terrain from the game's seed and the
//! space's parameters.
//!
//! The rest of the game sees only the [`Terrain`] it returns. Generation runs
//! in passes, each a step over the whole space that reads what earlier
//! passes wrote and writes one new layer:
//!
//! 1. **Elevation:** seamless noise, ranked around sea level.
//! 2. **Temperature:** the winter and summer temperatures, from latitude,
//!    altitude and noise.
//! 3. **Moisture:** seamless noise, ranked among land tiles.
//! 4. **Vegetation:** seamless noise, ranked among land tiles.
//! 5. **Biome:** each tile's biome, from its fields and the biome table.
//! 6. **Biome clean-up:** removes biome regions below a minimum size.
//! 7. **Obstacles:** places the obstacles of each biome.
//!
//! The passes and their intermediate data stay private, so adding, replacing
//! or reordering a pass changes only this crate.

mod biomes;
mod fields;
mod layer;
mod noise;
mod obstacles;
mod params;
mod space;

use expanse_terrain::{BiomeTable, Fields, Terrain, Tile};

pub use params::{FieldParams, InvalidParams, NoiseParams, SurfaceParams, TemperatureParams};

use crate::space::{Space, stream};

/// Generates the whole surface of the game whose seed is `seed`, before the
/// first tick.
///
/// The same seed, parameters and table give the same terrain, on every
/// platform.
///
/// # Errors
///
/// Returns an error naming the problem if the parameters are invalid.
pub fn generate_surface(
    seed: u64,
    params: &SurfaceParams,
    table: &BiomeTable,
) -> Result<Terrain, InvalidParams> {
    params.check()?;
    Ok(generate_checked_surface(seed, params, table))
}

/// Runs the passes, with parameters already checked.
fn generate_checked_surface(seed: u64, params: &SurfaceParams, table: &BiomeTable) -> Terrain {
    let space = Space::surface(seed, params);
    let grid = space.grid();

    let elevation = fields::elevation(&space, &params.elevation, params.water_share);
    let temperatures = fields::temperature(&space, &params.temperature, &elevation);
    let moisture = fields::ranked_on_land(&space, &params.moisture, stream::MOISTURE, &elevation);
    let vegetation =
        fields::ranked_on_land(&space, &params.vegetation, stream::VEGETATION, &elevation);
    let fields: Vec<Fields> = (0..grid.tile_count())
        .map(|index| Fields {
            elevation: elevation.values[index],
            winter_temperature: temperatures.winter[index],
            summer_temperature: temperatures.summer[index],
            moisture: moisture[index],
            vegetation: vegetation[index],
        })
        .collect();

    let mut biomes = biomes::classify(&fields, table);
    let min_tiles = usize::try_from(params.min_region_tiles).expect("u32 fits in usize");
    biomes::clean_up(grid, &mut biomes, table.biome_count(), min_tiles);

    let mut terrain = Terrain::new(params.width_chunks, params.height_chunks, |tile| {
        let index = grid.index(tile).expect("the terrain fills canonical tiles");
        let biome = biomes[index];
        Tile::new(fields[index], biome, table.biome(biome).is_impassable())
    })
    .expect("checked parameters give a terrain with tiles");
    obstacles::place(&mut terrain, &space, table, &biomes);
    terrain.with_row_latitudes(space.into_row_latitudes())
}
