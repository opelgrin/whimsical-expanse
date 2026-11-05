# Rust style

Conventions for writing code in this project. The aim is code that is clean,
idiomatic and easy to read. Tooling enforces what it can, and this document
covers the rest.

## Tooling

- **Formatting:** `rustfmt` with default settings. CI runs `cargo fmt --check`.
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
    presentation.
- **Toolchain:** pinned in `rust-toolchain.toml`, so local builds and CI use the
  same compiler.

Before pushing: `cargo fmt`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`.

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
- **Describe roles, not content.** Code and comments in the engine never name
  particular content (see the [invariants](INVARIANTS.md)).
