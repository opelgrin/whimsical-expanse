//! The terrain: the world's tiles, stored in chunks, with their fields, biome
//! and obstacles.
//!
//! After genesis the terrain is data. Nothing recomputes what a tile holds from
//! the seed. Chunks are shared behind immutable pointers, so presentation can
//! hold the terrain without copying it, and a change replaces only the chunks
//! it touches.

mod chunks;
mod obstacle;
mod tile;

pub use chunks::{CHUNK_SIZE, ChunkPos, Terrain};
pub use obstacle::{Footprint, InvalidFootprint, PlacementError};
pub use tile::{BiomeIndex, Fields, ObstacleCell, ObstacleIndex, PartOffset, Tile};
