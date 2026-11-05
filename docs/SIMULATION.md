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
- a weight, a base value and a stack size;
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

- Ordinary items are **values** in an inventory: a kind, a quantity and a small
  amount of state that splits stacks. **Quality** is included.
  **Spoilage** is deferred.
- Only unique or stateful items (a named sword, a letter) are entities.

### Items and decisions

- **Goals concern effects, not items.** A drive proposes "be nourished", not
  "eat bread". Methods are indexed by effect, with the actor's body rules
  applied. The planner never scans the item catalogue.
- **Tools have no value of their own.** A tool is a precondition of a method and
  gets its value through the goal chain. Nobody acquires things in advance
  unless a drive asks for it ("keep a food reserve", "be well equipped").
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
