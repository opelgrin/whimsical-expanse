# whimsical-expanse

A chill, sandbox 2D fantasy life sim and RPG with autonomous actors, in Rust.
This is a working name. The project is at milestone M0 (see the [roadmap](dev/ROADMAP.md)).

## Documentation

At the start of a session, read the [documentation map](dev/README.md), then the
status, the vision, the glossary and the invariants it points to. Follow the map's rules on
where information goes and which source has authority.

## Working rules

- Prefer the standard library and our own small code to new crates.
  Justify every dependency (see [architecture](dev/ARCHITECTURE.md#technology)).
- Follow the [style guide](dev/STYLE.md). Before committing, run `cargo fmt`,
  `cargo clippy --workspace --all-targets` and `cargo test --workspace`.
- Follow the [testing principles](dev/TESTING.md): every test names the bug it
  catches, builds what it relies on, and never depends on defaults. Run
  `cargo mutants` on your changes before opening a pull request.
- Never commit to `main`: work on a branch and open a pull request.
- Examples in design documents are illustrative, not agreed content.
