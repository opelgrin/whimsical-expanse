# Simulation

The rules of the [world](VISION.md): items, their effects, bodies and actor
knowledge. How actors *decide* is in [AI.md](AI.md).

Examples show what the engine must be able to support. They are not agreed
content.

## Items

### Three questions, three owners

| Question | Owner | Nature |
|---|---|---|
| What does this item do, in general? | the **item** (content) | objective, the same for everyone |
| What does it actually do to *me*? | the actor's **body** (species, traits) | objective, specific to the actor; applied by the simulation |
| How much do I want that? | the actor's **mind** (drives, preferences) | subjective; valued by the AI |

The item advertises its objective effects, the body modifies what actually
happens, and the mind values the result. The engine knows none of the concrete
items, species or preferences.

*Example:* raw meat offers "consume: nourishment +30, tags {meat, raw}", and
bread offers "consume: nourishment +20, tags {grain, cooked}". A carnivore's body
gets nothing from grain. A human's body turns raw meat into less nourishment
plus an upset stomach. A villager's preference adds a bonus for cooked food. The
wolf picks the meat and the villager the bread, through effects, tags, body rules
and drive values alone.

### Definition

An item kind (content) has:

- **tags**, a vocabulary shared by items, bodies and drives;
- a weight, a base value and a stack limit;
- **uses**, typically one to three: consume, equip, plant, read, give…
- for each use, a duration and **effects**, typically one to four.

### Effects

Effects are a closed set of typed mechanics, extended in code when a genuinely
new mechanic is needed. A first list:

- change a meter (which meters exist is content);
- add a timed status;
- change a stat while equipped, such as attack, armour, speed or vision range;
- transform or spawn, such as a seed becoming a crop or raw becoming cooked at a
  station;
- social: a gift's effect lands on the recipient and is valued by their tastes;
- knowledge: a map or a book reveals places or teaches methods.

### Bodies

Body rules are tag-based modifiers: they block or scale an effect, or add or
replace one, when the tags match. They come from species and traits, and cover
diets, items usable only by some actors, allergies and resistances.

**Equipment slots** are defined per species in content, not fixed by the engine.

### Preferences

Preferences belong to the mind: tag-based bonuses or penalties in drives ("likes
sweet", "hates fish"). They can carry whims ("fancies something sweet today").

### Representation

An **item kind** (above) is content: immutable and shared. In the world, **items
are entities**.

- **One identity model.** Every item, or stack of identical fungible items, is
  an entity with its own generational ID. Actors, world objects and items share
  the same storage, reference and lifecycle rules.
- **Stacks.** Identical items of a stackable kind share one entity with a
  quantity. Taking part of a stack splits off a new entity; combining identical
  stacks merges them and retires one ID. An item with a stack limit of one, or
  with distinct per-item state, has a quantity of one and never merges.
- **Where an item is.** An item lying in the world has a position. An item in a
  container or carried by an actor is held by that entity and has no position of
  its own. No wrapper entity is needed for a single item on the ground.
- **Per-item state.** Quality is expected; spoilage and durability are deferred.
  Uncommon per-item capabilities (a name, an owner, an inscription, durability)
  live in typed stores keyed by the item's entity ID, like any other entity
  capability.

Why entities with stack counts rather than pure value structs in inventories:

- ground items, equipment, containers and world objects all use the same entity
  IDs, stores, stale-reference handling and commit conflict resolution;
- any belief, goal or mechanic that refers to one specific item ("recover my
  sword", "pick up that apple") uses an ordinary entity ID instead of a second
  identity system;
- stack counts on identical goods keep farms, stockpiles and coins from creating
  an entity per unit.

### Items and decisions

- **Needs ask for effects, not specific items.** Hunger proposes "be nourished",
  not "eat bread"; a gift proposes social value for the recipient. Items supply
  methods through the effects they advertise, with the actor's body rules and
  preferences applied, so new items work without changing drives or scanning the
  item catalogue.
- **Goals and subgoals can still ask for items.** Even though every item in the
  simulation is an entity, actors usually plan and remember goods by effect, tag
  or kind ("needs a bottle", "keep a food reserve", "deliver grain"), and bind
  to a specific item's entity ID only when targeting a concrete stack to act on
  or when that individual item matters ("recover my sword").
- **Tools have no value of their own.** Unless a drive values having an item
  directly ("keep a food reserve", "own a sword"), an item gets its value only
  through the goal chain that needs it.
- **The mind values believed effects; the simulation applies true effects.**

## Knowledge

### Rules and facts

- **Rules of the world** (what an item does, how farming works, which creatures
  are dangerous) come from **shared knowledge**, authored per culture or
  species. Actors know them by default. Shared knowledge is authored
  independently of the true rules, so a culture can be wrong.
- **Facts about this world** (where things are, where someone was, what a chest
  holds) are only **perceived or told**.
- **Genesis exception:** at creation, initial knowledge may be derived from the
  truth *as a simulated past*. For example, a villager who grew up here knows
  their home area, their neighbours and the local shop. Nothing tops up
  knowledge from the truth afterwards.

**Execution checks the truth.** An attempt based on a wrong belief fails, the
belief is corrected, and the actor decides again.

### Kinds of knowledge

| Kind | Content | Stored | Forgotten |
|---|---|---|---|
| Self | own body, meters, inventory, skills | actor state | never; not treated as knowledge |
| Shared rules | item effects, known methods, dangers | per culture or species | no |
| Map | discovered terrain and topology, coarse | per actor, compact | no, or very slowly (deferred) |
| Things | what is where, always with when it was seen | per actor, bounded | yes, evicted |
| People | known actors: relationships, learned traits, last known place and time | per actor, bounded but sticky | rarely; close relations never |
| Memories | notable events | per actor, bounded | yes (later feature) |

- **Facts take slots.** Memory has a capacity and an eviction score combining
  importance, recency and reinforcement. This keeps each actor's cost bounded.
- **Variation by species** is in parameters (capacity, what counts as
  important) and in shared knowledge.
- **Every fact records when it was seen**, so actors can treat old information
  as less reliable.
- **Absence is perceived:** seeing an empty bush updates the belief.

### The player

- The map shows only what the player character has discovered (fog of war).
- The player can order the character to go somewhere the character does not
  know. The order then *gives* the character that knowledge: the player is a
  source of information.

### Later

- **Learning:** discovering item effects by trying them, learning recipes. These
  would be small per-actor differences on top of shared knowledge.
- **Communication** (an intended feature): facts told by others, such as
  gossip, rumours or news, as an effect of conversations. Facts record whether
  they were seen or told.
