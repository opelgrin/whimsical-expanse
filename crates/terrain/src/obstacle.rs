//! Obstacle footprints, and why placing one can fail.

use std::error::Error;
use std::fmt;

use expanse_core::grid::{TileOffset, TilePos};

use crate::tile::PartOffset;

/// The tiles an obstacle blocks, as offsets from its anchor tile.
///
/// The anchor is always part of the footprint. A footprint is content, and is
/// unrelated to the obstacle's sprite: a tree's image may cover several tiles
/// while it blocks only its trunk.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Footprint {
    /// The tiles other than the anchor, each distinct and non-zero: the
    /// offset from the anchor, and the way back that the tile stores.
    others: Vec<(TileOffset, PartOffset)>,
}

impl Footprint {
    /// A footprint of the anchor tile alone.
    #[must_use]
    pub const fn single_tile() -> Self {
        Self { others: Vec::new() }
    }

    /// A footprint of the anchor and the tiles at `others` from it.
    ///
    /// # Errors
    ///
    /// Returns an error if an offset repeats another or the anchor, or reaches
    /// too far from the anchor to be stored.
    pub fn new(others: impl IntoIterator<Item = TileOffset>) -> Result<Self, InvalidFootprint> {
        let mut checked: Vec<(TileOffset, PartOffset)> = Vec::new();
        for offset in others {
            let repeated = checked.iter().any(|&(earlier, _)| earlier == offset);
            if offset == TileOffset::new(0, 0) || repeated {
                return Err(InvalidFootprint::Repeated(offset));
            }
            let back = PartOffset::back_from(offset).ok_or(InvalidFootprint::TooFar(offset))?;
            checked.push((offset, back));
        }
        Ok(Self { others: checked })
    }

    /// The tiles other than the anchor: the offset from the anchor, and the
    /// way back.
    pub(crate) fn others(&self) -> &[(TileOffset, PartOffset)] {
        &self.others
    }
}

/// Why a [`Footprint`] was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidFootprint {
    /// The offset names the anchor or a tile already listed.
    Repeated(TileOffset),
    /// The offset reaches further from the anchor than a tile can record.
    TooFar(TileOffset),
}

impl fmt::Display for InvalidFootprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repeated(offset) => write!(
                f,
                "the footprint lists the tile at ({}, {}) twice; the anchor is always included",
                offset.dx, offset.dy
            ),
            Self::TooFar(offset) => write!(
                f,
                "the footprint tile at ({}, {}) is too far from the anchor",
                offset.dx, offset.dy
            ),
        }
    }
}

impl Error for InvalidFootprint {}

/// Why an obstacle could not be placed. Nothing was changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementError {
    /// Part of the footprint lies beyond the north or south edge.
    BeyondEdge,
    /// The canonical tile is already taken, by another obstacle or by another
    /// tile of the same footprint wrapped around the world.
    Blocked(TilePos),
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeyondEdge => write!(f, "the footprint reaches beyond the north or south edge"),
            Self::Blocked(tile) => write!(f, "the tile ({}, {}) is already taken", tile.x, tile.y),
        }
    }
}

impl Error for PlacementError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches footprints that list a tile twice, which would make placement
    /// collide with itself, and offsets that tiles could not store, which
    /// would make placement panic.
    #[test]
    fn rejects_repeated_tiles_and_offsets_too_far_to_store() {
        let east = TileOffset::new(1, 0);
        assert_eq!(
            Footprint::new([east, east]),
            Err(InvalidFootprint::Repeated(east))
        );
        let anchor = TileOffset::new(0, 0);
        assert_eq!(
            Footprint::new([anchor]),
            Err(InvalidFootprint::Repeated(anchor))
        );
        let far = TileOffset::new(0, -128);
        assert_eq!(Footprint::new([far]), Err(InvalidFootprint::TooFar(far)));
        let furthest = [TileOffset::new(127, 0), TileOffset::new(-127, -127)];
        assert!(Footprint::new(furthest).is_ok());
    }
}
