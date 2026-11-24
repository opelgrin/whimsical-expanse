# Architecture

How the system is structured and why. Not the rules every change must keep (see
[INVARIANTS.md](INVARIANTS.md)), not the plan (see [ROADMAP.md](ROADMAP.md)), not
what is built right now (see [STATUS.md](STATUS.md) and the code).

> This document describes the intended structure. The code is authoritative:
> where the two disagree, this document is fixed.

## Technology

- **Rust**, stable toolchain, as one Cargo workspace.
- **Platforms:** Linux, Windows and macOS (Apple silicon only).
- **winit** (windows and input), **wgpu** (rendering), **egui** (debug UI, and a
  first version of the game UI).
- **Dependencies are minimal.** Use the standard library first. A crate earns its
  place by doing a large, hard-to-write job (windowing, GPU, UI). Small utilities
  are written in-house, tailored to our needs: the RNG, the entity storage,
  deterministic maps, and the noise of generation.
- **libm** for sine, cosine and other functions that IEEE 754 does not round
  exactly. It is pure Rust, so it gives the same result on every platform, as
  [reproducibility](INVARIANTS.md#reproducibility) requires. Accurate and fast
  versions are hard to write.
- **bytemuck** for GPU data. It checks at compile time that a struct can be
  viewed as bytes, which hand-written `unsafe` cannot. The egui/wgpu stack
  already depends on it.
- **Lints and formatting:** see [STYLE.md](STYLE.md).

Bevy was rejected. Its ECS and scheduler would own the world and the frame,
which conflicts with an engine-independent simulation on its own thread. Its
breaking releases every few months also add ongoing cost.

## Crates

Packages are named `expanse-<crate>` (for example `expanse-core`), because
`core` is the name of Rust's own core library. A crate is created when it has
content, not in advance.

```text
crates/
  core/      IDs, coordinates (wrapping east–west), RNG streams, time
  terrain/   tiles in chunks, fields, biomes, obstacles, navigation
  worldgen/  generation: a space's terrain from the seed and its parameters
  sim/       world state, mechanics, perception, knowledge, events, snapshots
  ai/        drives, selection, methods, activities
  content/   loading and validating content definitions
  render/    wgpu: terrain, sprites, world overlays, the UI pass
  app/       tick pacing, simulation thread, window, input, UI
  game/      binary: opens the game window with default configuration
  headless/  binary: runs the simulation without a window
tools/
  cook/      asset cooking: licensed pack and placeholders
  docs-check/  warns about documents large enough to need a review
```

Dependencies point downwards only. `sim` and `ai` never depend on `render` or
`app`, and Cargo enforces this.

## Time

There are two clocks:

- **Action time** is measured in **ticks**, a fixed unit for physical, visible
  things such as walking one tile or swinging a tool. The simulation runs
  **20 ticks per second at 1×**, and 40 or 60 at 2× or 3×. The tick count is a
  `u64`.
- **Calendar time** (time of day, days, seasons) is derived from the tick count.
  **The length of a day in ticks is configurable**, and it sets the slow pace of
  the game. For example, a 30-minute day at 1× is 36,000 ticks.

Physical durations are written in ticks. Processes at the scale of a day (needs,
growth, routines, seasons) are written in calendar units ("per day") and
converted. Changing the length of a day therefore changes the pace of life, but
never how fast anything moves on screen.

## Runtime shape

The simulation thread owns the world and shares no mutable state with other
threads. Each tick:

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

**Player commands** are applied between ticks and recorded with their tick, so
a run can be replayed.

**Presentation** never reads the world. After each tick, the simulation
publishes a **snapshot**. Rendering draws one tick behind, interpolating motion
between the last two snapshots, and never extrapolates (see
[`simulation_thread.rs`](../crates/app/src/simulation_thread.rs)). Events, such
as those behind notifications, travel through a separate _ordered_ channel,
because snapshots can be skipped.

Commands and snapshots cross thread boundaries only through latest-value-wins
handoffs, never through queues, so no backlog builds up. Rendering therefore
never waits for the simulation. If a tick is slow, the camera, the UI and the
controls stay responsive, and entities stop at their last snapshot position. If
the simulation falls too far behind the selected speed, it drops the time it
owes instead of catching up indefinitely.

## Input

The window is the only code that sees platform events. It translates each one
for a single input state ([`crates/app/src/input/`](../crates/app/src/input/)),
which uses bindings to turn keys and mouse buttons into **input actions**.
Bindings are data, supplied by the game binary like other defaults.

Once per frame, consumers such as the camera controls read that state: what is
held, what was pressed, the cursor, its motion and scrolling. The end of the
frame forgets what happened but keeps what is held. Consumers never see platform
events, so each is tested without a window, and a new control adds a binding
instead of a branch in the window code.

The UI gets each event first. The input state ignores a press or a scroll that
the UI consumed, so that a click or a scroll over a panel does not also move the
camera. Releases and cursor changes always reach it, so that a button pressed
over the world and released over a panel is not left held.

An input action that changes the world becomes a player command; the rest stay
presentation.

## UI

The UI is built with egui. For now it is only the
[debug overlay](CONTEXT.md#presentation)
([`crates/app/src/overlay.rs`](../crates/app/src/overlay.rs)). It reads
snapshots like the rest of presentation and never acts on the simulation: it
returns requests, such as a new speed, which the window applies. egui runs every
frame, even while the overlay is hidden, so that its view of held buttons and
the cursor stays current.

The renderer owns everything that touches the GPU. It draws the UI in a second
pass over the world.

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
    spaces: Spaces, // each space's terrain, spatial index and passages
}
```

`Store<T>` is a small sparse set, iterated in a deterministic order. Mechanics
borrow only the stores they need, so their signatures show what they touch.

Why: capabilities such as health or inventory cut across kinds of entities, so
we need composition. The set of capabilities is closed and known at compile
time, because mechanics are typed code. An ECS's generic machinery (dynamic
types, queries, schedulers) brings nothing to that case. Moving to an ECS crate
later would be a mechanical change.

Items, and stacks of identical items, are entities too. Ground items, carried
items and per-item capabilities therefore use the same IDs and typed stores as
actors and objects (see [items](../docs/SIMULATION.md#representation)).

## World

- **The world is made of spaces.** The surface is one. Building interiors,
  dungeon floors and other worlds are others. Each space has its own terrain,
  in chunks, and its own size. A space either wraps east–west, as the surface
  does, or is bounded. Every distance and heuristic follows its space's shape.
- **Positions in the simulation are whole tiles of a space.** A position names
  its space. An entity moves to a neighbouring tile over a number of ticks.
  Smooth motion exists only in rendering.
- **Passages connect spaces.** A passage, such as a door or a cave mouth, joins
  a tile of one space to a tile of another. Crossing it takes ticks but is not a
  motion: rendering shows a transition instead of interpolating. Perception
  never crosses a passage.
- **Spaces appear and disappear at runtime**, for example when a building is
  built or demolished, so space IDs are generational, like entity IDs. A space
  may be generated when it is first needed. Its content then derives only from
  the seed and the space's stable identity, never from when it was generated.
- **Each space says whether the sky reaches it:** daylight, the seasonal look
  and weather. The calendar is the same in every space.
- Spaces have obstacles. Obstacles and terrain block vision cones.
- Navigation has three levels: the graph of spaces and passages; within each
  space, a coarse graph of chunk regions and the openings between them; and
  exact local paths. Each space caches the distances between its passages and
  refreshes them when its terrain changes. A route to another space costs the
  distance to a passage, plus the cached distances, plus the distance from the
  last passage. The same sum is the AI's cheap estimate of a cost. The region
  graph comes from Etendu, but it must be redesigned for obstacles inside chunks
  and for the wrap seam.

## Level of detail

Near the player character, the simulation runs every tick. Far from it, the same
simulation runs every N ticks, staggered so that the load stays smooth. This is
why [steps tolerate variable elapsed
time](INVARIANTS.md#simulation-steps-tolerate-variable-elapsed-time). Near and
far chunks hold the same kind of state, so a chunk changes detail at no cost.
That matters because passages make such changes frequent.

Distance is measured through passages. Within a space, it is the straight-line
distance, wrap-aware where the space wraps. To another space, it is the distance
to a passage, plus the cost of crossing it, plus the distance from its other
end, over the shortest such route. A character inside a tavern is therefore
near the square outside.

Detail is computed per chunk, and an entity takes the detail of its chunk. When
the player character enters another chunk:

1. A shortest-path search over passages only gives each passage's distance from
   the player character.
2. Each chunk takes the smallest of its direct distance, if it is in the player
   character's space, and each passage's distance plus the distance from that
   passage.

A chunk becomes near within a radius and becomes far again only beyond a larger
one, so that walking along the boundary or through a door does not toggle it.

## Content

Definitions (items, species, drives, methods…) are loaded and validated into an
immutable catalog before the simulation starts. The file format is open, and so
is whether `serde` earns its place.

## Assets

- Content names visuals by **logical asset IDs**, never by files inside the pack.
- The licensed pack lives outside the repository, located by configuration, and
  is identified by a committed provenance manifest (source, version, checksum,
  licence).
- Every logical ID has a placeholder in the repository.
- The `cook` tool builds a runtime container (atlases plus the mapping from IDs
  to regions) from either the pack, with our modifications, or the placeholders.

## Tooling

- A **headless runner** for tests and reproducible simulations.
- A **diagnostics runner**, a binary: many seeded runs, reports of stuck or
  looping actors, and the command that reproduces each case.
- In-game **inspection**: candidate goals and their values, goal chains, and
  knowledge. It serves debugging and the player alike.

<!-- size-reviewed: 11838 -->
