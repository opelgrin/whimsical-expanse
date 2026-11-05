# AI vision

How the actors of the [world](VISION.md) decide what to do. This document holds
requirements and the agreed direction. Mechanisms not yet decided are listed as
[open questions](#open-questions), not as commitments.

## Context

Every actor is autonomous: villagers, animals, monsters and the player
character. Each pursues goals and acts without player input. In principle the
world could run with no player at all, and actors would go on living.

The inspirations are The Sims, RimWorld and Dwarf Fortress: games where
interesting behaviour emerges from autonomous characters.

## Requirements

1. **Believable, not optimal.** Searching for optimal behaviour is a trap.
   Actors should act plausibly given what they know.
2. **Explainable.** "Why is this actor doing this?" is a player feature
   (see the vision's first pillar) and a debugging tool. Every decision must be
   explainable in human terms, as a chain of goals back to what motivated it.
3. **Content-agnostic.** The engine never names a goal, a drive, an actor kind
   or an object. It assumes no particular need: an actor may have no
   physiological needs at all.
4. **One common value scale.** Every candidate goal is valued on the same scale
   so that any two can be compared.
5. **Parallel from the start.** Actors decide in parallel, and the results are
   committed in a deterministic order. This is designed in, not retrofitted.
6. **Bounded cost.** Each decision has a bounded cost. The same AI runs at a
   reduced frequency far from the player and must tolerate a variable time
   between updates.
7. **No hidden access to the truth.** Actors decide from what they know: what
   they perceived (vision cones, occlusion) and remember, what they were told,
   the shared knowledge of the world's rules, and what genesis gave them. They
   never read the world's true state. See [knowledge](SIMULATION.md#knowledge).
8. **Player orders are not special.** Orders reach the player character through
   the same decision process as everything else. Direct control is the one,
   deliberately secondary, exception (see [player orders](#player-orders)).
9. **Danger and death early.** Fleeing, fighting and dying with a plan
   half-done are handled from early on, because they expose design flaws.

## Direction

### Drives propose goals

Anything an actor wants comes from a **drive**. A drive is content. It reads the
actor's beliefs and proposes **candidate goals**, each with a value: how much
the actor wants it *now*.

| Kind of motivation | Example drive |
|---|---|
| Body | Hunger reads a meter and proposes "be nourished". |
| Danger | Safety reads perceived threats and proposes "be out of sight of X". |
| Role or routine | A farmer's work drive values "field tended" in the morning; a guard values "intruders gone". |
| Social | Reads relationships and proposes "chat with Y". A request from Y is valued by how much the actor likes Y. |
| Aspiration | A long-term, low but steady drive: "own a sword", "befriend the blacksmith". |
| Obedience | Values the player's orders (player character only). |

Physiological needs are *one way* to write a drive. They are not the model.

**Value comes from the present situation, not a forecast.** A drive values a goal
from the actor's current state and beliefs. It does not project meters or
anything else into the future. This is the main lesson of the
[previous project](#lessons-from-etendu), and it is also how The Sims values
its choices.

**Whims.** Drives carry some randomness that stays coherent over time: it is
drawn once per period (a day, a mood) rather than at every evaluation. "Today I
fancy fishing" is believable; flipping between goals at every evaluation is not.
Since actors decide in parallel, random draws come from streams derived from the
seed, the actor and the period, never from one shared generator. That keeps them
reproducible whatever the thread scheduling.

Personality and traits weight drives. Calibrating drives against each other is
content and tuning work, supported by tooling that shows candidate goals and
their scores.

### Goals are conditions or activities

A **goal** is either:

- **a condition** on the actor's beliefs that can be checked: "has water", "is
  at home", "X is dead"; or
- **an activity** to complete: "chat with Y for a while", "rest until morning",
  "wander". An activity defines its own end, such as a duration, its partner
  leaving or a meter being full.

A goal says *what*, never *how*.

### Choosing and achieving

The decision has two layers that cooperate.

**Selection** picks which goal to pursue:

1. Collect candidate goals from all drives.
2. Take the top few by value.
3. Ask the achieving layer for a bounded estimate of each one's feasibility and
   cost.
4. Pick among the best by *value − cost*, with weighted randomness among the top
   choices, as The Sims does. The randomness comes from the actor's own random
   stream, so runs stay reproducible.

Selection never simulates what happens after a goal is reached.

**Achieving** works out how, through **methods chained backward**:

- A method states: "this achieves goal G, provided subgoals P are met first".
  Watering a plant needs water; getting water may need a bottle.
- Methods come from content and from what **advertises what it offers**:
  objects (smart objects), items (through their effects, see
  [items](SIMULATION.md#items)) and running activities. A new object that offers
  "has water" is used by every actor and every chain that needs water, with no
  new code.
- Planning chooses a method for a goal and recurses into its subgoals, with a
  bounded depth and breadth.
- The resulting goal chain *is* the explanation of the behaviour.

This deliberately gives up free GOAP-style search over primitive actions. Its
occasional surprising improvisation is not worth the cost, the opacity and the
debugging pain. Emergence comes from composing authored methods and object
advertisements.

### Routines

Routines are an intended feature. They give structure to actor behaviour: a
villager's day has a shape. Whims and weighted selection add variety, so a
routine is a strong tendency, not a script.

### Persistence

Actors stick to what they are doing, unless something matters more:

- the current goal gets a commitment bonus when goals are re-selected;
- re-selection happens on meaningful events or periodically, not every tick;
- an interruption suspends the current chain rather than discarding it, so it
  can be resumed.

The structure that holds suspended goals (a queue, a stack or something else)
is open.

### Player orders

Orders become candidate goals proposed by the player character's obedience
drive, valued high but not absolutely. A starving or terrified character may
postpone or refuse an order, as in The Sims. This follows from "your character
is just another actor".

The cost is that players may not see why an order was ignored. The answer is
tuning plus clear communication: the UI shows what the character is doing,
why, and why an order was set aside.

**Direct control** is the one exception. It is a secondary mode, entered by
holding a modifier key. It lets the player force an action, for example to get
the character out of a bad situation. The character carries out direct commands
without weighing them, and autonomy resumes as soon as the player stops.

### Interactions between actors

Examples in this section show what the engine must be able to support. They are
not agreed future content.

**Adversarial interactions need no shared mechanism.** A predator pursues
"prey is dead". The prey perceives a threat and its own safety drive proposes
fleeing. Each side decides independently.

**No actor decides for another.** An actor can only *propose*: a greeting, a
gift, a trade, joining an activity. The target evaluates the proposal through its
own drives at its next decision, and accepts or refuses based on its own state.
A proposal requires perception or speaking range: there is no telepathy.
Communication over distance (letters, messengers) could come later as content.

**Shared activities are world objects.** Voluntary or coordinated interactions
(a conversation, a shared meal, a trade, a cooperative hunt) take place inside a
shared activity. It has:

- a content-defined type;
- roles or slots with conditions, such as two to six talkers, or one buyer and
  one seller;
- an anchor, which is a place or a participant;
- a lifecycle: forming, active, ending;
- the internal logic of its type, such as turn-taking, offer and counter-offer,
  or role assignment.

**Joining and leaving use ordinary decisions.**

- "Take part in activity A in role R" is an activity goal like any other.
- A running activity **advertises itself** like a smart object: actors who
  perceive a group chatting see an offer of social value.
- A participant leaves when something else outranks staying.
- An activity ends when it falls below its minimum participants or its own
  logic completes.
- If two actors claim the last slot in the same tick, ordered commit decides.
  The loser's attempt fails and it decides again.

**Roles can make objects advertise.** A shop offers "buy here" only while a
seller occupies its counter role.

**Coarse resolution.** An activity type may resolve itself coarsely when far
from the player. For example, a conversation becomes a relationship change
without simulating each exchange.

**The player character answers proposals autonomously**, like any actor. The
player is notified and can override.

**Persistent groups** (packs, herds, families, a militia) are a separate,
longer-lived concept, with membership and perhaps a leader or territory. A
group can start activities, such as a pack launching a hunt. This is deferred.

### Social relationships

There is much to learn from The Sims here; the previous project had no social
interactions. Starting points:

- relationships as a few separate tracks (such as friendship and romance) rather
  than one number;
- interactions that are available or not depending on relationship, mood and
  traits;
- the target decides acceptance from its *own* state, and success or rejection
  changes the relationship.

## Lessons from Etendu

- **Valuation, not search, was the hardest part.** Everything was scored as
  need growth projected over time. Valuation that needs a forecast becomes a
  second planner.
- **Do not build around needs.** An engine shaped around relieving needs
  (hunger, energy) does not fit actors without them, and a formula unifying
  need and non-need goals did not work in practice.
- **Opacity is expensive.** Much time went into answering "why did actor X do
  this and not that?", mostly because of free search and unusual valuation
  metrics.
- **Ranking by value per unit of time rewards longer plans.** Keep the value and
  cost combination simple.
- **Narrow early content leaks into design.** A long period with one kind of
  actor eating one kind of food shaped assumptions, the planner and its
  performance targets. A [diverse representative set](VISION.md#representative-content)
  is used from early on.
- **Do not delay mortality.**
- **Parallelism must be designed in**, not retrofitted.

## Open questions

- **Persistence structure:** what holds suspended goals (a queue, a stack or
  something else)?
- **Persistent groups:** membership, leadership, and how a group starts and
  staffs activities. Deferred.
- **Routines:** how they are expressed (as drives valued by time of day, as
  schedules, or something else), and how strongly they bias choices.
- **Re-selection triggers:** which events cause an actor to reconsider?
- **Value scale:** what the common unit means, and how content authors calibrate
  drives against each other.
- **Method authoring:** what content declares, what objects advertise, and how
  deep chains may go.
- **Far-away actors:** what changes in decisions when an actor is updated rarely.
  This is the decision side of the [coarse simulation](VISION.md#open-questions)
  question, which holds the options.
