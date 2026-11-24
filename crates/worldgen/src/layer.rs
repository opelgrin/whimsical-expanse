//! Operations on a layer: one unquantised value per tile, in row-major order.

use std::num::NonZero;
use std::thread;

use expanse_core::grid::Grid;

/// Sets each value to `value_at` its index, spreading the work over the
/// available threads.
///
/// Each value depends only on its index, so the result is the same as
/// filling the values one by one, whatever the number of threads.
pub(crate) fn fill_in_parallel(values: &mut [f64], value_at: impl Fn(usize) -> f64 + Sync) {
    let threads = thread::available_parallelism().map_or(1, NonZero::get);
    let part_length = values.len().div_ceil(threads).max(1);
    thread::scope(|scope| {
        for (part_number, part) in values.chunks_mut(part_length).enumerate() {
            let value_at = &value_at;
            scope.spawn(move || {
                let first = part_number * part_length;
                for (offset, value) in part.iter_mut().enumerate() {
                    *value = value_at(first + offset);
                }
            });
        }
    });
}

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
    let mut line = Vec::with_capacity(width.max(height));

    // Each row, wrapping: a running sum over the window of 2r + 1 columns.
    let window = f64::from(u32::try_from(2 * radius + 1).expect("a small radius"));
    for row in values.chunks_exact_mut(width) {
        line.clear();
        line.extend_from_slice(row);
        let mut sum: f64 = (0..=2 * radius)
            .map(|offset| line[(offset + width - radius) % width])
            .sum();
        for (x, value) in row.iter_mut().enumerate() {
            *value = sum / window;
            sum += line[(x + radius + 1) % width] - line[(x + width - radius) % width];
        }
    }

    // Each column, cut off at the edges: sums of the values above each row.
    for x in 0..width {
        line.clear();
        line.push(0.0);
        for y in 0..height {
            let total = line[y] + values[y * width + x];
            line.push(total);
        }
        for y in 0..height {
            let top = y.saturating_sub(radius);
            let bottom = (y + radius + 1).min(height);
            let count = f64::from(u32::try_from(bottom - top).expect("a small window"));
            values[y * width + x] = (line[bottom] - line[top]) / count;
        }
    }
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
        .iter()
        .map(|&index| (values[index], index))
        .collect();
    ordered.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let count = u64::try_from(members.len()).expect("fewer tiles than u64::MAX");
    for (rank, (_, index)) in ordered.into_iter().enumerate() {
        let rank = u64::try_from(rank).expect("fewer tiles than u64::MAX");
        let value = u64::from(low) + rank * u64::from(span) / count;
        out[index] = u16::try_from(value).expect("ranks stay below low + span, within u16");
    }
}

fn to_usize(value: i32) -> usize {
    usize::try_from(value).expect("grid sizes are positive")
}

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

    /// Catches parts of the work written to the wrong place, skipped or
    /// overlapping, which would make the result depend on the number of
    /// threads.
    #[test]
    fn filling_in_parallel_matches_filling_one_by_one() {
        for length in [0, 1, 7, 10_007] {
            let mut values = vec![-1.0; length];
            fill_in_parallel(&mut values, |index| {
                f64::from(u32::try_from(index).expect("small")) * 0.5
            });
            let one_by_one: Vec<f64> = (0..length)
                .map(|index| f64::from(u32::try_from(index).expect("small")) * 0.5)
                .collect();
            assert_eq!(values, one_by_one);
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
