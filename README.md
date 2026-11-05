# whimsical-expanse

_Working name._

A chill, sandbox fantasy RPG in 2D. You follow a single character living in a
large, procedural world of biomes, settlements, wildlife and dungeons. The
character lives their own life. You can give them orders, or simply watch the
world evolve through its autonomous inhabitants.

It takes inspiration from The Sims, Stardew Valley and RimWorld, and is written
in Rust.

> [!NOTE]
> The project is in early development, and there is no playable build yet.

## Documentation

- [Vision](docs/VISION.md): what the game is meant to be.
- [AI](docs/AI.md): how actors decide.
- [Simulation](docs/SIMULATION.md): items, bodies and knowledge.
- [Developer documentation](dev/README.md): architecture, invariants and glossary.

## Building and checking

```sh
# Debug build of the whole workspace
cargo build --workspace

# Optimised release build of the headless binary
cargo build --release -p expanse-headless

# Checks, as run before committing and in CI
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Mutation testing of your changes, before opening a pull request
cargo install --locked cargo-mutants
git diff main > changes.diff && cargo mutants --in-diff changes.diff
```

See the [testing principles](dev/TESTING.md) and the
[style guide](dev/STYLE.md) for what the checks expect.

## License

The code and documentation are licensed under the [MIT License](LICENSE).
The art used in released builds comes from a third-party asset pack under its
own licence, and is not part of this repository.

> This is a non-commercial hobby project.
