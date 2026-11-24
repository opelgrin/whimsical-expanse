//! The world: owner of all simulation state.

use std::error::Error;
use std::fmt;
use std::hash::BuildHasher;

use expanse_core::hash::FixedBuildHasher;
use expanse_core::rng::Rng;
use expanse_core::time::Tick;
use expanse_terrain::Terrain;

use crate::snapshot::{EntityId, EntityView, Snapshot};
use crate::wanderer::Wanderer;

/// Everything a world is created from, besides its terrain.
///
/// There is deliberately no default: callers state every parameter, so tests
/// never depend on values tuned for the game.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorldConfig {
    /// The seed every random stream derives from.
    pub seed: u64,
    /// How many ticks the wanderer takes to move one tile.
    pub ticks_per_step: u64,
}

/// Why a world could not be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidConfig {
    /// Moving one tile would take no time.
    ZeroTicksPerStep,
    /// Every tile of the surface is blocked, so no entity can stand on it.
    NoPassableTile,
}

impl fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTicksPerStep => write!(f, "moving one tile must take at least one tick"),
            Self::NoPassableTile => write!(f, "the surface has no passable tile"),
        }
    }
}

impl Error for InvalidConfig {}

/// Random stream keys, so that each use of randomness gets its own stream.
mod stream {
    pub const WANDERER: u64 = 1;
    pub const WANDERER_START: u64 = 2;
}

/// The owner of all simulation state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct World {
    config: WorldConfig,
    tick: Tick,
    terrain: Terrain,
    wanderer: Wanderer,
}

impl World {
    /// The id of the single wandering entity.
    const WANDERER_ID: EntityId = EntityId(0);

    /// Creates a world at tick zero on `surface`, the terrain generated for
    /// it.
    ///
    /// The wanderer starts in the largest region of passable tiles, so that
    /// it is never shut in a small pocket.
    ///
    /// # Errors
    ///
    /// Returns an error if a step would take no time, or if the surface has
    /// no passable tile.
    pub fn new(config: WorldConfig, surface: Terrain) -> Result<Self, InvalidConfig> {
        if config.ticks_per_step == 0 {
            return Err(InvalidConfig::ZeroTicksPerStep);
        }
        let open = surface
            .largest_open_region()
            .ok_or(InvalidConfig::NoPassableTile)?;
        let wanderer_keys = [stream::WANDERER, u64::from(Self::WANDERER_ID.0)];
        let start =
            open[Rng::from_stream(config.seed, &[stream::WANDERER_START]).index(open.len())];
        let rng = Rng::from_stream(config.seed, &wanderer_keys);
        let wanderer = Wanderer::new(start, Tick::ZERO, rng, &surface, config.ticks_per_step);
        Ok(Self {
            config,
            tick: Tick::ZERO,
            terrain: surface,
            wanderer,
        })
    }

    /// The world's width and height in tiles.
    #[must_use]
    pub const fn dimensions(&self) -> (i32, i32) {
        let grid = self.terrain.grid();
        (grid.width(), grid.height())
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
            .update(self.tick, &self.terrain, self.config.ticks_per_step);
    }

    /// A hash of the complete simulation state, terrain included.
    ///
    /// Equal states give equal hashes for a given binary. Used to check that
    /// runs are reproducible; it is not stable across code changes.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        FixedBuildHasher::default().hash_one(self)
    }

    /// An owned projection of the current state, for presentation. It shares
    /// the terrain instead of copying it.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            tick: self.tick,
            terrain: self.terrain.clone(),
            entities: vec![EntityView {
                id: Self::WANDERER_ID,
                motion: self.wanderer.motion(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use expanse_core::grid::TilePos;
    use expanse_terrain::{BiomeIndex, Fields, Tile};

    use super::*;

    fn config(seed: u64) -> WorldConfig {
        WorldConfig {
            seed,
            ticks_per_step: 5,
        }
    }

    /// A surface of 2 by 1 chunks whose impassable tiles are those for which
    /// `impassable` holds.
    fn surface(impassable: impl Fn(TilePos) -> bool + Sync) -> Terrain {
        let fields = Fields {
            elevation: 0,
            winter_temperature: 0,
            summer_temperature: 0,
            moisture: 0,
            vegetation: 0,
        };
        Terrain::new(2, 1, |tile| {
            Tile::new(fields, BiomeIndex(0), impassable(tile))
        })
        .expect("a terrain with tiles")
    }

    /// A surface with scattered impassable tiles.
    fn scattered() -> Terrain {
        surface(|tile| (tile.x * 7 + tile.y * 3) % 5 == 0)
    }

    fn run(config: WorldConfig, ticks: u64) -> World {
        let mut world = World::new(config, scattered()).expect("valid config");
        for _ in 0..ticks {
            world.advance();
        }
        world
    }

    /// Catches a state hash that ignores the state. Reproducibility checks
    /// across processes compare hashes, so such a hash would make them pass
    /// whatever happens.
    #[test]
    fn the_state_hash_changes_as_the_world_changes() {
        let mut world = World::new(config(3), scattered()).expect("valid config");
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
            let mut world = World::new(config(seed), scattered()).expect("valid config");
            (0..500)
                .map(|_| {
                    world.advance();
                    world.snapshot().entities[0].motion.to
                })
                .collect::<Vec<_>>()
        };
        assert_ne!(trajectory(3), trajectory(4));
    }

    /// Catches a wanderer shut in a small pocket: a start chosen anywhere
    /// passable, or in the first region found. Two impassable walls cut a
    /// strip of two columns from the rest of the surface.
    #[test]
    fn the_wanderer_starts_in_the_largest_open_region() {
        let surface = surface(|tile| tile.x == 1 || tile.x == 4);
        for seed in 0..50 {
            let world = World::new(config(seed), surface.clone()).expect("valid config");
            let start = world.snapshot().entities[0].motion.from;
            assert!(!(2..=3).contains(&start.x), "seed {seed}: {start:?}");
            assert!(!surface.is_blocked(start), "seed {seed}: {start:?}");
        }
    }

    /// Catches configurations that would hang the simulation or leave an entity
    /// with nowhere to stand.
    #[test]
    fn rejects_configs_that_cannot_run() {
        let instant = WorldConfig {
            ticks_per_step: 0,
            ..config(1)
        };
        assert_eq!(
            World::new(instant, scattered()),
            Err(InvalidConfig::ZeroTicksPerStep)
        );
        assert_eq!(
            World::new(config(1), surface(|_| true)),
            Err(InvalidConfig::NoPassableTile)
        );
    }
}
