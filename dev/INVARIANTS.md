# Invariants

The rules every change preserves, each with its rationale. They change rarely,
and only by explicit agreement. This document does not describe how the system
is built ([ARCHITECTURE.md](ARCHITECTURE.md)), what the game should be
([docs/](../docs/)), or how actors decide ([docs/AI.md](../docs/AI.md)).

## Reproducibility

**Rule:** Given the same binary, content, configuration, seed and recorded inputs,
the simulation produces the same results, tick for tick. Recorded inputs include
player commands, each tagged with the tick it applied at. Nothing that varies
between runs may influence an outcome: not the wall clock, not thread scheduling,
not the order of an unordered container, and not a random generator shared
between actors or threads. Randomness derives from the seed and a stable identity,
such as an actor and a period. The simulation and AI crates do not depend on
windowing, rendering or audio.

This is not a promise across binaries or platforms.

**Rationale:** A failure that cannot be reproduced cannot be explained, and runs
cannot be compared. Hidden variation breaks reproducibility silently: a run looks
fine and differs only sometimes. A world that lives without a player must also
run without a window.

## Compute, then commit

**Rule:** Parallel phases only read world state. Their results are applied
afterwards, sequentially, in a defined order. Conflicts (two actors claiming the
same thing) are settled during that commit.

**Rationale:** This is how parallel decisions coexist with reproducibility. No
actor ever sees another's half-applied decision.

## Presentation never affects the simulation

**Rule:** The camera, rendering, the UI and what is loaded for display never
change what exists or what happens. The level of simulation detail (full near
the player, coarser far away) depends on simulation state, such as the player
character's position, never on the camera.

**Rationale:** Moving the camera must not become a hidden input to the world.

## Simulation steps tolerate variable elapsed time

**Rule:** A simulation step takes the time elapsed since that thing was last
updated. It never assumes exactly one tick has passed. State that can be derived
from time is computed when needed, not advanced every tick.

**Rationale:** A coarser, less frequent simulation far from the player must be
possible, and things nobody looks at must not be ticked needlessly.

## Actors know only what they know

**Rule:** An actor's decisions use only its own knowledge. No lookup into the
world's true state may supply, refresh or correct that knowledge, even as a
fallback. Missing knowledge stays missing. Execution checks attempts against the
truth, and a failed attempt updates the actor's beliefs.

**Rationale:** Beliefs that differ from the truth make behaviour believable and
allow learning from experience. A silent correction removes both.

The sources of knowledge are described in [docs/SIMULATION.md](../docs/SIMULATION.md#knowledge).

## The engine is independent of content

**Rule:** Simulation mechanics and AI work on identities, capabilities,
parameters and effects supplied by content. They never special-case a particular
item, actor kind, drive or goal, and they assume no particular drive.

Not naming any content is not enough to make a mechanic general. It must not rely
on a property that only the content it was developed with has.

**Rationale:** New content must not multiply special cases across the code.
Shared mechanics let new drives, species and items compose.

## Licensed assets stay out of version control

**Rule:** The licensed asset pack, and anything derived from it, never enters the
repository. The game builds, runs and passes its tests with the placeholder
assets alone.

**Rationale:** The pack's licence forbids redistribution, and anyone cloning the
open-source repository must still have a working game.
