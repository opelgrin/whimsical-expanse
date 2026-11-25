//! Creating a world: generating its surface, then placing what lives on it.

use std::error::Error;
use std::fmt;

use expanse_sim::{InvalidConfig, World, WorldConfig};
use expanse_terrain::BiomeTable;
use expanse_worldgen::{InvalidParams, SurfaceParams, generate_surface};

/// Everything a world is created from.
#[derive(Debug, Clone, PartialEq)]
pub struct Genesis {
    /// The world's configuration. Its seed is the game's seed, from which the
    /// surface is generated too.
    pub world: WorldConfig,
    /// The parameters of the surface.
    pub surface: SurfaceParams,
    /// The biomes of the surface, which presentation also reads for their
    /// names and colours.
    pub biome_table: BiomeTable,
}

impl Genesis {
    /// Generates the surface and creates the world on it, at tick zero.
    ///
    /// # Errors
    ///
    /// Returns an error naming the problem if the surface parameters are
    /// invalid, or if the world cannot be created on the surface they give.
    pub fn create_world(&self) -> Result<World, GenesisError> {
        let surface = generate_surface(self.world.seed, &self.surface, &self.biome_table)
            .map_err(GenesisError::Surface)?;
        World::new(self.world.clone(), surface).map_err(GenesisError::World)
    }
}

/// Why a world could not be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisError {
    /// The surface parameters are invalid.
    Surface(InvalidParams),
    /// The world cannot be created on the generated surface.
    World(InvalidConfig),
}

impl fmt::Display for GenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Surface(err) => write!(f, "invalid surface parameters: {err}"),
            Self::World(err) => write!(f, "cannot create the world: {err}"),
        }
    }
}

impl Error for GenesisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Surface(err) => Some(err),
            Self::World(err) => Some(err),
        }
    }
}
