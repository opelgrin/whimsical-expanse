# Architecture

How the system is structured and why. Not the rules every change must keep (see
[INVARIANTS.md](INVARIANTS.md)), not the plan (see [ROADMAP.md](ROADMAP.md)), not
what is built right now (see [STATUS.md](STATUS.md) and the code).

> **Status: partly built.** This describes the structure we intend. Where the code
> disagrees, the code is current and this document needs fixing. As parts are
> built, their sections are rewritten to describe what is implemented.

The [invariants](INVARIANTS.md) are the rules; this document explains the
structure that follows them.

## Technology

- **Rust**, stable toolchain, as one Cargo workspace.
- **Platforms:** Linux, Windows and macOS (Apple silicon only).
- **winit** (windows and input), **wgpu** (rendering), **egui** (debug UI, and a
  first version of the game UI).
- **Dependencies are minimal.** Use the standard library first. A crate must earn
  its place: large, hard-to-write pieces (windowing, GPU, UI) do; small utilities
  we can write ourselves, tailored to our needs, do not. The RNG, the entity
  storage and deterministic maps are written in-house.
- **bytemuck** for GPU data. It checks at compile time that a struct can be
  viewed as bytes (no padding, `repr(C)`), which hand-written `unsafe` cannot
  check. It is already a dependency of the egui/wgpu stack.
- **Lints and formatting** are described in [STYLE.md](STYLE.md).

Bevy was considered and rejected. Its ECS and scheduler would own the world
and the frame. That conflicts with an engine-independent simulation on its own
thread, and Bevy's breaking releases every few months add ongoing cost.

## Crates

Packages are named `expanse-<crate>` (for example `expanse-core`), because
`core` is the name of Rust's own core library. Crates are created when they
have content, not in advance.

```text
crates/
  core/      IDs, coordinates (wrapping east–west), RNG streams, time
  world/     terrain, chunks, navigation
  sim/       world state, mechanics, perception, knowledge, events, snapshots
  ai/        drives, selection, methods, activities
  content/   loading and validating content definitions
  render/    wgpu: terrain, sprites, overlays
  app/       tick pacing, simulation thread, window, input, UI
  game/      binary: opens the game window with default configuration
  headless/  binary: runs the simulation without a window
tools/
  cook/      asset cooking: licensed pack and placeholders
  docs-check/  warns about documents large enough to need a review
binaries: the game, the headless runner, diagnostics
```

Dependencies point downwards only. `sim` and `ai` never depend on `render` or
`app`, and Cargo enforces this.

## Time

There are two clocks:

- **Action time** is measured in **ticks**. A tick is a fixed unit of time for
  physical, visible things, such as walking one tile or swinging a tool. The
  simulation runs **20 ticks per second at 1×**, and 40 or 60 at 2× or 3×. The
  tick count is a `u64`.
- **Calendar time** (time of day, days, seasons) is derived from the tick count.
  **The length of a day in ticks is configurable**, and the slow pace of the
  game comes from it. For example, a 30-minute day at 1× is 36,000 ticks.

Physical durations are written in ticks. Processes that live at the scale of a
day (needs, growth, routines, seasons) are written in calendar units ("per day")
and converted. Changing the length of a day therefore changes the pace of life,
but never how fast anything moves on screen.

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

Rendering and simulation are independent. If a tick is slow, rendering keeps
running: the camera, the UI and the controls stay responsive, and entities hold
their last position until the next snapshot. If the simulation falls too far
behind the selected speed, it drops the time it owes rather than trying to
catch up indefinitely.

**Presentation** never reads the world directly. After each tick, the
simulation publishes a **snapshot**: an owned projection of what is visible. A
newer snapshot replaces one not yet drawn. Rendering interpolates motion between
two ticks and never extrapolates. Events (e.g. for notifications)
travel through a separate _ordered_ channel, because snapshots can be skipped.

The simulation thread owns the world and shares no mutable state with other
threads. Commands and snapshots cross thread boundaries only through
latest-value-wins handoffs. No backlogs or queues are held.

**Player commands** are applied between ticks and recorded with their tick, so a
run can be replayed.

## Input

The window is the only code that sees platform events. It passes each one to a
single input state ([`crates/app/src/input/`](../crates/app/src/input/)), which
uses bindings to turn keys and mouse buttons into **input actions**. Bindings are
data, supplied by the game binary like other defaults.

Once per frame, consumers read that state: what is held, what was pressed, the
cursor, its motion and scrolling. The camera controls are one consumer. The
frame then ends, which forgets what happened but keeps what is held. Consumers
never see platform events, so each is tested without a window and a new control
adds a binding instead of a branch in the window code.

A UI layer will get each event before the input state, so that scrolling or
clicking over a panel does not also move the camera. An input action that
changes the world becomes a player command; the rest stay presentation.

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

Items (and stacks of identical items) are entities too, so ground items, carried
items and per-item capabilities use the same entity IDs and typed stores as
actors and objects (see [items](../docs/SIMULATION.md#representation)).

## World

- **Positions in the simulation are whole tiles.** An entity moves to a
  neighbouring tile over a number of ticks. Smooth motion exists only in
  rendering.
- Tiles grouped into chunks. The world is finite and wraps east–west, so every
  distance and heuristic is wrap-aware.
- The world has obstacles. Vision cones are occluded by obstacles and terrain.
- Navigation has two levels: a coarse graph of chunk regions and the portals
  between them, plus exact local paths. The idea comes from Etendu, but it must
  be redesigned for obstacles inside chunks and for the wrap seam.

## Content

Definitions (items, species, drives, methods…) are loaded and validated into an
immutable catalog before the simulation starts. The file format is open, and so is
whether `serde` earns its place.

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
