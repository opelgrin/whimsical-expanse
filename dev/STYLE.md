# Rust style

Conventions for writing code in this project. The aim is code that is clean,
idiomatic and easy to read. Tooling enforces what it can, and this document
covers the rest.

## Tooling

- **Formatting:** `rustfmt` with default settings. CI enforces it.
- **Lints:** declared once in the workspace `Cargo.toml`, and every crate inherits
  them with `[lints] workspace = true`. CI runs clippy with `-D warnings`, so every
  warning fails the build.
  - `clippy::all` and `clippy::pedantic`, including the integer cast checks,
    which matter for tile and integer simulation arithmetic.
  - `unwrap` is discouraged outside tests. Use `expect("why this cannot fail")`
    so the assumption is written down.
  - No `dbg!`, `todo!` or `unimplemented!`. Libraries do not print; they log.
- **Project rules in `clippy.toml`:**
  - the standard `HashMap`/`HashSet` are banned (see [collections](#collections));
  - reading the wall clock is banned, except for real-time pacing and
    presentation;
  - starting threads by hand, and the parallel operations whose result can
    depend on the thread count, are banned (see [parallel code](#parallel-code)).
- **Toolchain:** pinned in `rust-toolchain.toml`, so local builds and CI use the
  same compiler.

## Exceptions to lints

Use `#[expect(lint, reason = "...")]`, never `#[allow]`. The reason is mandatory,
and `expect` warns once the exception is no longer needed. Put the exception on
the smallest item possible.

## Collections

- Use `FixedHashMap`/`FixedHashSet` from `expanse-core` for lookups. Their
  iteration order is reproducible, but arbitrary.
- When iteration order affects an outcome, use `BTreeMap`/`BTreeSet` or sort
  explicitly.

## Simulation code

- **Integers for simulation state.** Positions are tiles; time is a tick count
  (`u64`). Floats belong to presentation. Any exception must be justified.
- **Explicit arithmetic.** Where overflow is possible, choose its behaviour
  explicitly (`checked_`, `saturating_`, `wrapping_`). Convert between integer
  types with `From`/`TryFrom`, not `as`.
- **No hidden truth.** Decision code takes the actor's knowledge, not the world.
  Function signatures should make that visible.
- **Expected failures are values, not panics.** An actor finding an empty chest
  is a normal outcome. Panics are for broken invariants only.

## Parallel code

Parallel work uses rayon. Import it with `use rayon::prelude::*`. This is the
only glob import outside test modules: the prelude holds only the traits that
add the parallel methods, and clippy's `wildcard_imports` exempts preludes.
The simulation thread is the only thread started by hand. Library crates run
in whatever pool they are called from; only the binaries size the global pool.

- **Parallel by default.** A pass over every tile, chunk or actor is written
  parallel from the start. A pass that stays sequential says why in a comment:
  it depends on order, or its work is too small to gain from it.
- **Compute, then commit.** When a pass depends on order, compute what depends
  only on each element in parallel, then apply the results sequentially, in a
  defined order. The obstacle pass in `worldgen` is an example.
- **The result never depends on the split.** `collect` and `unzip` keep the
  order of the items, so they are always safe. A reduction (`sum`, `reduce`,
  `fold`) is allowed only for an associative operation, such as integer
  arithmetic that cannot overflow or that wraps, bitwise operations, or
  `min`/`max` with a total order. It needs an `#[expect]` that says so.
  Floating-point sums and products are not associative: collect the values,
  then reduce them sequentially.
- **The result never depends on which thread finishes first.** Use
  `find_first`, not `find_any`. Do not use `par_bridge`. Do not write to shared
  state from `for_each`, such as pushing to a list behind a lock or taking IDs
  from a counter.
- **Sorts have a total order.** Break ties with a stable key, such as the
  row-major index, so that the sorted order depends only on the values.

## Unsafe

`unsafe` is denied in every crate. A justified exception needs an `#[expect]`
with its reason and a `// SAFETY:` comment explaining why the block is sound.
Prefer a well-known crate that does the unsafe part with compile-time checks, as
`bytemuck` does for GPU data.

## Readability

- **Names say what things are,** using the [glossary](CONTEXT.md)'s terms. No
  abbreviations except well-established ones (`id`, `rng`).
- **Small units.** A function does one thing. When a module grows past a few
  hundred lines, it is probably doing several.
- **Comments explain why,** not what. Public items get doc comments that state
  their contract.
- **Tests sit next to the code** (`#[cfg(test)] mod tests`). Cross-crate
  scenarios go in a crate's `tests/` directory.
- **Stand-ins say so.** Code written only to fill a gap until later work
  replaces it opens its doc comment with `Stand-in:` and names what replaces
  it, such as "Stand-in: replaced by the calendar types in M1 item 6." Searching
  for `Stand-in:` then lists every such item, and the work that replaces one
  deletes it.
- **Describe roles, not content.** Code and comments in the engine never name
  particular content (see the [invariants](INVARIANTS.md)).
