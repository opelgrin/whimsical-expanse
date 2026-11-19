# Vision

What the game is meant to be, readable without the code. How actors decide is in
[AI.md](AI.md), the world's rules are in [SIMULATION.md](SIMULATION.md), and how
it is built is in [dev/](../dev/README.md).

> Working project name: *whimsical-expanse*. Not the final title.

## The idea

A chill, sandbox fantasy RPG in 2D. You follow a single character living in a
large, procedural world of biomes, settlements, wildlife and dungeons. The
character lives their own life: they eat, work, make friends, wander. You can
give them orders or take direct control at any time, but leaving them alone is
a perfectly valid way to play.

The inspirations are The Sims (autonomous characters, social relationships,
indirect control), Stardew Valley (farming, a cozy rhythm) and RimWorld (pausable,
order-driven play, emergent stories). The main source of wonder is watching the
world evolve through its autonomous inhabitants.

This is a hobby project. It is open source, non-commercial and will not be
published on storefronts.

## Pillars

These are aspirational. They set the direction, not a feature list.

1. **A world worth watching.** Autonomous behaviour only delights when it can be
   seen and understood. Intentions, moods, relationships and notable events are
   made visible to the player. Legibility is a player feature, not just a
   debugging aid.
2. **Your character is just another actor.** The player character runs the same
   AI as everyone else. Orders are proposals the character weighs; a starving
   or terrified character may postpone one. Direct control is the exception:
   the player forces actions, for example to save the character from a bad
   situation. When left alone, the character lives their own life.
3. **Low input, low pressure.** The game is mostly mouse-driven, with pause and
   speed controls. Danger depends on place: settlements and their surroundings
   are relatively safe, the wilds more exposed, and dungeons and hostile areas
   dangerous. A player can live a whole life without ever fighting.
4. **A living ecology, not a backdrop.** Villagers, animals and monsters all
   pursue their own goals. A wolf hunting a deer and a farmer going to market
   come from the same machinery. The world keeps living away from the player.
5. **Breadth over depth.** Farming, building, combat and trade are each "lite".
   The richness comes from how they interact through autonomous actors, not from
   the depth of any one system.

## Growing over time

The game starts simple and grows, the way No Man's Sky or The Sims grew
through years of updates. Content and behaviour can become very rich, but not
everything needs to exist before it can be called a game.

That implies:

- **Realistic milestones.** Each step must be small enough to finish. Ambition
  lives in this document, not in the next milestone.
- **Architecture open to extension.** New content (actors, items, goals,
  interactions) and new mechanics should arrive without reworking the core.
  The AI engine knows no concrete content.
- **Representative content early.** A small but *diverse* set of content guards
  against designing around one narrow case (see
  [below](#representative-content)).

## Decisions so far

| Topic | Decision |
|---|---|
| Player | One character. No household. |
| Death | The player character can die. The next character is a newcomer the player creates, who arrives in the same world, which goes on. Whether game over stays an option is an [open question](#open-questions). Mortality comes early in development. |
| Generations | Aging, descendants and kids are not planned initially. If kids come later, think Fable II/III, not The Sims. |
| Population | Renewed, so the world never empties. A dead actor leaves a vacancy, such as a village without its blacksmith, which a newcomer fills after a random delay: a situation for a while, ended at a time the player cannot predict. Animals are born. People arrive from outside the world: settlers to fill vacancies, and travellers who come with a purpose, stay a while and leave. Nothing arrives, is born or happens because the player is near. See [population](SIMULATION.md#population). |
| World | The surface is finite but large; exploration should be engaging. It is a band of latitude from a larger world that the game never names. It wraps east–west (a cylinder), and its climate follows latitude. When starting a game, the player will choose generation settings, such as the world's size and its range of latitude. |
| Spaces | Building interiors and dungeons are separate spaces, as in 2D Zelda games and Stardew Valley, not areas of the surface under a roof. Passages, such as doors, stairs and cave mouths, connect the spaces. The camera shows one space at a time, and vision never crosses a passage. All spaces share one clock and the same rules; each has its own size. |
| Dungeons | Persistent. A monster killed stays dead and loot taken stays taken. A cleared dungeon fills up again only when actors move in, not by a reset. |
| Terrain and perception | The world has obstacles. Vision is a cone whose range and width vary by species, is blocked by obstacles and terrain, and may shrink at night. |
| Map | Fog of war: the map shows what the player character has discovered. |
| Tone | The spirit and ambiance of Stardew Valley, not dark fantasy. A rougher setting (for example with crime in settlements) could be a separate game mode the player opts into, if ever implemented. |
| Danger | Graded by place. Settlements and their surroundings, such as farmland, are relatively safe, and guards may help. The deep forest and other wilds expose the character more. Dungeons and hostile areas are dangerous. |
| Time | A calendar with days and seasons. Seasons affect the world, such as farming and the look of plants and trees (the asset pack has seasonal variants). The pace is relatively slow, to suit the chill tone. The length of a day is configurable; its default is an [open question](#open-questions). |
| Controls | Primarily mouse. Ordinary clicks give **orders**, even with the character selected. Holding a modifier key (such as Alt, or its Mac equivalent) switches to **direct control**, which the character follows without weighing it. Direct control is deliberately secondary. The camera pans with the screen edges and WASD (ZQSD on AZERTY, i.e. physical key positions). |
| Off-screen | Full simulation near the player character. Far away, the same simulation runs less often (throttling). The world must actually progress elsewhere, not just look as if it did. Distance is measured through passages, so the square outside a tavern is near a character inside it. Throttling still costs more as the population grows; a separate low-fidelity path is an experiment for later, if that cost calls for it. |
| Combat | RimWorld-like: the character fights semi-autonomously, the player gives tactical orders, and the game can always be paused. |
| Motivation | Sandbox, with aspirations as a likely bridge between watching and playing. The form is an [open question](#open-questions); it can start barebones. |
| Economy | Very simple at first (buy, sell, a few prices). May grow if interesting. Never an economic simulation. |
| Building | Minimal at first. Less central than in The Sims. |
| Art | Based on a purchased asset pack that may be modified and extended. It cannot be redistributed, so the open-source game comes with abstract placeholder art instead. The placeholders must be readable enough to play with, not only to debug with. |
| Platforms | Linux, Windows and macOS (Apple silicon only). |
| Technology | Rust. Technical choices are recorded in the developer documentation. |

## Non-goals

- Not a combat or action game. No reflex-based gameplay.
- Not dark fantasy, at least not in the main game.
- Not a city builder or a colony manager. You follow one character.
- Not an economic simulation.
- Not a framework first. The simulation serves the game, not the other way round.
- Not commercial. No storefront releases.
- Not optimal AI. Believable beats optimal.

## Representative content

This is not a content plan. It is a deliberately varied set the engine should
support early on, so that no single case shapes the design. The set is chosen
to differ along several axes: needs versus no needs, social versus solitary,
hunter versus hunted, player-influenced versus not, and alive versus static.

| Content | Exercises |
|---|---|
| Player character | Same AI as everyone else; orders; takeover; social life; death |
| Villager (farmer) | Daily routine, multi-day work (crops), trade, relationships |
| Shopkeeper | A fixed place of work; the endpoint of a simple economy |
| Grazing animal (deer) | Physiological needs, fleeing, maybe herding |
| Predator (wolf) | Hunting a *moving* target; may hunt the player |
| Dungeon monster (skeleton) | **No physiological needs**; guards territory; hostile |
| Bandit | Humanoid like the villager but hostile; tests behaviour coming from disposition, not body |
| Crop | Changes over time without AI; part of the world simulation, not an actor |

Behaviours to cover early: eating and sleeping, farming over several days,
trading, **two-actor interactions** (a conversation needs coordination),
hunting and fleeing, fighting, dying, exploring and carrying things.

## First game

This is the smallest thing we would call a game, not the first milestone.
Technical slices come before it. *(Proposed; to be refined.)*

- One player character in a generated region: a few biomes, one village, some
  wild areas, one small dungeon.
- The character lives autonomously, takes orders and can be controlled directly.
- Villagers with routines and basic relationships with the player.
- A small farm plot; buying and selling.
- Wildlife with a predator–prey relationship.
- Simple combat in the dungeon and in hostile areas; death means game over.
- Enough legibility to enjoy watching: what an actor is doing and why.

## Open questions

- **Aspirations:** what form do they take?
- **Succession:** whether game over stays an option when the player character
  dies, and, once descendants exist, whether the player can continue as one.
- **Length of a day:** the default, given the slow pace we want. It is
  configurable in any case.
- **Leaving:** whether villagers can leave the world through passages to the
  outside, as travellers do.
- **Other worlds:** whether the game has worlds besides the surface, reached
  through passages such as portals. Spaces allow any number of them, of any
  size, not aligned with the surface.

## Heritage

Etendu is the ancestor: an earlier private, experimental and learning project
around GOAP-inspired game AI. It is not the model for this
project, nor its code base. Its lessons and some selected pieces are carried
over deliberately. One example is the tick-based simulation running partly
asynchronously from rendering. Its AI engine is not carried over.
