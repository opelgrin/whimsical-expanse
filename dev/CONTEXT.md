# Context

The project's vocabulary. One meaning per word, one word per meaning. This is a
glossary only: rules are in [INVARIANTS.md](INVARIANTS.md) and design is in
[docs/](../docs/).

## Time

**Tick**: the smallest step of the simulation, and the unit of action time.
Everything happens in whole ticks.

**Action time**: how long physical, visible things take, measured in ticks.

**Calendar**: time of day, days and seasons, derived from the tick count. The
length of a day in ticks is configurable.

**Frame**: one image drawn. Frames and ticks are independent.

## Input

**Input action**: what the player means by a key or mouse button, such as
panning the camera, whichever button is bound to it. Not an actor's action, and
not a player command, which reaches the world.

## Presentation

**Snapshot**: an owned projection of the world at one tick, published for
presentation. It holds no references into the world. It shares the terrain's
immutable chunks rather than copying them.

**Debug overlay**: development controls and readings drawn over the world,
hidden until toggled. Not the game's UI.

## Spaces and terrain

**World**: the whole simulation state at one tick: every space, what lives in
it and the time. The simulation thread owns it. Not the terrain alone.

**Space**: a grid of tiles with its own terrain, size and shape, such as the
surface, a building interior, a dungeon floor or another world. Every position
is in one space.

**Surface**: the main space, finite but large, which wraps east–west.

**Passage**: a link from a tile of one space to a tile of another, such as a
door, stairs, a cave mouth or a portal.

**Near**: within the radius of full simulation around the player character,
measured through passages. Everything else is **far**.

**Throttling**: running the simulation of far chunks every N ticks instead of
every tick.

**Tile**: one cell of the world grid. Positions in the simulation are whole tiles.

**Chunk**: a square block of tiles, stored and loaded as a unit. A storage
concept, never a rendering one.

**Wrap**: a space joins its east and west edges; north and south are edges.
The surface wraps.

**Seam**: where the last column of a wrapping space meets the first.

**Canonical**: a tile position whose `x` lies inside its space's columns. In a
wrapping space, every column has many positions, one canonical; rows have only
one.

**Terrain**: every tile of a space and what it holds: its fields, its biome
and its obstacle.

**Field**: a value that varies over the terrain and selects biomes: elevation,
winter and summer temperature, moisture, vegetation.

**Biome**: a kind of land, chosen for each tile from its fields by the biome
table, which content supplies.

**Biome table**: the biomes in priority order, each with a range for some
fields. A tile takes the first biome whose ranges contain its fields.

**Impassable**: of a biome, such as water, that blocks movement without an
obstacle.

**Region**: a set of tiles that share something, such as a biome, joined by
their edges, across the seam too.

**Sea level**: elevation 0. Tiles below it are water.

**Winter and summer temperature**: a tile's coldest and warmest average of
the year. Their mean is the **annual mean**, their difference the **swing**.

**Latitude**: of a row of the surface, from the north and south edges'
latitudes. It sets the climate and, south of the equator, shifts the seasons
by half a year.

**Generation**: building a space's terrain from the seed and the space's
parameters, before the first tick, in **passes**: steps over the whole space
that each write one layer, such as elevation or obstacles.

**Obstacle**: something on the terrain that blocks movement, such as a rock or
a tree. Part of the terrain, not an entity.

**Obstacle kind**: the content definition of an obstacle: its name and
footprint. A biome lists the kinds it places, each with a **density**, the
chance that a tile anchors one, and an optional **variation**, smooth noise
that groups them.

**Footprint**: the tiles an obstacle blocks, given as offsets from its anchor.
Unrelated to the size of its sprite.

**Anchor**: the tile of a footprint that holds the obstacle itself; the others
point to it.

## Entities

**Motion**: a move from one tile to another over a span of ticks. The
simulation knows only whole tiles; rendering interpolates along a motion.

**State hash**: a hash of the complete simulation state, used to check that
runs are reproducible. Stable for one binary only.

**Entity**: anything with an identity in the world: an actor, an object or an
item (or a stack of identical items; see
[items](../docs/SIMULATION.md#representation)). Identified by a generational ID.

**Store**: the typed storage of one capability (position, inventory…) for the
entities that have it.

**Actor**: an entity that perceives and decides. The player character is one.

**Player character**: the actor the player follows and can give orders to.

## Decisions

**Drive**: a source of wanting, defined in content. It reads the actor's beliefs
and proposes candidate goals with values.

**Goal**: what an actor wants. Either a *condition* that can be checked against
its beliefs, or an *activity* to complete. It says what, never how.

**Candidate goal**: a goal proposed by a drive and not yet selected.

**Value**: how much an actor wants a goal now. Every value is on one common scale.

**Whim**: randomness in a drive that holds for a period (a day, a mood).

**Selection**: choosing which candidate goal to pursue, by value minus estimated
cost.

**Method**: a way to achieve a goal, given its subgoals. Methods are chained
backward from the goal.

**Advertisement**: what an object, item or activity offers, through which it
supplies methods.

**Goal chain**: the selected goal and its subgoals. It is also the explanation of
what the actor is doing.

**Commitment bonus**: extra value for the goal currently pursued, so actors do
not switch for trivial reasons.

**Routine**: a strong tendency in when an actor does what. Not a script.

**Order**: a player instruction to the player character, weighed by its
obedience drive.

**Direct control**: the secondary mode in which the player character carries out
the player's commands without weighing them.

## Interactions

**Proposal**: what one actor offers another, such as a greeting, a gift, a trade
or an invitation. The target decides whether to accept.

**Shared activity**: a world object hosting a voluntary or coordinated
interaction, with roles, an anchor and a lifecycle.

**Role**: a slot in a shared activity or an object, with conditions on who can
fill it.

**Group**: a lasting set of actors, such as a pack or a family. Deferred.

## Items and bodies

**Item kind**: the content definition of an item: its tags, uses and effects.

**Use**: something that can be done with an item, such as consume, equip or plant.

**Effect**: a typed mechanic a use applies, such as changing a meter or adding a
status.

**Tag**: a word shared by items, body rules and preferences.

**Body rule**: a modifier from species or traits that changes what an effect
actually does to an actor.

**Preference**: a bonus or penalty in a drive, keyed by tags.

## Knowledge

**Shared knowledge**: the rules of the world as a culture or species believes
them. It can be wrong.

**Fact**: something about this particular world: what is where, who was where.
Perceived or told, recorded with when it was learned, and forgettable.

**Genesis**: the creation of the world. It may give actors knowledge as a
simulated past.

**Belief**: what an actor holds true: shared knowledge plus facts. It may
differ from the truth.

**Truth**: the actual state of the world. The simulation applies it; actors never
read it.
