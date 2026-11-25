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

use rayon::prelude::*;

use crate::grid::Grid;

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
    /// `key` is called once per tile, in parallel and in no particular order.
    /// For each tile at row-major `index`:
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
    pub fn find<K: PartialEq + Send + Sync>(
        grid: Grid,
        key: impl Fn(usize) -> Option<K> + Sync,
    ) -> Self {
        let count = grid.tile_count();
        assert!(
            u32::try_from(count).is_ok_and(|count| count < NO_REGION),
            "grids hold fewer than u32::MAX tiles"
        );
        let width = usize::try_from(grid.width()).expect("grid sizes are positive");
        let height = usize::try_from(grid.height()).expect("grid sizes are positive");

        // Run-based connected-component labelling. A run is a maximal stretch
        // of tiles in one row that share a key (see `Run`). All tiles of a run
        // are in the same region, so the work is done on runs, which are far
        // fewer than tiles.
        //
        // 1. Split each row into runs. Rows are independent, so this is
        //    parallel.
        // 2. Link the runs that touch and share a key: the two ends of a row
        //    across the seam, and runs in adjacent rows that share a column.
        //    Each pair of adjacent rows is independent, so this is parallel.
        // 3. Join the linked runs with union-find and number the regions.
        //    This is sequential, but it works on runs and links, not tiles.
        // 4. Give each tile the region of its run, in parallel by row, and
        //    list the tiles of each region.
        //
        // Union-find keeps the lowest run of each set as its root, so regions
        // are numbered in the order of their first tile. A row-major flood
        // fill gives the same numbering.
        let rows: Vec<Vec<Run<K>>> = (0..height)
            .into_par_iter()
            .map(|y| runs_of_row(y * width, width, &key))
            .collect();
        let mut row_starts = Vec::with_capacity(height + 1);
        row_starts.push(0);
        for row in &rows {
            row_starts.push(row_starts[row_starts.len() - 1] + row.len());
        }
        let runs: Vec<&Run<K>> = rows.iter().flatten().collect();
        let region_of_run = number_runs(&runs, links(&rows, &row_starts, width));

        let mut region_of = vec![NO_REGION; count];
        region_of
            .par_chunks_mut(width)
            .zip(&rows)
            .enumerate()
            .for_each(|(y, (tiles, row))| {
                let row_start = y * width;
                for (run, region) in row.iter().zip(&region_of_run[row_starts[y]..]) {
                    tiles[run.start - row_start..run.end - row_start].fill(*region);
                }
            });
        let (members, starts) = members(&runs, &region_of_run);
        Self {
            region_of,
            members,
            starts,
        }
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

    /// The row-major indices of all tiles in `region`, in increasing order.
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

/// The pairs of runs that touch and share a key, and so are in the same
/// region. A pair is either the first and last runs of a row, which meet
/// across the seam, or two runs in adjacent rows that share a column.
///
/// Runs are numbered among all runs in row-major order. `row_starts` holds
/// the number of each row's first run.
fn links<K: PartialEq + Sync>(
    rows: &[Vec<Run<K>>],
    row_starts: &[usize],
    width: usize,
) -> Vec<(usize, usize)> {
    (0..rows.len())
        .into_par_iter()
        .flat_map_iter(|y| {
            let first = row_starts[y];
            let seam = seam_link(&rows[y], width).map(|(west, east)| (first + west, first + east));
            let below = rows.get(y + 1).map_or_else(Vec::new, |next| {
                downward_links(&rows[y], next, width)
                    .into_iter()
                    .map(|(above, below)| (first + above, row_starts[y + 1] + below))
                    .collect()
            });
            seam.into_iter().chain(below)
        })
        .collect()
}

/// The region of each run, given the links between runs. Regions are
/// numbered in the order of their first run.
///
/// Sequential: there are far fewer runs and links than tiles.
fn number_runs<K>(runs: &[&Run<K>], links: Vec<(usize, usize)>) -> Vec<u32> {
    let mut roots = Roots::new(runs.len());
    for (a, b) in links {
        roots.join(a, b);
    }
    let mut region_of_run = vec![NO_REGION; runs.len()];
    let mut count = 0;
    for run in 0..runs.len() {
        // A root is the first run of its region, so it is numbered before
        // the rest of the region.
        let root = roots.root(run);
        region_of_run[run] = if root == run {
            count += 1;
            count - 1
        } else {
            region_of_run[root]
        };
    }
    region_of_run
}

/// The tiles of every region, region after region, and where each region's
/// tiles start, followed by the total.
///
/// Sequential: the runs of one region come from many rows, so the rows cannot
/// fill `members` independently. The pass copies one range of tiles per run.
fn members<K>(runs: &[&Run<K>], region_of_run: &[u32]) -> (Vec<u32>, Vec<usize>) {
    let count = region_of_run
        .iter()
        .max()
        .map_or(0, |&last| to_usize(last) + 1);
    let mut starts = vec![0; count + 1];
    for (run, &region) in runs.iter().zip(region_of_run) {
        starts[to_usize(region) + 1] += run.end - run.start;
    }
    for region in 1..=count {
        starts[region] += starts[region - 1];
    }
    let mut next = starts.clone();
    let mut members = vec![0; starts[starts.len() - 1]];
    for (run, &region) in runs.iter().zip(region_of_run) {
        let next = &mut next[to_usize(region)];
        for (slot, tile) in members[*next..].iter_mut().zip(run.start..run.end) {
            *slot = to_u32(tile);
        }
        *next += run.end - run.start;
    }
    (members, starts)
}

/// A maximal stretch of tiles in one row that share a key: row-major indices
/// `start..end`. A run never wraps across the seam. When tiles with the same
/// key cross the seam, they form two runs, the first and the last of the row,
/// and [`seam_link`] links them.
struct Run<K> {
    start: usize,
    end: usize,
    key: K,
}

/// The runs of the row of `width` tiles that starts at `row_start`, from west
/// to east. Tiles without a key belong to no run.
fn runs_of_row<K: PartialEq>(
    row_start: usize,
    width: usize,
    key: &impl Fn(usize) -> Option<K>,
) -> Vec<Run<K>> {
    let mut runs: Vec<Run<K>> = Vec::new();
    for index in row_start..row_start + width {
        let Some(tile_key) = key(index) else {
            continue;
        };
        match runs.last_mut() {
            Some(run) if run.end == index && run.key == tile_key => run.end += 1,
            _ => runs.push(Run {
                start: index,
                end: index + 1,
                key: tile_key,
            }),
        }
    }
    runs
}

/// The first and last runs of `row`, by position in it, if they meet across
/// the seam with the same key.
fn seam_link<K: PartialEq>(row: &[Run<K>], width: usize) -> Option<(usize, usize)> {
    let (first, last) = (row.first()?, row.last()?);
    let row_start = first.start - first.start % width;
    let meet = row.len() > 1 && first.start == row_start && last.end == row_start + width;
    (meet && first.key == last.key).then_some((0, row.len() - 1))
}

/// The pairs of runs, by position in `above` and in `below`, that share a
/// column and a key.
fn downward_links<K: PartialEq>(
    above: &[Run<K>],
    below: &[Run<K>],
    width: usize,
) -> Vec<(usize, usize)> {
    let column = |index: usize| index % width;
    let end_column = |run: &Run<K>| column(run.end - 1) + 1;
    let mut links = Vec::new();
    let (mut a, mut b) = (0, 0);
    while a < above.len() && b < below.len() {
        let (upper, lower) = (&above[a], &below[b]);
        let overlap =
            column(upper.start) < end_column(lower) && column(lower.start) < end_column(upper);
        if overlap && upper.key == lower.key {
            links.push((a, b));
        }
        // Move past the run that ends first; it overlaps nothing further.
        match end_column(upper).cmp(&end_column(lower)) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => {
                a += 1;
                b += 1;
            }
        }
    }
    links
}

/// Union-find over runs, where the root of a set is its lowest run.
struct Roots {
    parent: Vec<usize>,
}

impl Roots {
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
        }
    }

    /// The root of `run`'s set, halving the path on the way.
    fn root(&mut self, mut run: usize) -> usize {
        while self.parent[run] != run {
            self.parent[run] = self.parent[self.parent[run]];
            run = self.parent[run];
        }
        run
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.parent[a.max(b)] = a.min(b);
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
    use crate::grid::{Direction, TilePos};
    use crate::rng::Rng;

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

    /// The regions by a plain flood fill from each tile in row-major order:
    /// the region of each tile, numbered as `Regions` numbers them.
    fn flood_fill(grid: Grid, keys: &[Option<u8>]) -> Vec<Option<usize>> {
        let mut region_of = vec![None; keys.len()];
        let mut count = 0;
        for first in 0..keys.len() {
            if region_of[first].is_some() || keys[first].is_none() {
                continue;
            }
            region_of[first] = Some(count);
            let mut stack = vec![first];
            while let Some(index) = stack.pop() {
                for direction in Direction::ALL {
                    let Some(neighbour) = grid
                        .neighbour(grid.tile_at(index), direction)
                        .and_then(|tile| grid.index(tile))
                    else {
                        continue;
                    };
                    if region_of[neighbour].is_none() && keys[neighbour] == keys[first] {
                        region_of[neighbour] = Some(count);
                        stack.push(neighbour);
                    }
                }
            }
            count += 1;
        }
        region_of
    }

    /// Catches runs that are joined when they do not touch, or not joined
    /// when they do: across the seam, between rows, at the north and south
    /// edges, and on grids one or two tiles wide. Random keys over many
    /// sizes, checked against a plain flood fill.
    #[test]
    fn regions_match_a_flood_fill_on_random_grids() {
        for seed in 0..200 {
            let mut rng = Rng::from_stream(seed, &[]);
            let width = 1 + rng.index(9);
            let height = 1 + rng.index(9);
            let grid = Grid::new(
                i32::try_from(width).expect("small"),
                i32::try_from(height).expect("small"),
            )
            .expect("a grid with tiles");
            let kinds = 1 + rng.below(3);
            let keys: Vec<Option<u8>> = (0..grid.tile_count())
                .map(|_| u8::try_from(rng.below(kinds + 1)).expect("small"))
                .map(|key| (key > 0).then_some(key))
                .collect();
            let regions = Regions::find(grid, |index| keys[index]);
            let expected = flood_fill(grid, &keys);
            let found: Vec<Option<usize>> = (0..keys.len())
                .map(|index| regions.region_of(index))
                .collect();
            assert_eq!(found, expected, "seed {seed}, {width} × {height}");
            for region in 0..regions.count() {
                let tiles: Vec<usize> = regions.tiles(region).collect();
                let in_region: Vec<usize> = (0..keys.len())
                    .filter(|&index| expected[index] == Some(region))
                    .collect();
                assert_eq!(tiles, in_region, "seed {seed}, region {region}");
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
