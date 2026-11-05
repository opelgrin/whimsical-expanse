# Testing

We value a small number of meaningful tests over a large number of weak ones.
Every test costs CI time and maintenance, and a test that breaks for no reason
teaches people to ignore failures. Less is more.

## Principles

### Every test names the bug it catches

Before writing a test, answer this: *what realistic mistake would make this test
fail, and does no other test already catch it?* If there is no good answer, do
not write the test. If an existing test has no good answer, delete it.

State the answer in the test's doc comment when the test's name does not make it
obvious.

**Bug-driven tests come first.** Every real bug that is fixed gets a regression
test at the right level: it has already happened once.

### Tests build what they rely on

- A test sets up its own world, content and parameters explicitly, through small
  builders. It never relies on the game's default configuration or content.
- Assertions are written relative to those parameters: "takes `walk_ticks`
  ticks", never "takes 8".
- Changing a default or retuning content must never break a test. The default
  content exists for running the game and for diagnostics.

### Prefer properties to exact values

Many important guarantees are properties, and properties survive tuning and
refactoring:

- determinism: two runs from the same seed are equal;
- conservation: items are not created from nothing;
- knowledge: an actor never believes a fact it did not perceive, was not told and
  did not receive at genesis;
- wrapping: distances are symmetric across the seam;
- paths are valid, connected and passable.

Check a property over many generated cases by looping over seeds with our own
deterministic RNG. No property-testing crate is needed.

### Behaviour tests assert outcomes, not traces

An AI or simulation test states an outcome: "a hungry actor that knows where food
is eventually eats", or "no actor pursues a goal targeting an entity that no
longer exists". It does not assert exact routes, ticks or decision traces. Those
break with every policy change and get updated blindly.

### No tautologies

The expected value must come from an independent source: a value worked out by
hand, a much simpler reference implementation, or a property. Recomputing it with
the code's own formula proves nothing.

### What we do not test

- trivial accessors, constructors and derived traits;
- what the type system already guarantees;
- third-party crates;
- pixels. Rendering math (camera transforms, interpolation) is pure and testable,
  but what an image looks like is checked by eye.

## Levels

| Level | What | Where it runs |
|---|---|---|
| Unit | pure logic: arithmetic, wrapping, selection over hand-built candidates | every CI run |
| Scenario | a small hand-built world and a behaviour, such as fleeing a perceived threat | every CI run |
| Simulation | determinism over a short run; one smoke run of the representative content that checks properties only (no panic, no stuck actors) | every CI run, kept short |
| Diagnostics | many seeds and long runs, looking for looping or stalled actors | manually or on a schedule |
| Mutation | `cargo-mutants` | on each pull request's changes, and occasionally in full |

Tests sit next to their code (`#[cfg(test)] mod tests`). Cross-crate scenarios go
in a crate's `tests/` directory.

## Speed and reliability

- The whole suite stays fast, well under a minute. A slow test is a cost every
  change pays.
- No sleeps and no wall clock in tests. The simulation is deterministic, so a
  flaky test is a bug: fix it or delete it immediately.

## Mutation testing

[`cargo-mutants`](https://mutants.rs) makes small changes to the code and checks
that some test fails. A surviving mutant points to a missing or weak test. It is
a tool to install (`cargo install --locked cargo-mutants`), not a project
dependency.

- Run it on your changes before opening a pull request:
  `git diff main > changes.diff && cargo mutants --in-diff changes.diff` (keep
  the diff file outside the repository, or delete it afterwards). Run it in full
  occasionally with `cargo mutants --workspace`.
- **Do not chase a perfect score.** Some mutants change nothing a meaningful test
  could observe, such as equivalent rewrites, `Debug` output or log messages.
  Record those in `.cargo/mutants.toml` with the reason, rather than writing tests
  just to kill them.

## For coding agents

- Do not add tests "for coverage". Justify every new test by the bug it catches.
- Prefer extending an existing scenario over adding a near-duplicate.
- Deleting a redundant, trivial or brittle test is a welcome contribution.
- Tests are code: readable, with names that state a behaviour, such as
  `hungry_actor_with_food_in_reach_eats`.
