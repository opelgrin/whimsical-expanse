# Roadmap

The order of milestones and what each delivers: the initial steps, not the full
roadmap. It changes as we learn. Not the current position (see
[STATUS.md](STATUS.md)), the structure (see [ARCHITECTURE.md](ARCHITECTURE.md))
or the game's design (see [docs/](../docs/)).

## How this roadmap works

- **A milestone is a visible, runnable outcome**, not a list of features. Each
  ends with an exit criterion that can be demonstrated.
- **Detail decreases with distance.** The next milestones list coarse items;
  later ones are one line. A milestone's items are refined when it starts, in a
  spec if it is large enough.
- **The game grows from the start.** Every milestone ends in something that runs,
  and the player character appears early. We do not build the whole simulation
  before the game.

## Cross-cutting principles

- **Mortality from the first entities.** Entities can cease to exist from the
  moment they exist. That covers storage, references, knowledge about an entity
  that is gone, goals that target it and activities it was part of. No part of
  the decision system may be built on the hidden assumption that actors are
  immortal. Death by starvation arrives with the first drives (M3), combat later.
- **Representative content early.** The decision system is exercised from the
  start by an actor without physiological drives, next to those with them (see
  [the vision](../docs/VISION.md#representative-content)).
- **Determinism is tested from M0**, and every later milestone keeps the test
  passing.
- **CI on all supported platforms from M0.**

## M0: Foundation slice

One thin cut through every layer.

- [x] Cargo workspace and crate skeletons.
- [ ] Window (winit + wgpu), camera with panning and zoom.
- [x] Simulation thread, tick loop and snapshots.
- [ ] Interpolated motion.
- [x] One entity moving across a flat grid.
- [x] Headless runner, and a determinism test: the same seed gives the same world
  state hash after N ticks.
- [ ] egui debug overlay.
- [x] CI on Linux, Windows and macOS.

**Exit:** the game shows an entity moving on a grid. Headless runs give the same
hash every time, on every platform.

## M1: A world to explore

- Generation: elevation, temperature and moisture, with latitude for climate,
  giving biomes. Obstacles.
- Chunks, east–west wrapping.
- Batched tile renderer with placeholder art.
- Calendar with days and seasons.
- Tile inspector.
- Asset pipeline: provenance manifest, `cook` tool, logical asset IDs. The real
  pack is tried locally (tiles and seasonal variants) to confirm we can work with
  it. It never enters git.

**Exit:** you can pan across a generated world, cross the seam and watch the
seasons change, with placeholder art or, locally, the real pack.

## M2: Bodies, perception and movement

- Entities and typed stores, with removal and stale references handled.
- Two-level navigation with obstacles and the wrap.
- Vision cones with occlusion, and bounded memory of facts.
- The player character as an actor, with "go there" orders.

**Exit:** actors wander, see and remember, and debug views show their cones and
memories. You can send your character somewhere.

## M3: Deciding

- Drives, selection and backward-chained methods.
- Objects and items that advertise, with effects and body rules. Meters.
- Decide how items are represented (values or entities) before inventories are
  built on it (see [items](../docs/SIMULATION.md#representation)).
- Goal chain inspector.
- Content: a grazing animal, a villager, and an actor with no physiological
  drives.
- Death by starvation.

**Exit:** actors live on their own and can die, and you can ask any of them why
they are doing what they do.

## Later

One line each, refined when reached. Together they lead to the
[first game](../docs/VISION.md#first-game).

- **Danger:** health, fighting, fleeing, death in combat, predator and prey.
- **Social:** proposals, shared activities, relationships, routines.
- **The player layer:** obedience, direct control, fog of war, notifications.
- **Farming and a simple economy.**
- **A first dungeon.**
- **Coarse simulation far from the player.**
