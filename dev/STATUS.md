# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-25
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** seeing the world (item 4 of the spec): tiles in their biome's
  colour, a camera that wraps across the seam, terrain views by field, the
  obstacle layer, the tile inspector and regeneration from the debug overlay.
  Tuning the default biome table is deferred until there is gameplay.
- **Next:** item 5, the asset pipeline.
- **Open:** the terrain does not yet record the seed and generation
  parameters it came from, as the spec asks. Nothing reads them, and the
  parameters hold floats, which the world's `Eq` and state hash cannot
  include as they stand. To settle before saves exist.
