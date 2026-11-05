# Invariants

The rules every change preserves. They change rarely, and only by explicit
agreement, never as a side effect of an implementation. The structure that
follows from them is in [ARCHITECTURE.md](ARCHITECTURE.md).

## Reproducible, clock-free, headless simulation

**Rule:** Given the same binary, content, configuration, seed and recorded inputs,
the simulation produces the same results, tick for tick. Recorded inputs include
player commands, each tagged with the tick it applied at. The simulation never
reads the wall clock to decide an outcome or to stop a computation. Its results
never depend on thread scheduling. The simulation and AI crates do not depend on
windowing, rendering or audio.

This is not a promise across binaries or platforms.

**Rationale:** Reproducible runs make failures explainable and experiments
comparable. A world that lives without a player must also run without a window.

## Deterministic iteration and randomness

**Rule:** Nothing that affects outcomes iterates a container whose order varies
between runs. The standard `HashMap` and `HashSet` with their default, randomly
seeded hasher are not used at all; maps with a fixed hasher are used instead.
When iteration order affects an outcome, the order is a meaningful one: an
ordered container or an explicit sort. Randomness comes from streams derived from
the seed and a stable identity, such as an actor and a tick or period. Parallel
work never draws from a shared generator.

**Rationale:** The standard hash maps are seeded randomly per process, and a
shared generator consumed by several threads depends on scheduling. Both break
reproducibility silently.

## Compute, then commit

**Rule:** Parallel phases only read world state. Their results are applied
afterwards, sequentially, in a defined order such as ascending entity ID.
Conflicts (two actors claiming the same thing) are settled during that commit.

**Rationale:** This is how parallel decisions coexist with determinism. It also
means no actor ever sees another's half-applied decision.

## Presentation never affects the simulation

**Rule:** The camera, rendering, the UI and what is loaded for display never
change what exists or what happens. The level of simulation detail (full near
the player, coarser far away) is a simulation rule. It depends on simulation
state, such as the player character's position, never on the camera.

**Rationale:** Moving the camera must not become a hidden input to the world.

## Simulation steps tolerate variable elapsed time

**Rule:** A simulation step takes the elapsed time since that thing was last
updated. It never assumes exactly one tick has passed. Where state can be
derived from time (growth, decay), it is computed when needed rather than
advanced every tick.

**Rationale:** This keeps a coarser, less frequent simulation far from the
player possible, whichever approach is chosen. It also avoids ticking things
that nobody looks at.

## Actors have no hidden access to the truth

**Rule:** An actor's decisions use only its own knowledge. That knowledge comes
from:

- what it perceives and remembers;
- what it is told;
- the shared knowledge of the world's rules;
- what genesis gave it as a simulated past;
- the player's orders, for the player character.

No lookup into the world's true state may supply, refresh or correct that
knowledge, even as a fallback. Missing knowledge stays missing. Execution checks
attempts against the truth, and a failed attempt updates the actor's beliefs.

**Rationale:** Beliefs that differ from the truth produce believable behaviour,
and learning from experience. A silent correction removes both.

## The engine is independent of content

**Rule:** Simulation mechanics and AI work on identities, capabilities,
parameters and effects supplied by content. They never special-case a
particular item, actor kind, drive or goal. They assume no particular drive,
including physiological needs. Mechanics are typed code: content
instantiates them, and a genuinely new mechanic extends the typed code. The AI
engine works through an abstract model and never inspects content or
mechanic-specific representations.

Not naming any content is not enough to make a mechanic general. It must not
rely on a property that only the content it was developed with has.

**Rationale:** New content must not multiply special cases across the code.
Shared mechanics let new drives, species and items compose.

## Licensed assets stay out of version control

**Rule:** The licensed asset pack, and anything derived from it, never enters the
repository. The game builds, runs and passes its tests with the placeholder
assets alone.

**Rationale:** The pack's licence forbids redistribution, and anyone cloning the
open-source repository must still have a working game.
