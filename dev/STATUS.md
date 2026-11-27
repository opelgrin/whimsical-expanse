# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-27
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** sprites (item 5c of the spec): obstacle and entity sprites are
  gathered with overhang-expanded culling across the seam, sorted by anchor
  `(y, x, key)`, batched per atlas page, and drawn with crisp pixel filtering.
  Obstacle sprites show in the textured terrain view; entities use their sprite
  in every view; the debug obstacle layer draws over sprites. Multi-row
  obstacles (`boulder`) now anchor on their southmost row.
- **Next:** item 6, calendar and seasons.
- **Open:** the terrain does not yet record the seed and generation
  parameters it came from, as the spec asks. Nothing reads them, and the
  parameters hold floats, which the world's `Eq` and state hash cannot
  include as they stand. To settle before saves exist.
