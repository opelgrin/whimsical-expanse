//! Positions on the tile grid, and the grid's shape: it wraps east–west.

/// A tile on the world grid. Positions in the simulation are whole tiles.
///
/// A position is not necessarily canonical: `x` may lie outside the grid's
/// columns, and only a [`Grid`] knows which column that is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    /// Creates a tile position.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// The difference between two tile positions, in tiles. South and east are
/// positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileOffset {
    pub dx: i32,
    pub dy: i32,
}

impl TileOffset {
    /// Creates a tile offset.
    #[must_use]
    pub const fn new(dx: i32, dy: i32) -> Self {
        Self { dx, dy }
    }
}

/// The shape of the world grid: `width` columns joined east–west at the seam,
/// and `height` rows between a north and a south edge.
///
/// A tile is canonical when `0 <= x < width` and `0 <= y < height`. Columns
/// repeat every `width` tiles, so `x` and `x + width` name the same column.
/// Rows do not repeat: there is nothing beyond the north and south edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Grid {
    width: i32,
    height: i32,
}

impl Grid {
    /// Creates a grid of `width` by `height` tiles, or `None` if it would have
    /// no tiles.
    #[must_use]
    pub const fn new(width: i32, height: i32) -> Option<Self> {
        if width < 1 || height < 1 {
            None
        } else {
            Some(Self { width, height })
        }
    }

    /// The number of columns.
    #[must_use]
    pub const fn width(self) -> i32 {
        self.width
    }

    /// The number of rows.
    #[must_use]
    pub const fn height(self) -> i32 {
        self.height
    }

    /// The number of tiles.
    #[must_use]
    pub fn tile_count(self) -> usize {
        to_usize(self.width) * to_usize(self.height)
    }

    /// The position of `tile` in row-major order, wrapping across the seam,
    /// or `None` beyond the north and south edges. Indices run from 0 to
    /// [`tile_count`](Self::tile_count), row by row from the north edge.
    #[must_use]
    pub fn index(self, tile: TilePos) -> Option<usize> {
        let tile = self.canonical(tile)?;
        Some(to_usize(tile.y) * to_usize(self.width) + to_usize(tile.x))
    }

    /// The canonical tile at row-major `index`, the inverse of
    /// [`index`](Self::index).
    ///
    /// # Panics
    ///
    /// Panics if `index` is not below [`tile_count`](Self::tile_count).
    #[must_use]
    pub fn tile_at(self, index: usize) -> TilePos {
        assert!(
            index < self.tile_count(),
            "tile index {index} is outside the grid"
        );
        let width = to_usize(self.width);
        let narrow = |value: usize| i32::try_from(value).expect("grid coordinates fit in i32");
        TilePos::new(narrow(index % width), narrow(index / width))
    }

    /// The canonical form of `tile`, or `None` if it lies beyond the north or
    /// south edge.
    #[must_use]
    pub fn canonical(self, tile: TilePos) -> Option<TilePos> {
        self.translate(tile, TileOffset::new(0, 0))
    }

    /// The canonical tile `offset` away from `tile`, wrapping across the seam,
    /// or `None` if it lies beyond the north or south edge.
    #[must_use]
    pub fn translate(self, tile: TilePos, offset: TileOffset) -> Option<TilePos> {
        // Widened so that no input can overflow.
        let y = i64::from(tile.y) + i64::from(offset.dy);
        if !(0..i64::from(self.height)).contains(&y) {
            return None;
        }
        let x = (i64::from(tile.x) + i64::from(offset.dx)).rem_euclid(i64::from(self.width));
        Some(TilePos::new(narrow(x), narrow(y)))
    }

    /// The canonical neighbour of `tile` in `direction`, across the seam if
    /// needed, or `None` beyond the north and south edges.
    #[must_use]
    pub fn neighbour(self, tile: TilePos, direction: Direction) -> Option<TilePos> {
        self.translate(tile, direction.offset())
    }

    /// The shortest offset from `from` to `to`, going across the seam when
    /// that is shorter.
    ///
    /// `dx` lies in `-(width / 2)..width - width / 2`. When the width is even
    /// and the two columns are exactly half a world apart, both ways are
    /// equally short and `dx` is `-(width / 2)` whichever tile comes first.
    /// Everywhere else, swapping the tiles negates the offset.
    ///
    /// # Panics
    ///
    /// Panics if the rows are more than `i32::MAX` apart, which two tiles of
    /// the grid never are.
    #[must_use]
    pub fn offset(self, from: TilePos, to: TilePos) -> TileOffset {
        let width = i64::from(self.width);
        let half = width / 2;
        let dx = (i64::from(to.x) - i64::from(from.x) + half).rem_euclid(width) - half;
        let dy = i32::try_from(i64::from(to.y) - i64::from(from.y))
            .expect("rows of the grid are less than i32::MAX apart");
        TileOffset::new(narrow(dx), dy)
    }

    /// The number of four-way steps between `a` and `b`, across the seam when
    /// that is shorter.
    #[must_use]
    pub fn manhattan_distance(self, a: TilePos, b: TilePos) -> u32 {
        let offset = self.offset(a, b);
        offset.dx.unsigned_abs() + offset.dy.unsigned_abs()
    }
}

/// Converts a coordinate or size already known to be non-negative.
fn to_usize(value: i32) -> usize {
    usize::try_from(value).expect("canonical coordinates and sizes are non-negative")
}

/// Converts a value already reduced to the grid's range back to a coordinate.
fn narrow(value: i64) -> i32 {
    i32::try_from(value).expect("values reduced to the grid's range fit in i32")
}

/// One of the four cardinal directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    /// All four directions, in a fixed order.
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The step of one tile in this direction. North is towards negative `y`.
    #[must_use]
    pub const fn offset(self) -> TileOffset {
        match self {
            Self::North => TileOffset::new(0, -1),
            Self::East => TileOffset::new(1, 0),
            Self::South => TileOffset::new(0, 1),
            Self::West => TileOffset::new(-1, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(width: i32, height: i32) -> Grid {
        Grid::new(width, height).expect("a grid with tiles")
    }

    /// Every tile of every small grid, with `x` also a few worlds to either
    /// side, so that non-canonical positions are covered.
    fn small_grids_and_tiles() -> impl Iterator<Item = (Grid, TilePos)> {
        (1..=6).flat_map(|width| {
            (1..=3).flat_map(move |height| {
                (-2 * width..3 * width).flat_map(move |x| {
                    (0..height).map(move |y| (grid(width, height), TilePos::new(x, y)))
                })
            })
        })
    }

    /// Catches a grid without tiles, whose wrapping would divide by zero, and
    /// canonical forms that do not wrap columns or that invent rows beyond the
    /// edges. The expected values are worked out by hand.
    #[test]
    fn columns_wrap_and_rows_end_at_the_edges() {
        assert_eq!(Grid::new(0, 3), None);
        assert_eq!(Grid::new(5, 0), None);
        let grid = grid(5, 3);
        assert_eq!(
            grid.canonical(TilePos::new(-1, 0)),
            Some(TilePos::new(4, 0))
        );
        assert_eq!(grid.canonical(TilePos::new(5, 2)), Some(TilePos::new(0, 2)));
        assert_eq!(
            grid.canonical(TilePos::new(-12, 1)),
            Some(TilePos::new(3, 1))
        );
        assert_eq!(grid.canonical(TilePos::new(2, -1)), None);
        assert_eq!(grid.canonical(TilePos::new(2, 3)), None);
    }

    /// Catches a wrong or duplicated direction step, and neighbours that stop
    /// at the seam instead of continuing on the other side.
    #[test]
    fn neighbours_continue_across_the_seam_in_both_directions() {
        let grid = grid(5, 3);
        let neighbours = |tile| Direction::ALL.map(|direction| grid.neighbour(tile, direction));
        assert_eq!(
            neighbours(TilePos::new(2, 1)),
            [
                Some(TilePos::new(2, 0)),
                Some(TilePos::new(3, 1)),
                Some(TilePos::new(2, 2)),
                Some(TilePos::new(1, 1)),
            ]
        );
        assert_eq!(
            grid.neighbour(TilePos::new(4, 1), Direction::East),
            Some(TilePos::new(0, 1))
        );
        assert_eq!(
            grid.neighbour(TilePos::new(0, 1), Direction::West),
            Some(TilePos::new(4, 1))
        );
    }

    /// Catches row-major indices that skip or repeat tiles, swap rows and
    /// columns, or do not wrap first.
    #[test]
    fn every_tile_has_one_index_and_back() {
        let grid = grid(5, 3);
        for index in 0..grid.tile_count() {
            assert_eq!(grid.index(grid.tile_at(index)), Some(index));
        }
        assert_eq!(grid.index(TilePos::new(1, 2)), Some(11));
        assert_eq!(grid.index(TilePos::new(-1, 0)), Some(4));
        assert_eq!(grid.index(TilePos::new(0, 3)), None);
    }

    /// Catches neighbours that wrap north–south, or that leave the grid.
    #[test]
    fn no_neighbour_beyond_the_north_and_south_edges() {
        let grid = grid(5, 3);
        assert_eq!(grid.neighbour(TilePos::new(2, 0), Direction::North), None);
        assert_eq!(grid.neighbour(TilePos::new(2, 2), Direction::South), None);
    }

    /// Catches an offset that ignores the seam, takes the long way round, or
    /// does not lead to its target, and a distance that is not the length of
    /// the shortest offset. The reference is worked out independently: of the
    /// two ways round, the shorter one.
    #[test]
    fn the_offset_is_the_shortest_way_to_the_target() {
        for (grid, from) in small_grids_and_tiles() {
            for (_, to) in small_grids_and_tiles().filter(|(other, _)| *other == grid) {
                let offset = grid.offset(from, to);
                assert_eq!(
                    grid.translate(from, offset),
                    grid.canonical(to),
                    "{grid:?} {from:?} {to:?}"
                );
                let eastwards = (to.x - from.x).rem_euclid(grid.width());
                let shortest = eastwards.min(grid.width() - eastwards);
                assert_eq!(offset.dx.abs(), shortest, "{grid:?} {from:?} {to:?}");
                let rows = (to.y - from.y).unsigned_abs();
                assert_eq!(
                    grid.manhattan_distance(from, to),
                    shortest.unsigned_abs() + rows,
                    "{grid:?} {from:?} {to:?}"
                );
            }
        }
    }

    /// Catches a distance that depends on which tile comes first, and an
    /// offset that does not reverse when the tiles are swapped, apart from the
    /// documented tie at exactly half the width.
    #[test]
    fn the_offset_reverses_when_the_tiles_are_swapped() {
        for (grid, a) in small_grids_and_tiles() {
            for (_, b) in small_grids_and_tiles().filter(|(other, _)| *other == grid) {
                assert_eq!(grid.manhattan_distance(a, b), grid.manhattan_distance(b, a));
                let there = grid.offset(a, b);
                let back = grid.offset(b, a);
                assert_eq!(back.dy, -there.dy);
                let tie = grid.width() % 2 == 0 && there.dx.abs() == grid.width() / 2;
                if tie {
                    assert_eq!((there.dx, back.dx), (-grid.width() / 2, -grid.width() / 2));
                } else {
                    assert_eq!(back.dx, -there.dx, "{grid:?} {a:?} {b:?}");
                }
            }
        }
    }
}
