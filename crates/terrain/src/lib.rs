//! The terrain: a space's tiles, stored in chunks, with their fields, biome
//! and obstacles, and the biome table that gives them meaning.
//!
//! After genesis the terrain is data. Nothing recomputes what a tile holds from
//! the seed. Chunks are shared behind immutable pointers, so presentation can
//! hold the terrain without copying it, and a change replaces only the chunks
//! it touches.

mod biome;
mod chunks;
mod climate;
mod field;
mod obstacle;
mod tile;

pub use biome::{
    Biome, BiomeDefinition, BiomeObstacle, BiomeObstacleDefinition, BiomeTable, FieldRanges,
    InvalidBiomeTable, ObstacleKind, ObstacleKindDefinition, ObstacleProblem, Variation,
};
pub use chunks::{CHUNK_SIZE, ChunkPos, ChunkStamp, Terrain};
pub use climate::{Latitude, TimeOfYear, temperature_at};
pub use field::{
    Field, Fields, HIGHEST_CELSIUS, LOWEST_CELSIUS, SEA_LEVEL, temperature_from_celsius,
};
pub use obstacle::{Footprint, InvalidFootprint, PlacementError};
pub use tile::{BiomeIndex, ObstacleCell, ObstacleIndex, PartOffset, Tile};
