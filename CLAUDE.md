# whimsical-expanse

A chill, sandbox 2D fantasy life sim and RPG with autonomous actors, in Rust.
This is a working name. The project is in its design phase, with no code yet.

## Documentation

At the start of a session, read the [documentation map](dev/README.md), then the
vision, the glossary and the invariants it points to. Follow the map's rules on
where information goes and which source has authority.

## Working rules

- Prefer the standard library and our own small code to new crates.
  Justify every dependency (see [architecture](dev/ARCHITECTURE.md#technology)).
- Never use the default `HashMap`/`HashSet` in anything that affects simulation
  outcomes (see [invariants](dev/INVARIANTS.md)).
- Examples in design documents are illustrative, not agreed content.
