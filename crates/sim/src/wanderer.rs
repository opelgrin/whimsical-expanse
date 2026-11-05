//! A placeholder entity that wanders the grid, one tile at a time.
//!
//! It exists to exercise the tick loop, snapshots and determinism before real
//! actors arrive.

use expanse_core::grid::{Direction, TilePos};
use expanse_core::rng::Rng;
use expanse_core::time::Tick;

use crate::snapshot::Motion;

/// The rectangle of tiles an entity may occupy: `0..width` by `0..height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bounds {
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    fn contains(self, tile: TilePos) -> bool {
        (0..self.width).contains(&tile.x) && (0..self.height).contains(&tile.y)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Wanderer {
    motion: Motion,
    rng: Rng,
}

impl Wanderer {
    /// Starts at `tile` at tick `now`, already on its way to a first neighbour.
    pub fn new(tile: TilePos, now: Tick, rng: Rng, bounds: Bounds, ticks_per_step: u64) -> Self {
        let mut wanderer = Self {
            motion: Motion {
                from: tile,
                to: tile,
                start: now,
                end: now,
            },
            rng,
        };
        wanderer.start_next_step(bounds, ticks_per_step);
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
    pub fn update(&mut self, now: Tick, bounds: Bounds, ticks_per_step: u64) {
        while self.motion.end <= now {
            self.start_next_step(bounds, ticks_per_step);
        }
    }

    /// Starts a step to a random neighbouring tile within `bounds`, beginning
    /// when the current motion ends. Waits in place if no neighbour is inside.
    fn start_next_step(&mut self, bounds: Bounds, ticks_per_step: u64) {
        let here = self.motion.to;
        let mut options = [here; Direction::ALL.len()];
        let mut count = 0;
        for direction in Direction::ALL {
            if let Some(next) = here
                .neighbour(direction)
                .filter(|&tile| bounds.contains(tile))
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
    use super::*;

    const BOUNDS: Bounds = Bounds {
        width: 5,
        height: 3,
    };

    fn wanderer(seed: u64, ticks_per_step: u64) -> Wanderer {
        Wanderer::new(
            TilePos::new(2, 1),
            Tick::ZERO,
            Rng::from_stream(seed, &[]),
            BOUNDS,
            ticks_per_step,
        )
    }

    /// The variable-elapsed-time invariant: catches steps that start at the
    /// update tick instead of when the previous step ended, or that skip or
    /// repeat steps when updates are infrequent.
    #[test]
    fn updating_rarely_gives_the_same_result_as_updating_every_tick() {
        let ticks_per_step = 3;
        for seed in 0..20 {
            let mut every_tick = wanderer(seed, ticks_per_step);
            let mut rarely = wanderer(seed, ticks_per_step);
            for count in 1..=200 {
                every_tick.update(Tick::new(count), BOUNDS, ticks_per_step);
                if count % 37 == 0 || count == 200 {
                    rarely.update(Tick::new(count), BOUNDS, ticks_per_step);
                }
            }
            assert_eq!(every_tick, rarely, "seed {seed}");
        }
    }

    /// Catches broken movement: jumps of more than one tile, leaving the
    /// bounds, gaps or overlaps in time, or steps of the wrong duration.
    #[test]
    fn steps_are_continuous_single_tile_moves_inside_the_bounds() {
        let ticks_per_step = 4;
        for seed in 0..20 {
            let mut wanderer = wanderer(seed, ticks_per_step);
            let mut previous = wanderer.motion();
            for count in 1..=400 {
                wanderer.update(Tick::new(count), BOUNDS, ticks_per_step);
                let motion = wanderer.motion();
                if motion == previous {
                    continue;
                }
                assert_eq!(motion.from, previous.to, "seed {seed}: teleported");
                assert_eq!(motion.start, previous.end, "seed {seed}: gap in time");
                assert_eq!(motion.end.count() - motion.start.count(), ticks_per_step);
                assert!(BOUNDS.contains(motion.to), "seed {seed}: left the bounds");
                let distance =
                    (motion.to.x - motion.from.x).abs() + (motion.to.y - motion.from.y).abs();
                assert_eq!(distance, 1, "seed {seed}: not a single-tile step");
                previous = motion;
            }
        }
    }

    /// Catches a hang or a panic when there is nowhere to go.
    #[test]
    fn waits_in_place_when_no_neighbour_is_inside() {
        let single_tile = Bounds {
            width: 1,
            height: 1,
        };
        let mut wanderer = Wanderer::new(
            TilePos::new(0, 0),
            Tick::ZERO,
            Rng::from_stream(1, &[]),
            single_tile,
            2,
        );
        wanderer.update(Tick::new(10), single_tile, 2);
        let motion = wanderer.motion();
        assert_eq!(
            (motion.from, motion.to),
            (TilePos::new(0, 0), TilePos::new(0, 0))
        );
        assert!(motion.end > Tick::new(10));
    }
}
