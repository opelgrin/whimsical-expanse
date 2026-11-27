# Status

Where the work stands, for someone starting a session. Not the plan (see the
[roadmap](ROADMAP.md), which also records what is done) and not the history (git).
Rewrite it, never append. The pull request that finishes a work item updates it.

- **Updated:** 2026-11-27
- **Milestone:** [M1: A world to explore](ROADMAP.md#m1-a-world-to-explore),
  refined in [its spec](specs/M1.md). M0 is done.
- **Last:** textured terrain (item 5b of the spec): `cook` bakes one mipmap
  level, ground-over-ground pair blocks, and keeps each ground's variants on
  one atlas page (container format version 2). The renderer draws layered blob
  tiles from a per-chunk LRU GPU cache invalidated by chunk pointer identity,
  with crisp pixel filtering. Flat biome and field views remain in the debug
  overlay.
- **Next:** item 5c, drawing obstacle and entity sprites.
- **Open:** the terrain does not yet record the seed and generation
  parameters it came from, as the spec asks. Nothing reads them, and the
  parameters hold floats, which the world's `Eq` and state hash cannot
  include as they stand. To settle before saves exist.
