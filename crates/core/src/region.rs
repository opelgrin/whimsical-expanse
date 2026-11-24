//! Connected components of tiles that share an arbitrary key value.
//!
//! A **region** is a maximal connected group of tiles that all share the same
//! key. Two tiles are connected if they are edge-adjacent (including across the
//! grid seam). A tile without a key belongs to no region.
//!
//! The **key** is an arbitrary value chosen by the caller — it could represent
//! terrain type, color, elevation band, ownership, or any property that
//! distinguishes groups of tiles. The only requirement is that it supports
//! equality comparison (`K: PartialEq`).
//!
//! # Example
//!
//! ```ignore
//! // Group tiles by terrain type.
//! enum Terrain { Grass, Water, Mountain }
//! let regions = Regions::find(grid, |index| {
//!     let terrain = get_terrain(index);
//!     Some(terrain) // or None for impassable void tiles
//! });
//! ```

use crate::grid::{Direction, Grid};

/// The connected components of a grid, computed from per-tile keys.
///
/// Each region is a maximal set of edge-adjacent tiles (across the seam too)
/// that share the same key. Tiles without a key are excluded.
///
/// Regions are numbered 0, 1, 2, … in the order of their first tile's
/// row-major index, so the numbering is deterministic and depends only on
/// the keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regions {
    /// The region ID of each tile, by row-major index, or [`NO_REGION`]
    /// for tiles without a key.
    region_of: Vec<u32>,
    /// All region tiles packed sequentially: region 0's tiles, then region 1's,
    /// and so on.
    members: Vec<u32>,
    /// Boundaries into `members`: `starts[i]..starts[i+1]` is the range of
    /// region `i`'s tiles, with a trailing `starts[count]` equal to `members.len()`.
    starts: Vec<usize>,
}

const NO_REGION: u32 = u32::MAX;

impl Regions {
    /// Computes the regions of `grid` from per-tile keys.
    ///
    /// `key` is called once per tile (in row-major order). For each tile at
    /// row-major `index`:
    /// - `Some(k)` means the tile belongs to the region identified by key `k`.
    /// - `None` means the tile is excluded from all regions (e.g. void or
    ///   impassable).
    ///
    /// Two tiles are in the same region if and only if they share a key and
    /// are connected by a path of tiles with that same key. Connectivity is
    /// 4-directional (edge-adjacent), including across the grid seam.
    ///
    /// # Panics
    ///
    /// Panics if the grid has `u32::MAX` tiles or more.
    pub fn find<K: PartialEq>(grid: Grid, key: impl Fn(usize) -> Option<K>) -> Self {
        let count = grid.tile_count();
        assert!(
            u32::try_from(count).is_ok_and(|count| count < NO_REGION),
            "grids hold fewer than u32::MAX tiles"
        );
        let mut regions = Self {
            region_of: vec![NO_REGION; count],
            members: Vec::with_capacity(count),
            starts: vec![0],
        };
        for first in 0..count {
            if regions.region_of[first] != NO_REGION {
                continue;
            }
            let Some(first_key) = key(first) else {
                continue;
            };
            let region = to_u32(regions.starts.len() - 1);
            regions.region_of[first] = region;
            regions.members.push(to_u32(first));
            // The members of this region double as the queue of tiles to visit.
            let mut next = regions.starts[regions.starts.len() - 1];
            while let Some(&tile) = regions.members.get(next) {
                next += 1;
                let tile = grid.tile_at(to_usize(tile));
                for direction in Direction::ALL {
                    let Some(neighbour) = grid
                        .neighbour(tile, direction)
                        .and_then(|neighbour| grid.index(neighbour))
                    else {
                        continue;
                    };
                    if regions.region_of[neighbour] == NO_REGION
                        && key(neighbour).is_some_and(|other| other == first_key)
                    {
                        regions.region_of[neighbour] = region;
                        regions.members.push(to_u32(neighbour));
                    }
                }
            }
            regions.starts.push(regions.members.len());
        }
        regions
    }

    /// The number of regions (connected components).
    #[must_use]
    pub fn count(&self) -> usize {
        self.starts.len() - 1
    }

    /// The region ID of the tile at row-major `index`, or `None` if the tile
    /// has no key.
    #[must_use]
    pub fn region_of(&self, index: usize) -> Option<usize> {
        let region = self.region_of[index];
        (region != NO_REGION).then(|| to_usize(region))
    }

    /// The row-major indices of all tiles in `region`, in the order they were
    /// discovered during region-finding.
    pub fn tiles(&self, region: usize) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.members[self.starts[region]..self.starts[region + 1]]
            .iter()
            .map(|&tile| to_usize(tile))
    }

    /// The region with the most tiles, the first (lowest-numbered) on a tie,
    /// or `None` if there are no regions.
    #[must_use]
    pub fn largest(&self) -> Option<usize> {
        (0..self.count())
            .rev()
            .max_by_key(|&region| self.tiles(region).len())
    }
}

fn to_u32(index: usize) -> u32 {
    u32::try_from(index).expect("the grid holds fewer than u32::MAX tiles")
}

fn to_usize(index: u32) -> usize {
    usize::try_from(index).expect("supported targets have at least 32-bit usize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::TilePos;

    /// Regions of a grid drawn as text: one character per tile, `.` for a
    /// tile without a key.
    fn find(rows: &[&str]) -> (Grid, Regions) {
        let width = i32::try_from(rows[0].len()).expect("a small grid");
        let height = i32::try_from(rows.len()).expect("a small grid");
        let grid = Grid::new(width, height).expect("a grid with tiles");
        let keys: Vec<u8> = rows.iter().flat_map(|row| row.bytes()).collect();
        let regions = Regions::find(grid, |index| Some(keys[index]).filter(|&key| key != b'.'));
        (grid, regions)
    }

    fn region_at(grid: Grid, regions: &Regions, x: i32, y: i32) -> Option<usize> {
        regions.region_of(grid.index(TilePos::new(x, y)).expect("a tile of the grid"))
    }

    /// Catches regions that stop at the seam, that join tiles by their corners
    /// or across different keys, or that include tiles without a key. The
    /// expected regions are worked out by hand.
    #[test]
    fn regions_join_equal_keys_by_their_edges_across_the_seam() {
        let (grid, regions) = find(&[
            "aab.a", //
            "b.bba", //
            "abbb.",
        ]);
        let a_west = region_at(grid, &regions, 0, 0);
        assert_eq!(a_west, region_at(grid, &regions, 1, 0));
        assert_eq!(a_west, region_at(grid, &regions, 4, 0), "across the seam");
        assert_eq!(a_west, region_at(grid, &regions, 4, 1));
        assert_ne!(
            a_west,
            region_at(grid, &regions, 0, 2),
            "only a corner is shared"
        );
        let b_east = region_at(grid, &regions, 2, 0);
        assert_eq!(b_east, region_at(grid, &regions, 1, 2));
        assert_ne!(b_east, region_at(grid, &regions, 0, 1));
        assert_eq!(region_at(grid, &regions, 1, 1), None);
        assert_eq!(regions.count(), 4);
        let sizes: Vec<usize> = (0..regions.count())
            .map(|region| regions.tiles(region).len())
            .collect();
        assert_eq!(sizes, [4, 6, 1, 1]);
        for region in 0..regions.count() {
            for tile in regions.tiles(region) {
                assert_eq!(regions.region_of(tile), Some(region));
            }
        }
    }

    /// Catches a largest region chosen by something other than its size, and
    /// a tie that depends on anything but the numbering.
    #[test]
    fn the_largest_region_is_the_first_of_the_biggest() {
        let (_, regions) = find(&["ab.cc.dd"]);
        assert_eq!(regions.largest(), Some(2));
        let (_, empty) = find(&["..."]);
        assert_eq!(empty.largest(), None);
    }
}
