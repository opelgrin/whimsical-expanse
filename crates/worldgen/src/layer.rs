//! Operations on a layer: one unquantised value per tile, in row-major order.

use expanse_core::grid::Grid;
use rayon::prelude::*;

/// Replaces each value by the average over the square of `radius` tiles
/// around it, wrapping across the seam. Near the north and south edges, the
/// square is cut off and the average is over the tiles that remain.
///
/// `radius` is less than half the grid's width.
pub(crate) fn smooth(grid: Grid, values: &mut [f64], radius: u16) {
    if radius == 0 {
        return;
    }
    let width = to_usize(grid.width());
    let height = to_usize(grid.height());
    let radius = usize::from(radius);
    debug_assert!(2 * radius < width);

    // Each row, wrapping: a running sum over the window of 2r + 1 columns.
    let window = f64::from(u32::try_from(2 * radius + 1).expect("a small radius"));
    values.par_chunks_exact_mut(width).for_each_init(
        || Vec::with_capacity(width),
        |line, row| {
            line.clear();
            line.extend_from_slice(row);
            let mut sum: f64 = (0..=2 * radius)
                .map(|offset| line[(offset + width - radius) % width])
                .sum();
            for (x, value) in row.iter_mut().enumerate() {
                *value = sum / window;
                sum += line[(x + radius + 1) % width] - line[(x + width - radius) % width];
            }
        },
    );

    // Each column, cut off at the edges: sums of the values above each row.
    // The columns are computed apart, then written back row by row, since a
    // column is not a contiguous part of `values`.
    let columns: Vec<Vec<f64>> = (0..width)
        .into_par_iter()
        .map(|x| {
            let mut above = Vec::with_capacity(height + 1);
            above.push(0.0);
            for y in 0..height {
                let total = above[y] + values[y * width + x];
                above.push(total);
            }
            (0..height)
                .map(|y| {
                    let top = y.saturating_sub(radius);
                    let bottom = (y + radius + 1).min(height);
                    let count = f64::from(u32::try_from(bottom - top).expect("a small window"));
                    (above[bottom] - above[top]) / count
                })
                .collect()
        })
        .collect();
    values
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            for (value, column) in row.iter_mut().zip(&columns) {
                *value = column[y];
            }
        });
}

/// Sets `out` at each tile of `members` to its rank among them by `values`,
/// spread evenly from `low` up to, but excluding, `low + span`: the share of
/// members below a value is that value's share of the span.
///
/// Ties go to the tile that comes first in row-major order, so the ranks are
/// all different and depend only on the values.
pub(crate) fn rank(values: &[f64], members: &[usize], low: u32, span: u32, out: &mut [u16]) {
    // Sorting the values with their indices keeps the comparisons in cache.
    let mut ordered: Vec<(f64, usize)> = members
        .par_iter()
        .map(|&index| (values[index], index))
        .collect();
    ordered.par_sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let count = u64::try_from(members.len()).expect("fewer tiles than u64::MAX");
    // Sequential: the members are scattered over `out`, so the writes cannot
    // be split into separate parts of it. Each is cheap.
    for (rank, (_, index)) in ordered.into_iter().enumerate() {
        let rank = u64::try_from(rank).expect("fewer tiles than u64::MAX");
        let value = u64::from(low) + rank * u64::from(span) / count;
        out[index] = u16::try_from(value).expect("ranks stay below low + span, within u16");
    }
}

fn to_usize(value: i32) -> usize {
    usize::try_from(value).expect("grid sizes are positive")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a window off by one, a seam that is not wrapped, and an
    /// average near the edges that divides by tiles that do not exist. Values
    /// worked out by hand.
    #[test]
    fn smoothing_averages_a_wrapping_window_cut_off_at_the_edges() {
        let grid = Grid::new(5, 3).expect("a grid with tiles");
        #[rustfmt::skip]
        let mut values = vec![
            5.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        smooth(grid, &mut values, 1);
        // Across: the 5 spreads to columns 4, 0 and 1, as 5/3 each. Down: rows
        // 0 and 1 average two rows and one, giving 5/6; row 2 has no share.
        let five_sixths = 5.0 / 6.0;
        #[rustfmt::skip]
        let expected = [
            five_sixths, five_sixths, 0.0, 0.0, five_sixths,
            5.0 / 9.0, 5.0 / 9.0, 0.0, 0.0, 5.0 / 9.0,
            0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        for (index, (value, expected)) in values.iter().zip(expected).enumerate() {
            assert!((value - expected).abs() < 1e-12, "tile {index}: {value}");
        }
    }

    /// Catches ranks that are not spread over the whole span, that follow
    /// the members' order instead of their values, or that collide on ties.
    #[test]
    fn ranks_spread_evenly_by_value_and_break_ties_by_position() {
        let values = [0.3, 0.1, 0.3, 0.9, -2.0];
        let mut out = [0; 5];
        rank(&values, &[0, 1, 2, 3], 100, 400, &mut out);
        assert_eq!(out, [200, 100, 300, 400, 0]);
    }
}
