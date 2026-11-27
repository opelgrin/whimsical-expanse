//! The cooked asset format: the container the game loads, the 47-tile blob
//! scheme, plain RGBA images, and the binary reader and writer.
//!
//! Offline asset cooking (placeholder drawing, packing, mipmap generation,
//! and reading external art) lives in `tools/cook`, which depends on this
//! crate. Runtime crates (`render`, `app`) depend only on this crate so no
//! offline cooking machinery or dependencies enter the game binary.

pub mod blob;
mod container;
mod format;
mod image;

pub use container::{
    Atlas, CELL, Container, GroundEntry, GroundIndex, GroundPairEntry, Overhang, PADDING, Region,
    SpriteEntry, SpriteIndex,
};
pub use format::{FORMAT_VERSION, ReadError, read_container, write_container};
pub use image::{Image, Rgba, TRANSPARENT};

/// The side of a tile, in pixels.
pub const TILE_SIZE: u32 = 16;

/// The number of seasonal variants of an asset that has them, in calendar
/// order from spring.
pub const SEASONS: usize = 4;

/// The size of a blob block, in pixels.
pub const BLOCK_SIZE: [u32; 2] = [
    blob::BLOCK_COLUMNS * TILE_SIZE,
    blob::BLOCK_ROWS * TILE_SIZE,
];

/// The maximum width and height of an atlas page, in pixels. Every GPU that
/// `wgpu` supports accepts textures of this size.
pub const MAX_ATLAS_SIZE: u32 = 2048;
