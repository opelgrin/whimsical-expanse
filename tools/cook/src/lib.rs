//! Asset cooking: from sources of art to the container the game loads.
//!
//! Every source of art becomes [source assets](source), and everything from
//! there is shared: packing on atlas pages, padding, checking and writing.
//! The game only ever loads a container from disk, so a build without the
//! licensed pack runs the same path as one with it.
//!
//! The library holds no GPU types: the renderer reads the [`Container`] and
//! uploads it.

pub mod blob;
mod container;
mod cook;
mod format;
mod image;
mod pack;
pub mod placeholder;
mod source;

pub use container::{
    CELL, Container, GroundEntry, GroundIndex, Overhang, PADDING, Region, SpriteEntry, SpriteIndex,
};
pub use cook::{CookError, cook};
pub use format::{FORMAT_VERSION, ReadError, read_container, write_container};
pub use image::{Image, Rgba};
pub use pack::MAX_ATLAS_SIZE;
pub use source::{
    BLOCK_SIZE, InvalidSource, SEASONS, SourceGround, SourceSprite, Sources, TILE_SIZE,
};
