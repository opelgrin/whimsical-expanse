# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-26
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** cooked placeholders (item 5a of the spec): `just cook` draws the
  placeholders and writes the container, which the game loads and checks
  against the content at startup. The debug overlay shows the atlas. The
  terrain is still drawn in flat colours.
- **Next:** item 5b, drawing the terrain from the atlas.
- **Open:** the terrain does not yet record the seed and generation
  parameters it came from, as the spec asks. Nothing reads them, and the
  parameters hold floats, which the world's `Eq` and state hash cannot
  include as they stand. To settle before saves exist.
