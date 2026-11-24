//! The simulation: world state, mechanics and snapshots.
//!
//! It runs without a window and never reads the wall clock. Presentation only
//! sees it through [`Snapshot`]s.

mod snapshot;
mod wanderer;
mod world;

pub use snapshot::{EntityId, EntityView, Motion, Snapshot};
pub use world::{InvalidConfig, World, WorldConfig};
