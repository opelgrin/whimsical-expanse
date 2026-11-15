//! A placeholder entity that wanders the grid, one tile at a time.
//!
//! It exists to exercise the tick loop, snapshots and determinism before real
//! actors arrive.

use expanse_core::grid::{Direction, TilePos};
use expanse_core::rng::Rng;
use expanse_core::time::Tick;
use expanse_terrain::Terrain;

use crate::snapshot::Motion;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Wanderer {
    motion: Motion,
    rng: Rng,
}

impl Wanderer {
    /// Starts at `tile` at tick `now`, already on its way to a first neighbour.
    pub fn new(tile: TilePos, now: Tick, rng: Rng, terrain: &Terrain, ticks_per_step: u64) -> Self {
        let mut wanderer = Self {
            motion: Motion {
                from: tile,
                to: tile,
                start: now,
                end: now,
            },
            rng,
        };
        wanderer.start_next_step(terrain, ticks_per_step);
        wanderer
    }

    pub const fn motion(&self) -> Motion {
        self.motion
    }

    /// Brings the wanderer up to `now`, however many ticks have passed since
    /// the last update.
    ///
    /// Each step starts exactly when the previous one ended, not when the
    /// update happens. Updating rarely therefore gives the same result as
    /// updating every tick.
    pub fn update(&mut self, now: Tick, terrain: &Terrain, ticks_per_step: u64) {
        while self.motion.end <= now {
            self.start_next_step(terrain, ticks_per_step);
        }
    }

    /// Starts a step to a random unblocked neighbouring tile, across the seam
    /// if needed, beginning when the current motion ends. Waits in place if
    /// every neighbour is blocked.
    fn start_next_step(&mut self, terrain: &Terrain, ticks_per_step: u64) {
        let grid = terrain.grid();
        let here = self.motion.to;
        let mut options = [here; Direction::ALL.len()];
        let mut count = 0;
        for direction in Direction::ALL {
            if let Some(next) = grid
                .neighbour(here, direction)
                .filter(|&tile| !terrain.is_blocked(tile))
            {
                options[count] = next;
                count += 1;
            }
        }
        let to = if count == 0 {
            here
        } else {
            options[self.rng.index(count)]
        };
        let start = self.motion.end;
        self.motion = Motion {
            from: here,
            to,
            start,
            end: start.after(ticks_per_step),
        };
    }
}

#[cfg(test)]
mod tests {
    use expanse_terrain::{BiomeIndex, Fields, Footprint, ObstacleIndex, Tile};

    use super::*;

    fn open_terrain() -> Terrain {
        let plain = Tile::new(
            Fields {
                elevation: 0,
                temperature: 0,
                moisture: 0,
            },
            BiomeIndex(0),
        );
        Terrain::new(1, 1, |_| plain).expect("a terrain with tiles")
    }

    /// A terrain where every tile is blocked except `open`.
    fn walled_except(open: &[TilePos]) -> Terrain {
        let mut terrain = open_terrain();
        let grid = terrain.grid();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let tile = TilePos::new(x, y);
                if !open.contains(&tile) {
                    terrain
                        .place_obstacle(tile, ObstacleIndex(0), &Footprint::single_tile())
                        .expect("a free tile");
                }
            }
        }
        terrain
    }

    /// A terrain with scattered single-tile obstacles, `start` left free.
    fn scattered(start: TilePos) -> Terrain {
        let mut terrain = open_terrain();
        let grid = terrain.grid();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let tile = TilePos::new(x, y);
                if (x * 7 + y * 3) % 5 == 0 && tile != start {
                    terrain
                        .place_obstacle(tile, ObstacleIndex(0), &Footprint::single_tile())
                        .expect("a free tile");
                }
            }
        }
        terrain
    }

    fn wanderer(seed: u64, start: TilePos, terrain: &Terrain, ticks_per_step: u64) -> Wanderer {
        Wanderer::new(
            start,
            Tick::ZERO,
            Rng::from_stream(seed, &[]),
            terrain,
            ticks_per_step,
        )
    }

    /// The variable-elapsed-time invariant: catches steps that start at the
    /// update tick instead of when the previous step ended, or that skip or
    /// repeat steps when updates are infrequent.
    #[test]
    fn updating_rarely_gives_the_same_result_as_updating_every_tick() {
        let ticks_per_step = 3;
        let terrain = open_terrain();
        let start = TilePos::new(2, 1);
        for seed in 0..20 {
            let mut every_tick = wanderer(seed, start, &terrain, ticks_per_step);
            let mut rarely = wanderer(seed, start, &terrain, ticks_per_step);
            for count in 1..=200 {
                every_tick.update(Tick::new(count), &terrain, ticks_per_step);
                if count % 37 == 0 || count == 200 {
                    rarely.update(Tick::new(count), &terrain, ticks_per_step);
                }
            }
            assert_eq!(every_tick, rarely, "seed {seed}");
        }
    }

    /// Catches broken movement: jumps of more than one tile, positions that
    /// are not canonical, steps onto obstacles, gaps or overlaps in time, or
    /// steps of the wrong duration. The wanderer starts at the seam, so it
    /// crosses it in both directions.
    #[test]
    fn steps_are_continuous_single_tile_moves_onto_free_tiles() {
        let ticks_per_step = 4;
        let start = TilePos::new(0, 16);
        let terrain = scattered(start);
        let grid = terrain.grid();
        for seed in 0..20 {
            let mut wanderer = wanderer(seed, start, &terrain, ticks_per_step);
            let mut previous = wanderer.motion();
            for count in 1..=400 {
                wanderer.update(Tick::new(count), &terrain, ticks_per_step);
                let motion = wanderer.motion();
                if motion == previous {
                    continue;
                }
                assert_eq!(motion.from, previous.to, "seed {seed}: teleported");
                assert_eq!(motion.start, previous.end, "seed {seed}: gap in time");
                assert_eq!(motion.end.count() - motion.start.count(), ticks_per_step);
                assert_eq!(grid.canonical(motion.to), Some(motion.to), "seed {seed}");
                assert!(!terrain.is_blocked(motion.to), "seed {seed}: {motion:?}");
                assert_eq!(
                    grid.manhattan_distance(motion.from, motion.to),
                    1,
                    "seed {seed}: not a single-tile step"
                );
                previous = motion;
            }
        }
    }

    /// Catches neighbours that stop at the seam: the only way out is across it.
    #[test]
    fn crosses_the_seam_when_that_is_the_only_way() {
        let start = TilePos::new(0, 5);
        let across = TilePos::new(open_terrain().grid().width() - 1, 5);
        let terrain = walled_except(&[start, across]);
        let wanderer = wanderer(1, start, &terrain, 2);
        assert_eq!(wanderer.motion().to, across);
    }

    /// Catches a hang or a panic when there is nowhere to go.
    #[test]
    fn waits_in_place_when_every_neighbour_is_blocked() {
        let start = TilePos::new(3, 3);
        let terrain = walled_except(&[start]);
        let mut wanderer = wanderer(1, start, &terrain, 2);
        wanderer.update(Tick::new(10), &terrain, 2);
        let motion = wanderer.motion();
        assert_eq!((motion.from, motion.to), (start, start));
        assert!(motion.end > Tick::new(10));
    }
}
