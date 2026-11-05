# Developer documentation

Documentation for developers and coding agents: how the project is built and the
rules for changing it. What the game is meant to be lives in [docs/](../docs/),
which can be read without the code.

## Which document answers which question

| Question | Source |
|---|---|
| What is the game trying to be? | [docs/VISION.md](../docs/VISION.md) |
| How do actors decide? | [docs/AI.md](../docs/AI.md) |
| What are the world's rules (items, knowledge)? | [docs/SIMULATION.md](../docs/SIMULATION.md) |
| What do project terms mean? | [CONTEXT.md](CONTEXT.md) |
| What must every change preserve? | [INVARIANTS.md](INVARIANTS.md) |
| How does the system fit together? | [ARCHITECTURE.md](ARCHITECTURE.md) |
| What comes next? | [ROADMAP.md](ROADMAP.md) |
| How do we write Rust here? | [STYLE.md](STYLE.md) |
| How do we test? | [TESTING.md](TESTING.md) |
| What does the system do right now? | the code and its tests |

Documents planned once they have content: `specs/` (requirements for a work item), `proposals/` (designs still
under discussion) and `findings/` (facts and dead ends worth keeping, with their
evidence).

## Orientation

At the start of a session, read the vision, the glossary and the invariants.
Read the architecture and other documents when the task needs them.

## Authority

- The code and tests define current behaviour.
- The invariants constrain all work. Changing one needs explicit agreement.
- The `docs/` documents define the direction. Their examples illustrate what the
  engine must support; they are not agreed content.
- When two sources disagree, point out the contradiction instead of silently
  choosing one.

## Where information goes

- **A rule every change must keep:** INVARIANTS, with a short rationale.
- **How the system is structured:** ARCHITECTURE. Link to the code rather than
  copying its details.
- **What a term means:** CONTEXT. One meaning per word, one word per meaning.
- **Game design and direction:** the matching document in `docs/`.
- **A detailed mechanism:** next to its code, in doc comments.

Keep the rationale next to the rule or structure it explains. Git keeps the
history, so documents describe the present rather than how they got there.

## Writing conventions

- Describe concepts and roles, not named content. "An actor without
  physiological drives", not "the skeleton".
- Link only to public, versioned sources.
