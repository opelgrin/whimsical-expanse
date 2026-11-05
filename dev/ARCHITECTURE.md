# Architecture

> **Status: intended.** No code exists yet. This document describes the
> structure we intend to build. As code arrives, each section is rewritten to
> describe what is actually implemented.

The [invariants](INVARIANTS.md) are the rules; this document explains the
structure that follows them.

## Technology

- **Rust**, stable toolchain, as one Cargo workspace.
- **Platforms:** Linux, Windows and macOS.
- **winit** (windows and input), **wgpu** (rendering), **egui** (debug UI, and a
  first version of the game UI).
- **Dependencies are minimal.** Use the standard library first. A crate must earn
  its place: large, hard-to-write pieces (windowing, GPU, UI) do; small utilities
  we can write ourselves, tailored to our needs, do not. The RNG, the entity
  storage and deterministic maps are written in-house.

Bevy was considered and rejected. Its ECS and scheduler would own the world
and the frame. That conflicts with an engine-independent simulation on its own
thread, and Bevy's breaking releases every few months add ongoing cost.

## Crates

```text
crates/
  core/      IDs, coordinates (wrapping east–west), RNG streams, time
  world/     terrain, chunks, navigation
  sim/       world state, mechanics, perception, knowledge, events, snapshots
  ai/        drives, selection, methods, activities
  content/   loading and validating content definitions
  render/    wgpu: terrain, sprites, overlays
  app/       window, input, simulation thread, UI
tools/
  cook/      asset cooking: licensed pack and placeholders
binaries: the game, a headless runner, diagnostics
```

Dependencies point downwards only. `sim` and `ai` never depend on `render` or
`app`, and Cargo enforces this.

## Runtime shape

The simulation runs on its own thread and owns the world. Each tick:

1. **Perceive:** actors update their knowledge from what they perceive. This is
   computed in parallel and committed in order.
2. **Decide:** actors due for a decision compute it in parallel, reading the
   world immutably (`&World`, shared across threads).
3. **Commit:** decisions are applied sequentially in entity-ID order
   (`&mut World`). Conflicts are settled here.
4. **Advance:** mechanics run and actions execute against the truth. Events are
   recorded.

Parallel phases use a thread pool. The borrow checker expresses
compute-then-commit directly: shared borrows while computing, an exclusive
borrow while committing.

**Presentation** never reads the world directly. After each tick, the
simulation publishes a **snapshot**: an owned projection of what is visible. A
newer snapshot replaces one not yet drawn. Rendering interpolates motion between
two ticks and never extrapolates. Events (for notifications and the chronicle)
travel through a separate *ordered* channel, because snapshots can be skipped.

**Player commands** are applied between ticks and recorded with their tick, so a
run can be replayed.

## Entities and storage

There is no general-purpose ECS. Entities are generational IDs. Each
capability has a typed store, as a field of the world:

```rust
pub struct World {
    ids: EntityAllocator,
    positions: Store<Position>,
    inventories: Store<Inventory>,
    minds: Store<Mind>,
    // one store per capability, added when a mechanic needs it
    terrain: Terrain,
    spatial: SpatialIndex,
}
```

`Store<T>` is a small sparse set, iterated in a deterministic order. Mechanics
borrow only the stores they need, so what they touch is visible in their
signatures.

Why: we need composition, because capabilities such as health or inventory cut
across kinds of entities. The set of capabilities is closed and known at compile
time, since mechanics are typed code. An ECS's generic machinery (dynamic types,
queries, schedulers) brings no benefit to that case. Moving to an ECS crate later
would be a mechanical change.

Ordinary items are values inside inventories, not entities (see
[items](../docs/SIMULATION.md#items)).

## World

- Tiles grouped into chunks. The world is finite and wraps east–west, so every
  distance and heuristic is wrap-aware.
- The world has obstacles. Vision cones are occluded by obstacles and terrain.
- Navigation has two levels: a coarse graph of chunk regions and the portals
  between them, plus exact local paths. The idea comes from Etendu, but it must
  be redesigned for obstacles inside chunks and for the wrap seam.

## Content

Definitions (items, species, drives, methods…) are loaded and validated into an
immutable catalog before the simulation starts. The file format is still open.

## Assets

- Content names visuals by **logical asset IDs**.
- The licensed pack lives outside the repository, located by configuration, and
  is identified by a committed provenance manifest (source, version, checksum,
  licence).
- The `cook` tool builds a runtime container (atlases plus the mapping from IDs
  to regions) from either the pack or the in-repository placeholders.

## Tooling

- A **headless runner** for tests and reproducible simulations.
- A **diagnostics runner**: many seeded runs, reports of stuck or looping actors,
  and the command that reproduces each case.
- In-game **inspection**: candidate goals and their values, goal chains, and
  knowledge. This serves debugging and the player alike.

## Open technical questions

- The content file format, and whether `serde` earns its place.
- The tick rate and how ticks map to the calendar.
- The approach to coarse simulation far from the player.
- CI across the three platforms.
