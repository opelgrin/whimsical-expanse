//! What every pass knows about the space being generated: its grid, its
//! stream of randomness and where its tiles lie.

use expanse_core::grid::Grid;
use expanse_core::rng::stream_key;
use expanse_terrain::{Latitude, Terrain};

use crate::noise::circle_point;
use crate::params::SurfaceParams;

/// The space being generated.
pub(crate) struct Space {
    grid: Grid,
    /// The space's stream of randomness, from which each pass derives its own.
    seed: u64,
    /// The latitude of each row, from the north edge.
    row_latitudes: Vec<Latitude>,
    /// Each column's point on the circle around the cylinder.
    circle: Vec<(f64, f64)>,
}

impl Space {
    /// The surface described by `params`, which have been checked, in the
    /// game whose seed is `seed`.
    pub(crate) fn surface(seed: u64, params: &SurfaceParams) -> Self {
        let grid = Terrain::grid_for(params.width_chunks, params.height_chunks)
            .expect("checked parameters give a grid with tiles");
        let height = f64::from(grid.height());
        let north = params.north_latitude_degrees;
        let south = params.south_latitude_degrees;
        let row_latitudes = (0..grid.height())
            .map(|y| {
                let degrees = north + (south - north) * (f64::from(y) + 0.5) / height;
                Latitude::from_degrees(degrees).expect("between two valid latitudes")
            })
            .collect();
        let width = f64::from(grid.width());
        let circle = (0..grid.width())
            .map(|x| circle_point(f64::from(x) + 0.5, width))
            .collect();
        Self {
            grid,
            seed: stream_key(seed, &[stream::SURFACE]),
            row_latitudes,
            circle,
        }
    }

    pub(crate) const fn grid(&self) -> Grid {
        self.grid
    }

    /// The seed of the pass stream identified by `keys`, the first of which
    /// is a label from [`stream`].
    pub(crate) fn stream(&self, keys: &[u64]) -> u64 {
        stream_key(self.seed, keys)
    }

    /// The latitude of the tile at row-major `index`.
    pub(crate) fn latitude(&self, index: usize) -> Latitude {
        let row = usize::try_from(self.grid.tile_at(index).y).expect("a canonical row");
        self.row_latitudes[row]
    }

    pub(crate) fn into_row_latitudes(self) -> Vec<Latitude> {
        self.row_latitudes
    }

    /// The centre of the tile at row-major `index` on the cylinder: its point
    /// on the circle, and the distance along the cylinder, in tiles.
    pub(crate) fn cylinder_point(&self, index: usize) -> (f64, f64, f64) {
        let tile = self.grid.tile_at(index);
        let column = usize::try_from(tile.x).expect("a canonical column");
        let (x, y) = self.circle[column];
        (x, y, f64::from(tile.y) + 0.5)
    }
}

/// Labels of the streams of randomness, so that each use gets its own. They
/// are part of what a seed means: changing one changes every world.
pub(crate) mod stream {
    /// The surface, among the spaces of a game.
    pub const SURFACE: u64 = 1;
    pub const ELEVATION: u64 = 1;
    pub const TEMPERATURE: u64 = 2;
    pub const MOISTURE: u64 = 3;
    pub const VEGETATION: u64 = 4;
    /// The draw that decides whether a tile anchors an obstacle.
    pub const OBSTACLE: u64 = 5;
    /// The variation of an obstacle kind's density in a biome.
    pub const OBSTACLE_VARIATION: u64 = 6;
}
