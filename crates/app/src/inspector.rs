//! The tile inspector: what the terrain holds at one tile, for the debug
//! overlay to show.

use expanse_core::grid::TilePos;
use expanse_terrain::{
    BiomeIndex, ChunkPos, Fields, Latitude, ObstacleCell, ObstacleIndex, Terrain,
};

/// What the terrain holds at one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileReport {
    /// The tile, canonical.
    pub tile: TilePos,
    /// The chunk that holds the tile.
    pub chunk: ChunkPos,
    /// The latitude of the tile's row, if the space has one.
    pub latitude: Option<Latitude>,
    pub fields: Fields,
    pub biome: BiomeIndex,
    pub obstacle: ObstacleReport,
}

/// What a tile holds of an obstacle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleReport {
    /// No obstacle.
    None,
    /// The anchor of an obstacle of this kind.
    Anchor(ObstacleIndex),
    /// Another tile of an obstacle's footprint.
    Part {
        /// The obstacle's kind, from its anchor, or `None` if the anchor
        /// holds no obstacle, which a valid terrain never allows.
        kind: Option<ObstacleIndex>,
        /// The anchor, canonical.
        anchor: TilePos,
    },
}

/// What `terrain` holds at `tile`, wrapping across the seam, or `None`
/// beyond the north and south edges.
#[must_use]
pub fn inspect(terrain: &Terrain, tile: TilePos) -> Option<TileReport> {
    let grid = terrain.grid();
    let tile = grid.canonical(tile)?;
    let held = terrain.tile(tile)?;
    let obstacle = match held.obstacle() {
        ObstacleCell::Empty => ObstacleReport::None,
        ObstacleCell::Anchor(kind) => ObstacleReport::Anchor(kind),
        ObstacleCell::Part(back) => {
            let anchor = grid.translate(tile, back.into())?;
            let kind = match terrain.tile(anchor).map(expanse_terrain::Tile::obstacle) {
                Some(ObstacleCell::Anchor(kind)) => Some(kind),
                _ => None,
            };
            ObstacleReport::Part { kind, anchor }
        }
    };
    Some(TileReport {
        tile,
        chunk: terrain.chunk_containing(tile)?,
        latitude: terrain.row_latitude(tile.y),
        fields: held.fields,
        biome: held.biome,
        obstacle,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use expanse_core::grid::TileOffset;
    use expanse_terrain::{CHUNK_SIZE, Footprint, Tile};

    use super::*;

    /// Catches a footprint tile reported with its own position as the
    /// anchor, an anchor beyond the seam instead of canonical, or a kind
    /// read from the wrong tile. The obstacle is anchored on the east edge
    /// and reaches across the seam.
    #[test]
    fn a_footprint_tile_across_the_seam_reports_its_anchor_and_kind() {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        let mut terrain = Terrain::new(2, 1, |_| Tile::new(fields, BiomeIndex(0), false))
            .expect("a terrain with tiles");
        let east_edge = terrain.grid().width() - 1;
        let anchor = TilePos::new(east_edge, 3);
        let footprint = Footprint::new([TileOffset::new(1, 0)]).expect("a valid footprint");
        terrain
            .place_obstacle(anchor, ObstacleIndex(5), &footprint)
            .expect("free tiles");

        let part = inspect(&terrain, TilePos::new(0, 3)).expect("a tile");
        assert_eq!(
            part.obstacle,
            ObstacleReport::Part {
                kind: Some(ObstacleIndex(5)),
                anchor,
            }
        );
        let anchor_report = inspect(&terrain, TilePos::new(-1, 3)).expect("a tile");
        assert_eq!(anchor_report.tile, anchor);
        assert_eq!(anchor_report.chunk, ChunkPos { x: 1, y: 0 });
        assert_eq!(
            anchor_report.obstacle,
            ObstacleReport::Anchor(ObstacleIndex(5))
        );
        assert_eq!(inspect(&terrain, TilePos::new(0, CHUNK_SIZE)), None);
    }
}
