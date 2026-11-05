//! The world: owner of all simulation state.

use std::error::Error;
use std::fmt;
use std::hash::BuildHasher;

use expanse_core::grid::TilePos;
use expanse_core::hash::FixedBuildHasher;
use expanse_core::rng::Rng;
use expanse_core::time::Tick;

use crate::snapshot::{EntityId, EntityView, Snapshot};
use crate::wanderer::{Bounds, Wanderer};

/// Everything a world is created from.
///
/// There is deliberately no default: callers state every parameter, so tests
/// never depend on values tuned for the game.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorldConfig {
    /// The seed every random stream derives from.
    pub seed: u64,
    /// Width of the grid, in tiles.
    pub width: i32,
    /// Height of the grid, in tiles.
    pub height: i32,
    /// How many ticks the wanderer takes to move one tile.
    pub ticks_per_step: u64,
}

/// Why a [`WorldConfig`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidConfig {
    /// The grid has no tiles.
    EmptyGrid,
    /// Moving one tile would take no time.
    ZeroTicksPerStep,
}

impl fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGrid => write!(f, "the grid must be at least one tile wide and high"),
            Self::ZeroTicksPerStep => write!(f, "moving one tile must take at least one tick"),
        }
    }
}

impl Error for InvalidConfig {}

/// Random stream keys, so that each use of randomness gets its own stream.
mod stream {
    pub const WANDERER: u64 = 1;
}

/// The owner of all simulation state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct World {
    config: WorldConfig,
    tick: Tick,
    wanderer: Wanderer,
}

impl World {
    /// The id of the single wandering entity.
    const WANDERER_ID: EntityId = EntityId(0);

    /// Creates a world at tick zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the grid is empty or a step would take no time.
    pub fn new(config: WorldConfig) -> Result<Self, InvalidConfig> {
        if config.width < 1 || config.height < 1 {
            return Err(InvalidConfig::EmptyGrid);
        }
        if config.ticks_per_step == 0 {
            return Err(InvalidConfig::ZeroTicksPerStep);
        }
        let rng = Rng::from_stream(
            config.seed,
            &[stream::WANDERER, u64::from(Self::WANDERER_ID.0)],
        );
        let centre = TilePos::new(config.width / 2, config.height / 2);
        let wanderer = Wanderer::new(
            centre,
            Tick::ZERO,
            rng,
            bounds(&config),
            config.ticks_per_step,
        );
        Ok(Self {
            config,
            tick: Tick::ZERO,
            wanderer,
        })
    }

    /// The current tick.
    #[must_use]
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// Advances the world by one tick.
    pub fn advance(&mut self) {
        self.tick = self.tick.after(1);
        self.wanderer
            .update(self.tick, bounds(&self.config), self.config.ticks_per_step);
    }

    /// A hash of the complete simulation state.
    ///
    /// Equal states give equal hashes for a given binary. Used to check that
    /// runs are reproducible; it is not stable across code changes.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        FixedBuildHasher::default().hash_one(self)
    }

    /// An owned projection of the current state, for presentation.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            tick: self.tick,
            entities: vec![EntityView {
                id: Self::WANDERER_ID,
                motion: self.wanderer.motion(),
            }],
        }
    }
}

const fn bounds(config: &WorldConfig) -> Bounds {
    Bounds {
        width: config.width,
        height: config.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(seed: u64) -> WorldConfig {
        WorldConfig {
            seed,
            width: 16,
            height: 9,
            ticks_per_step: 5,
        }
    }

    fn run(config: WorldConfig, ticks: u64) -> World {
        let mut world = World::new(config).expect("valid config");
        for _ in 0..ticks {
            world.advance();
        }
        world
    }

    /// The determinism invariant: catches any source of variation between runs,
    /// such as a randomly seeded hasher or iteration over an unordered
    /// container.
    #[test]
    fn same_seed_gives_the_same_state() {
        assert_eq!(run(config(3), 500), run(config(3), 500));
    }

    /// Catches a state hash that ignores the state. Reproducibility checks
    /// across processes compare hashes, so such a hash would make them pass
    /// whatever happens.
    #[test]
    fn the_state_hash_changes_as_the_world_changes() {
        let mut world = World::new(config(3)).expect("valid config");
        let mut previous = world.state_hash();
        for _ in 0..20 {
            world.advance();
            let current = world.state_hash();
            assert_ne!(current, previous, "at {:?}", world.tick());
            previous = current;
        }
    }

    /// Catches a world that does not advance, or advances time without updating
    /// what lives in it. Comparing two runs cannot see either: both would be
    /// equally frozen.
    #[test]
    fn advancing_moves_time_and_the_wanderer_forward() {
        let config = config(3);
        let ticks = 4 * config.ticks_per_step;
        let world = run(config, ticks);
        assert_eq!(world.tick(), Tick::new(ticks));
        let motion = world.snapshot().entities[0].motion;
        assert!(
            motion.start > Tick::ZERO,
            "the wanderer never finished a step"
        );
        assert!(motion.start <= world.tick() && world.tick() < motion.end);
    }

    /// Catches randomness that is ignored or not seeded, which would make every
    /// world behave identically. Compares movement rather than state hashes:
    /// the hash includes the generator's internal state, so it differs between
    /// seeds even when nothing uses the generator.
    #[test]
    fn different_seeds_give_different_movement() {
        let trajectory = |seed| {
            let mut world = World::new(config(seed)).expect("valid config");
            (0..500)
                .map(|_| {
                    world.advance();
                    world.snapshot().entities[0].motion.to
                })
                .collect::<Vec<_>>()
        };
        assert_ne!(trajectory(3), trajectory(4));
    }

    /// Catches configurations that would hang the simulation or leave an entity
    /// with nowhere to stand.
    #[test]
    fn rejects_configs_that_cannot_run() {
        let empty = WorldConfig {
            width: 0,
            ..config(1)
        };
        assert_eq!(World::new(empty), Err(InvalidConfig::EmptyGrid));
        let instant = WorldConfig {
            ticks_per_step: 0,
            ..config(1)
        };
        assert_eq!(World::new(instant), Err(InvalidConfig::ZeroTicksPerStep));
    }
}
