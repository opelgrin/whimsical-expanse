//! The biome passes: classification, then clean-up.

use expanse_core::grid::{Direction, Grid};
use expanse_core::region::Regions;
use expanse_terrain::{BiomeIndex, BiomeTable, Fields};
use rayon::prelude::*;

/// Each tile's biome: the first in the table whose ranges contain its fields.
pub(crate) fn classify(fields: &[Fields], table: &BiomeTable) -> Vec<BiomeIndex> {
    fields
        .par_iter()
        .map(|fields| table.classify(fields))
        .collect()
}

/// Gives each region smaller than `min_tiles` the biome most common along its
/// border, the first in table order on a tie, until no region is smaller.
///
/// A region is a set of tiles of one biome joined by their edges, across the
/// seam too. Within one round, a region is left for the next round if a
/// neighbouring region has just changed, since its border is then out of
/// date. The first small region of each round always changes, and joins a
/// neighbour, so every round leaves fewer regions.
///
/// `min_tiles` is at most the number of tiles, so a small region never covers
/// the whole grid and always has a border.
///
/// Sequential: whether a region changes depends on the changes before it in
/// the round. Finding the regions, most of the work, is parallel.
pub(crate) fn clean_up(
    grid: Grid,
    biomes: &mut [BiomeIndex],
    biome_count: usize,
    min_tiles: usize,
) {
    let mut changed = vec![false; biomes.len()];
    let mut border = Vec::new();
    let mut tally = vec![0_usize; biome_count];
    loop {
        let regions = Regions::find(grid, |index| Some(biomes[index]));
        let mut any_small = false;
        changed.fill(false);
        for region in 0..regions.count() {
            if regions.tiles(region).len() >= min_tiles {
                continue;
            }
            any_small = true;
            border.clear();
            for index in regions.tiles(region) {
                let tile = grid.tile_at(index);
                border.extend(
                    Direction::ALL
                        .into_iter()
                        .filter_map(|direction| grid.neighbour(tile, direction))
                        .filter_map(|neighbour| grid.index(neighbour))
                        .filter(|&neighbour| regions.region_of(neighbour) != Some(region)),
                );
            }
            border.sort_unstable();
            border.dedup();
            if border.iter().any(|&index| changed[index]) {
                continue;
            }
            tally.fill(0);
            for &index in &border {
                tally[usize::from(biomes[index].0)] += 1;
            }
            let most_common = most_common(&tally);
            for index in regions.tiles(region) {
                biomes[index] = most_common;
                changed[index] = true;
            }
        }
        if !any_small {
            return;
        }
    }
}

/// The biome with the highest count, the first on a tie.
fn most_common(tally: &[usize]) -> BiomeIndex {
    let (position, _) = tally
        .iter()
        .enumerate()
        .rev()
        .max_by_key(|&(_, &count)| count)
        .expect("a table has at least one biome");
    BiomeIndex(u16::try_from(position).expect("a table has fewer than 2^16 biomes"))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Biomes drawn as text, one digit per tile.
    fn biomes(rows: &[&str]) -> (Grid, Vec<BiomeIndex>) {
        let width = i32::try_from(rows[0].len()).expect("a small grid");
        let height = i32::try_from(rows.len()).expect("a small grid");
        let grid = Grid::new(width, height).expect("a grid with tiles");
        let biomes = rows
            .iter()
            .flat_map(|row| row.bytes())
            .map(|digit| BiomeIndex(u16::from(digit - b'0')))
            .collect();
        (grid, biomes)
    }

    /// Catches a region that takes the wrong biome: not the most common
    /// along its border, not the first on a tie, or counted through the
    /// seam wrongly. The expected biomes are worked out by hand.
    #[test]
    fn a_small_region_takes_the_most_common_biome_along_its_border() {
        // The 1 at the seam touches 0 three times (west across the seam,
        // north, south) and 2 once.
        let (grid, mut tiles) = biomes(&[
            "000220", //
            "122220", //
            "000220",
        ]);
        clean_up(grid, &mut tiles, 3, 2);
        assert_eq!(tiles, biomes(&["000220", "022220", "000220"]).1);

        // A tie between 1 and 2: the first in table order wins.
        let (grid, mut tiles) = biomes(&["11022"]);
        clean_up(grid, &mut tiles, 3, 2);
        assert_eq!(tiles, biomes(&["11122"]).1);
    }
}
