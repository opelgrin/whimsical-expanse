# Commands for building, checking and testing. CI runs these same recipes.
# Each recipe is a plain cargo or git command, so it works on Linux, macOS and Windows.

[windows]
set shell := ["powershell.exe", "-NoLogo", "-Command"]

# List the recipes
default:
    @just --list

# Build the whole workspace
build:
    cargo build --workspace

# Run the game window, e.g. `just game --seed 1`
game *args:
    cargo run --release --locked -p expanse-game -- {{args}}

# Run the headless simulation, e.g. `just headless --seed 1 --ticks 100000`
headless *args:
    cargo run --release --locked -q -p expanse-headless -- {{args}}

# Format the code in place
fmt:
    cargo fmt --all

# Fail if the code is not formatted
fmt-check:
    cargo fmt --all --check

# Lint every target; warnings are errors
lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Run the test suite
test:
    cargo test --workspace --locked

# Warn about documents that need a size review. Warnings never fail the recipe.
docs-check:
    cargo run -q --locked -p expanse-docs-check

# The diff is written to the git-ignored mutants.diff in the repository root,
# so the recipe works the same on every platform.

# Mutation testing of the changes since a base branch (needs cargo-mutants)
mutants base="main":
    git diff {{base}} --output=mutants.diff
    cargo mutants --in-diff mutants.diff

# Mutation testing of the whole workspace. Slow; run occasionally.
mutants-full:
    cargo mutants --workspace

# Everything a change must pass before it is reported as done
check: fmt-check lint test docs-check
