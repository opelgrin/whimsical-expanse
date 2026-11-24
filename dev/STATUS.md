# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-24
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** generation v0 (item 3 of the spec): the `worldgen` crate, the
  biome table with its obstacle kinds, and the default content. The game and
  the headless runner start on a generated surface. The renderer still draws
  a checkerboard, so the generated terrain cannot be seen yet.
- **Next:** item 4, seeing the world: coloured tiles, a wrapping camera, the
  tile inspector, debug views and regeneration.
- **Open:** the terrain does not yet record the seed and generation
  parameters it came from, as the spec asks. Nothing reads them, and the
  parameters hold floats, which the world's `Eq` and state hash cannot
  include as they stand. To settle before saves exist.
