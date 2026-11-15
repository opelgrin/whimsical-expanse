//! A trivial terrain pattern that stands in for world generation until it
//! exists.
//!
//! It fills every part of a tile, including obstacles that span several
//! tiles, so that the code reading terrain meets all of it. It means nothing:
//! the fields follow the coordinates, the biomes are latitude bands, and the
//! obstacles sit on a regular lattice.

use expanse_core::grid::{TileOffset, TilePos};
use expanse_terrain::{BiomeIndex, CHUNK_SIZE, Fields, Footprint, ObstacleIndex, Terrain, Tile};

/// How many latitude bands of biomes the pattern has.
const BIOME_BANDS: i32 = 3;

/// A terrain of `world_width_chunks` by `world_height_chunks` chunks, or `None` if it would
/// have no tiles. The tile at the centre is always free.
pub fn terrain(world_width_chunks: u16, world_height_chunks: u16) -> Option<Terrain> {
    let height = i32::from(world_height_chunks) * CHUNK_SIZE;
    let mut terrain = Terrain::new(world_width_chunks, world_height_chunks, |tile| {
        let biome = u16::try_from(tile.y * BIOME_BANDS / height).expect("a few bands");
        Tile::new(fields(tile), BiomeIndex(biome))
    })?;

    let single = Footprint::single_tile();
    let wide =
        Footprint::new([TileOffset::new(1, 0), TileOffset::new(2, 0)]).expect("a valid footprint");
    let grid = terrain.grid();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let (obstacle, footprint) = match (x % 16, y % 8) {
                (3 | 11, 0) => (ObstacleIndex(0), &single),
                (8, 4) => (ObstacleIndex(1), &wide),
                _ => continue,
            };
            terrain
                .place_obstacle(TilePos::new(x, y), obstacle, footprint)
                .expect("the pattern's obstacles never overlap");
        }
    }
    Some(terrain)
}

fn fields(tile: TilePos) -> Fields {
    Fields {
        elevation: ramp(tile.x),
        temperature: ramp(tile.y),
        moisture: ramp(tile.x + tile.y),
    }
}

/// A value that climbs over 256 tiles, then starts again.
fn ramp(position: i32) -> u16 {
    u16::try_from(position.rem_euclid(256) * 256).expect("below 65,536")
}
