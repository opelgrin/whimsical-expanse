//! Positions on the tile grid.

/// A tile on the world grid. Positions in the simulation are whole tiles.
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

    /// The neighbouring tile in `direction`, or `None` if the coordinate would
    /// overflow. North is towards negative `y`.
    #[must_use]
    pub const fn neighbour(self, direction: Direction) -> Option<Self> {
        let (dx, dy) = direction.offset();
        match (self.x.checked_add(dx), self.y.checked_add(dy)) {
            (Some(x), Some(y)) => Some(Self { x, y }),
            _ => None,
        }
    }
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

    /// The `(x, y)` step of one tile in this direction.
    const fn offset(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a wrong or duplicated offset, which would leave some direction
    /// unreachable while every move still looked valid.
    #[test]
    fn the_four_directions_reach_the_four_adjacent_tiles() {
        let here = TilePos::new(10, 20);
        let neighbours = Direction::ALL.map(|direction| here.neighbour(direction));
        assert_eq!(
            neighbours,
            [
                Some(TilePos::new(10, 19)),
                Some(TilePos::new(11, 20)),
                Some(TilePos::new(10, 21)),
                Some(TilePos::new(9, 20)),
            ]
        );
    }

    /// Catches arithmetic that wraps or panics at the edge of the coordinate
    /// range instead of reporting that there is no neighbour.
    #[test]
    fn no_neighbour_beyond_the_coordinate_range() {
        assert_eq!(TilePos::new(i32::MAX, 0).neighbour(Direction::East), None);
        assert_eq!(TilePos::new(0, i32::MIN).neighbour(Direction::North), None);
    }
}
