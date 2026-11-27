//! Offline asset cooking: from sources of art to the cooked container in
//! `expanse-assets`.
//!
//! Every source of art becomes [source assets](source), and everything from
//! there is shared: packing on atlas pages, padding, mipmap generation,
//! checking and writing. The game never depends on this crate: it only loads
//! containers through `expanse-assets`.

mod cook;
mod mipmap;
mod pack;
pub mod placeholder;
mod source;

pub use cook::{CookError, cook};
pub use source::{InvalidSource, SourceGround, SourceGroundPair, SourceSprite, Sources};
