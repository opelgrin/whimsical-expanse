//! What one tile holds.

use expanse_core::grid::TileOffset;

/// One tile of terrain: its fields, its biome and the obstacle on it, if any.
///
/// Obstacles are placed only through
/// [`Terrain::place_obstacle`](crate::Terrain::place_obstacle), which keeps
/// footprints whole, so a new tile has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub fields: Fields,
    pub biome: BiomeIndex,
    obstacle: ObstacleCell,
}

impl Tile {
    /// A tile without an obstacle.
    #[must_use]
    pub const fn new(fields: Fields, biome: BiomeIndex) -> Self {
        Self {
            fields,
            biome,
            obstacle: ObstacleCell::Empty,
        }
    }

    /// The obstacle on this tile, or the part of one.
    #[must_use]
    pub const fn obstacle(&self) -> ObstacleCell {
        self.obstacle
    }

    pub(crate) const fn set_obstacle(&mut self, obstacle: ObstacleCell) {
        self.obstacle = obstacle;
    }
}

/// The values from which a tile's biome is chosen.
///
/// Each is quantised to the whole `u16` range: 0 is the lowest value the
/// generator produces and `u16::MAX` the highest. Integers keep the terrain
/// small, exactly comparable and hashable, and make every value valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fields {
    pub elevation: u16,
    pub temperature: u16,
    pub moisture: u16,
}

/// A biome, as its position in the biome table, which content supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BiomeIndex(pub u16);

/// A kind of obstacle, as its position in the obstacle list, which content
/// supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObstacleIndex(pub u16);

/// What a tile holds of an obstacle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObstacleCell {
    /// No obstacle.
    Empty,
    /// The anchor tile of an obstacle, which says which obstacle it is.
    Anchor(ObstacleIndex),
    /// Another tile of an obstacle's footprint, with the offset from this tile
    /// to the anchor.
    Part(PartOffset),
}

/// The offset from a footprint tile to its anchor, small enough to keep tiles
/// compact. [`Footprint`](crate::Footprint) guarantees that every offset fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartOffset {
    dx: i8,
    dy: i8,
}

impl PartOffset {
    /// The offset back to the anchor from the tile at `from_anchor`, or `None`
    /// if it does not fit.
    pub(crate) fn back_from(from_anchor: TileOffset) -> Option<Self> {
        let dx = from_anchor.dx.checked_neg()?;
        let dy = from_anchor.dy.checked_neg()?;
        Some(Self {
            dx: i8::try_from(dx).ok()?,
            dy: i8::try_from(dy).ok()?,
        })
    }
}

impl From<PartOffset> for TileOffset {
    fn from(offset: PartOffset) -> Self {
        Self::new(i32::from(offset.dx), i32::from(offset.dy))
    }
}
