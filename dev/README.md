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
| What comes next, in order? | [ROADMAP.md](ROADMAP.md) |
| Where are we right now? | [STATUS.md](STATUS.md) |
| How do we write Rust here? | [STYLE.md](STYLE.md) |
| How do we test? | [TESTING.md](TESTING.md) |
| Why was a structural choice made? | the rationale beside that structure, in [ARCHITECTURE.md](ARCHITECTURE.md) or [INVARIANTS.md](INVARIANTS.md) |
| What does the system do right now? | the code and its tests |

If a kind of information has no row, add a row, not a new document.

Documents planned once they have content: `specs/` (requirements for a work item),
`proposals/` (designs still under discussion) and `findings/` (facts and dead ends
worth keeping, with their evidence).

## Orientation

At the start of a session, read the status, the vision, the glossary and the
invariants. Read the architecture and other documents when the task needs them.

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

## Keeping documents navigable

Documents written with coding agents grow fast and scatter. These rules keep
them findable by people.

- **One home per fact.** A fact lives in the one document the table above names.
  Other documents link to it and never restate it. Do not add a collecting
  document (a list of open questions, a changelog): keep a question in the
  section it concerns.
- **A scope line.** A new or rewritten document opens with what it covers and,
  where a neighbour could be confused with it, what it does not.
- **Size is a signal, not a rule.** `cargo run -p expanse-docs-check` warns about
  a document over 10 KB in `dev/` or 16 KB in `docs/`, and CI shows the warnings
  in its job summary. A warning never fails the build. It asks for a review:
  1. Does the document still have one scope, matching its opening line?
  2. Does part of it belong elsewhere, or duplicate another document?
  3. Could a typical question be answered by reading only part of it? If so,
     split along that line. If not, a split only makes readers load several files.
  4. Does anything describe the past instead of the present?

  The outcome is to split, to trim, or to leave it as it is and add
  `<!-- size-reviewed: N -->` with its current size in bytes. A reviewed document
  is flagged again only after growing by a quarter.
- **Work-item documents are scaffolding.** A spec or proposal is deleted once its
  work is done, after moving what lasts to its home. Git keeps the rest.
- **Decisions stay beside what they explain.** Write a separate decision record
  only for a choice that is expensive to reverse and that someone would plausibly
  propose again. When the decision changes, rewrite the record instead of adding
  one that supersedes it. Create a `decisions/` folder when about five qualify.
- **Prune when a milestone ends.** Act on the size warnings, delete what is stale
  or duplicated, and rewrite [STATUS.md](STATUS.md).

## Writing conventions

- Describe concepts and roles, not named content. "An actor without
  physiological drives", not "the skeleton".
- Link only to public, versioned sources.
