# Context

The project's vocabulary. One meaning per word, one word per meaning. This is a
glossary only: rules are in [INVARIANTS.md](INVARIANTS.md) and design is in
[docs/](../docs/).

## Time

**Tick**: the smallest step of the simulation. Everything happens in whole ticks.

**Calendar**: days and seasons, derived from the tick count.

**Frame**: one image drawn. Frames and ticks are independent.

## Space

**Tile**: one cell of the world grid.

**Chunk**: a square block of tiles, stored and loaded as a unit. A storage
concept, never a rendering one.

**Wrap**: the world joins its east and west edges; north and south are edges.

## Entities

**Entity**: anything with an identity in the world, such as an actor, an object
or a unique item. Identified by a generational ID.

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
