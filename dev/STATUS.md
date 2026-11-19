# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-19
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** the world shape (item 2 of the spec): the grid wraps, the terrain
  is stored in shared chunks with footprints, filled with a stand-in pattern,
  and the wanderer crosses the seam. The camera still clamps, so the seam
  cannot be seen yet. The spec now defines the generation model, and puts
  generation before seeing the world.
- **Next:** item 3, generation v0: the `worldgen` crate, the biome table, the
  passes and the obstacles.
- **Open:** nothing blocking.
