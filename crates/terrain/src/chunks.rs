//! The terrain of the whole world, stored in shared chunks.

use std::sync::Arc;

use expanse_core::grid::{Grid, TilePos};
use expanse_core::region::Regions;
use rayon::prelude::*;

use crate::climate::Latitude;
use crate::obstacle::{Footprint, PlacementError};
use crate::tile::{ObstacleCell, ObstacleIndex, Tile};

/// The width and height of a chunk, in tiles.
///
/// A power of two. Chunks are a storage unit, so storage, level of detail and
/// navigation regions are what could argue for another size than 32.
pub const CHUNK_SIZE: i32 = 32;

/// A chunk, by its column and row of chunks. Chunk `(0, 0)` holds the tiles
/// from `(0, 0)` to `(CHUNK_SIZE - 1, CHUNK_SIZE - 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
}

impl ChunkPos {
    /// The chunk's tile with the smallest coordinates.
    #[must_use]
    pub const fn origin(self) -> TilePos {
        TilePos::new(self.x * CHUNK_SIZE, self.y * CHUNK_SIZE)
    }

    /// The chunk holding a canonical tile.
    const fn of(tile: TilePos) -> Self {
        Self {
            x: tile.x / CHUNK_SIZE,
            y: tile.y / CHUNK_SIZE,
        }
    }
}

/// A square block of tiles, row by row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Chunk {
    tiles: Box<[Tile]>,
}

/// The terrain of a space: every tile, in chunks, and the latitude of each
/// row if the space has one.
///
/// Each chunk sits behind a shared pointer, and so does the list of chunks.
/// Cloning the terrain therefore copies two pointers, whatever its size, and
/// a clone never sees later changes. A change copies only the chunks it
/// touches, and the list of pointers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Terrain {
    grid: Grid,
    world_width_chunks: i32,
    /// Row by row.
    chunks: Arc<Vec<Arc<Chunk>>>,
    /// The latitude of each row, from the north edge, if the space has one.
    row_latitudes: Option<Arc<[Latitude]>>,
}

impl Terrain {
    /// The grid of a terrain `world_width_chunks` by `world_height_chunks`
    /// chunks, or `None` if it would have no tiles.
    #[must_use]
    pub fn grid_for(world_width_chunks: u16, world_height_chunks: u16) -> Option<Grid> {
        Grid::new(
            i32::from(world_width_chunks) * CHUNK_SIZE,
            i32::from(world_height_chunks) * CHUNK_SIZE,
        )
    }

    /// A terrain of `world_width_chunks` by `world_height_chunks` chunks, or `None` if it
    /// would have no tiles.
    ///
    /// `fill` gives each tile, called once for every canonical position, in
    /// parallel and in no particular order. Tiles start without obstacles;
    /// [`place_obstacle`](Self::place_obstacle) adds them.
    pub fn new(
        world_width_chunks: u16,
        world_height_chunks: u16,
        fill: impl Fn(TilePos) -> Tile + Sync,
    ) -> Option<Self> {
        let grid = Self::grid_for(world_width_chunks, world_height_chunks)?;
        let world_width_chunks = i32::from(world_width_chunks);
        let world_height_chunks = i32::from(world_height_chunks);
        let chunks = (0..world_height_chunks)
            .into_par_iter()
            .flat_map_iter(|y| (0..world_width_chunks).map(move |x| ChunkPos { x, y }))
            .map(|chunk| {
                let origin = chunk.origin();
                let tiles = (0..CHUNK_SIZE)
                    .flat_map(|y| (0..CHUNK_SIZE).map(move |x| TilePos::new(x, y)))
                    .map(|local| fill(TilePos::new(origin.x + local.x, origin.y + local.y)))
                    .collect();
                Arc::new(Chunk { tiles })
            })
            .collect();
        Some(Self {
            grid,
            world_width_chunks,
            chunks: Arc::new(chunks),
            row_latitudes: None,
        })
    }

    /// The same terrain, with `latitudes` as the latitude of each row, from
    /// the north edge.
    ///
    /// # Panics
    ///
    /// Panics unless there is one latitude per row.
    #[must_use]
    pub fn with_row_latitudes(mut self, latitudes: Vec<Latitude>) -> Self {
        assert_eq!(
            latitudes.len(),
            usize::try_from(self.grid.height()).expect("a positive height"),
            "one latitude per row"
        );
        self.row_latitudes = Some(latitudes.into());
        self
    }

    /// The latitude of row `y`, or `None` if the space has no latitude or the
    /// row lies beyond the north or south edge.
    #[must_use]
    pub fn row_latitude(&self, y: i32) -> Option<Latitude> {
        let latitudes = self.row_latitudes.as_ref()?;
        latitudes.get(usize::try_from(y).ok()?).copied()
    }

    /// The shape of the grid the terrain covers.
    #[must_use]
    pub const fn grid(&self) -> Grid {
        self.grid
    }

    /// The tile at `tile`, wrapping across the seam, or `None` beyond the
    /// north and south edges.
    #[must_use]
    pub fn tile(&self, tile: TilePos) -> Option<&Tile> {
        let (chunk, local) = self.locate(tile)?;
        Some(&self.chunks[chunk].tiles[local])
    }

    /// The chunk that holds `tile`, wrapping across the seam, or `None` beyond
    /// the north and south edges.
    #[must_use]
    pub fn chunk_containing(&self, tile: TilePos) -> Option<ChunkPos> {
        self.grid.canonical(tile).map(ChunkPos::of)
    }

    /// Whether movement onto `tile` is blocked, by an obstacle or by an
    /// impassable biome. Tiles beyond the north and south edges count as
    /// blocked.
    #[must_use]
    pub fn is_blocked(&self, tile: TilePos) -> bool {
        self.tile(tile)
            .is_none_or(|tile| tile.impassable || tile.obstacle() != ObstacleCell::Empty)
    }

    /// The tiles of the largest region of unblocked tiles, joined by their
    /// edges and across the seam, or `None` if every tile is blocked. On a
    /// tie, the region whose first tile comes first in row-major order.
    ///
    /// An entity placed there is never shut in a small pocket.
    #[must_use]
    pub fn largest_open_region(&self) -> Option<Vec<TilePos>> {
        let grid = self.grid;
        let regions = Regions::find(grid, |index| {
            (!self.is_blocked(grid.tile_at(index))).then_some(())
        });
        let largest = regions.largest()?;
        Some(
            regions
                .tiles(largest)
                .map(|index| grid.tile_at(index))
                .collect(),
        )
    }

    /// Places `obstacle` with its anchor at `anchor` and its `footprint`
    /// around it, across the seam and chunk boundaries if needed.
    ///
    /// # Errors
    ///
    /// Returns an error, and changes nothing, if part of the footprint lies
    /// beyond the north or south edge, or on a tile that already holds an
    /// obstacle. An impassable tile can hold one.
    pub fn place_obstacle(
        &mut self,
        anchor: TilePos,
        obstacle: ObstacleIndex,
        footprint: &Footprint,
    ) -> Result<(), PlacementError> {
        let anchor = self
            .grid
            .canonical(anchor)
            .ok_or(PlacementError::BeyondEdge)?;
        let mut cells = vec![(anchor, ObstacleCell::Anchor(obstacle))];
        for &(offset, back) in footprint.others() {
            let tile = self
                .grid
                .translate(anchor, offset)
                .ok_or(PlacementError::BeyondEdge)?;
            cells.push((tile, ObstacleCell::Part(back)));
        }
        for (index, &(tile, _)) in cells.iter().enumerate() {
            let repeated = cells[..index].iter().any(|&(earlier, _)| earlier == tile);
            let taken = self
                .tile(tile)
                .is_some_and(|tile| tile.obstacle() != ObstacleCell::Empty);
            if repeated || taken {
                return Err(PlacementError::Blocked(tile));
            }
        }
        for (tile, cell) in cells {
            self.tile_mut(tile).set_obstacle(cell);
        }
        Ok(())
    }

    /// The tile at `tile`, for changing it. Copies its chunk first if anything
    /// else shares it.
    fn tile_mut(&mut self, tile: TilePos) -> &mut Tile {
        let (chunk, local) = self
            .locate(tile)
            .expect("only tiles between the edges are changed");
        let chunks = Arc::make_mut(&mut self.chunks);
        &mut Arc::make_mut(&mut chunks[chunk]).tiles[local]
    }

    /// The index of the chunk holding `tile` and of the tile inside it, or
    /// `None` beyond the north and south edges.
    fn locate(&self, tile: TilePos) -> Option<(usize, usize)> {
        let tile = self.grid.canonical(tile)?;
        let chunk = ChunkPos::of(tile);
        let origin = chunk.origin();
        let chunk_index = chunk.y * self.world_width_chunks + chunk.x;
        let local_index = (tile.y - origin.y) * CHUNK_SIZE + (tile.x - origin.x);
        Some((to_index(chunk_index), to_index(local_index)))
    }
}

fn to_index(value: i32) -> usize {
    usize::try_from(value).expect("canonical tiles give non-negative indices")
}

#[cfg(test)]
mod tests {
    use expanse_core::grid::TileOffset;

    use super::*;
    use crate::field::Fields;
    use crate::tile::BiomeIndex;

    /// A tile that records the position it was filled for.
    fn marked(tile: TilePos) -> Tile {
        Tile::new(
            Fields {
                elevation: u16::try_from(tile.x).expect("test worlds are small"),
                winter_temperature: u16::try_from(tile.y).expect("test worlds are small"),
                summer_temperature: 0,
                moisture: 0,
                vegetation: 0,
            },
            BiomeIndex(0),
            false,
        )
    }

    fn terrain(world_width_chunks: u16, world_height_chunks: u16) -> Terrain {
        Terrain::new(world_width_chunks, world_height_chunks, marked).expect("a terrain with tiles")
    }

    /// Catches tiles stored in one place and looked up in another, such as
    /// swapped rows and columns or a wrong count of chunks per row, and
    /// positions that are not wrapped first. The terrain is wider than high,
    /// so swapping the two shows.
    #[test]
    fn every_tile_maps_to_one_chunk_and_back_to_its_canonical_tile() {
        assert!(Terrain::new(0, 1, marked).is_none());
        assert!(Terrain::new(1, 0, marked).is_none());
        let terrain = terrain(3, 2);
        let grid = terrain.grid();
        assert_eq!(
            (grid.width(), grid.height()),
            (3 * CHUNK_SIZE, 2 * CHUNK_SIZE)
        );
        assert_eq!(Terrain::grid_for(3, 2), Some(grid));
        assert_eq!(Terrain::grid_for(0, 1), None);
        for x in (-2 * grid.width()..3 * grid.width()).step_by(3) {
            for y in -1..=grid.height() {
                let tile = TilePos::new(x, y);
                let Some(canonical) = grid.canonical(tile) else {
                    assert_eq!(terrain.tile(tile), None);
                    assert_eq!(terrain.chunk_containing(tile), None);
                    continue;
                };
                let origin = terrain
                    .chunk_containing(tile)
                    .expect("a tile between the edges")
                    .origin();
                let inside = (canonical.x - origin.x, canonical.y - origin.y);
                assert!(
                    (0..CHUNK_SIZE).contains(&inside.0) && (0..CHUNK_SIZE).contains(&inside.1),
                    "{tile:?} is not inside the chunk at {origin:?}"
                );
                assert_eq!(terrain.tile(tile), Some(&marked(canonical)), "{tile:?}");
            }
        }
    }

    /// Catches footprints that stop at the seam, or that are written on one
    /// side only.
    #[test]
    fn a_footprint_across_the_seam_blocks_the_tiles_on_both_sides() {
        let mut terrain = terrain(2, 1);
        let east_edge = terrain.grid().width() - 1;
        let footprint = Footprint::new([
            TileOffset::new(1, 0),
            TileOffset::new(2, 0),
            TileOffset::new(0, 1),
        ])
        .expect("a valid footprint");
        terrain
            .place_obstacle(TilePos::new(east_edge, 5), ObstacleIndex(3), &footprint)
            .expect("free tiles");
        for blocked in [(east_edge, 5), (0, 5), (1, 5), (east_edge, 6), (-1, 5)] {
            let tile = TilePos::new(blocked.0, blocked.1);
            assert!(terrain.is_blocked(tile), "{tile:?}");
        }
        for free in [(east_edge - 1, 5), (2, 5), (0, 6), (east_edge, 4)] {
            let tile = TilePos::new(free.0, free.1);
            assert!(!terrain.is_blocked(tile), "{tile:?}");
        }
        let anchor_from_part = |tile| match terrain.tile(tile).map(Tile::obstacle) {
            Some(ObstacleCell::Part(back)) => terrain.grid().translate(tile, back.into()),
            other => panic!("{tile:?} holds {other:?}"),
        };
        let anchor = Some(TilePos::new(east_edge, 5));
        assert_eq!(anchor_from_part(TilePos::new(1, 5)), anchor);
        assert_eq!(anchor_from_part(TilePos::new(east_edge, 6)), anchor);
    }

    /// Catches overlapping obstacles, footprints cut off at an edge, and
    /// placements that fail halfway, leaving part of a footprint behind.
    #[test]
    fn a_placement_that_fails_changes_nothing() {
        let mut terrain = terrain(1, 1);
        let bottom = terrain.grid().height() - 1;
        let wide = Footprint::new([TileOffset::new(1, 0), TileOffset::new(2, 0)])
            .expect("a valid footprint");
        let rock = Footprint::single_tile();
        terrain
            .place_obstacle(TilePos::new(6, 4), ObstacleIndex(0), &rock)
            .expect("a free tile");
        let before = terrain.clone();

        assert_eq!(
            terrain.place_obstacle(TilePos::new(4, 4), ObstacleIndex(1), &wide),
            Err(PlacementError::Blocked(TilePos::new(6, 4)))
        );
        let tall = Footprint::new([TileOffset::new(0, 1)]).expect("a valid footprint");
        assert_eq!(
            terrain.place_obstacle(TilePos::new(9, bottom), ObstacleIndex(1), &tall),
            Err(PlacementError::BeyondEdge)
        );
        let around_the_world =
            Footprint::new([TileOffset::new(CHUNK_SIZE, 0)]).expect("a valid footprint");
        assert_eq!(
            terrain.place_obstacle(TilePos::new(12, 4), ObstacleIndex(1), &around_the_world),
            Err(PlacementError::Blocked(TilePos::new(12, 4)))
        );
        assert_eq!(terrain, before);
    }

    /// Catches an impassable biome that does not block movement, or that
    /// keeps an obstacle off its tile, which would make placement depend on
    /// more than footprints.
    #[test]
    fn an_impassable_tile_is_blocked_and_can_still_hold_an_obstacle() {
        let water = TilePos::new(3, 3);
        let mut terrain = Terrain::new(1, 1, |tile| {
            let mut marked = marked(tile);
            marked.impassable = tile == water;
            marked
        })
        .expect("a terrain with tiles");
        assert!(terrain.is_blocked(water));
        assert!(!terrain.is_blocked(TilePos::new(3, 4)));
        terrain
            .place_obstacle(water, ObstacleIndex(0), &Footprint::single_tile())
            .expect("an impassable tile without an obstacle is free");
    }

    /// Catches copies that grow with the terrain, and changes that reach a
    /// copy held elsewhere, such as a snapshot being drawn. Only the chunk
    /// that changed may be copied.
    #[test]
    fn a_change_copies_only_its_chunk_and_never_reaches_a_clone() {
        let mut terrain = terrain(2, 2);
        let snapshot = terrain.clone();
        assert!(Arc::ptr_eq(&terrain.chunks, &snapshot.chunks));

        let tile = TilePos::new(CHUNK_SIZE + 3, 4);
        terrain
            .place_obstacle(tile, ObstacleIndex(0), &Footprint::single_tile())
            .expect("a free tile");
        assert!(!snapshot.is_blocked(tile));
        assert!(terrain.is_blocked(tile));
        let shared: Vec<bool> = terrain
            .chunks
            .iter()
            .zip(snapshot.chunks.iter())
            .map(|(changed, kept)| Arc::ptr_eq(changed, kept))
            .collect();
        assert_eq!(shared, [true, false, true, true]);
    }
}
