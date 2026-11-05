//! What presentation sees of the world.

use expanse_core::grid::TilePos;
use expanse_core::time::Tick;

/// Identifies an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u32);

/// A move from one tile to another over a span of ticks.
///
/// The entity is at `from` at tick `start` and at `to` at tick `end`. A motion
/// whose `from` equals its `to` is a wait. Rendering interpolates between the
/// two; the simulation only knows whole tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Motion {
    pub from: TilePos,
    pub to: TilePos,
    pub start: Tick,
    pub end: Tick,
}

/// One entity as presentation sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityView {
    pub id: EntityId,
    pub motion: Motion,
}

/// An owned projection of the world at one tick, published for presentation.
///
/// A snapshot holds no references into the world, so the simulation can keep
/// running while presentation draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub tick: Tick,
    pub entities: Vec<EntityView>,
}
