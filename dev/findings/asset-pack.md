# The licensed asset pack

How the licensed pack is structured, and what that means for rendering, the
placeholders and the `cook` tool. It records facts about the pack, with how
each was checked. The rules for using the pack are in
[INVARIANTS.md](../INVARIANTS.md#licensed-assets-stay-out-of-version-control)
and [ARCHITECTURE.md](../ARCHITECTURE.md#assets).

- **Pack:** "Farm RPG – Tiny Asset Pack (All in One)", by EmanuelleDev
  ([itch.io page](https://emanuelledev.itch.io)). The full version, bought.
- **Surveyed:** 2026-11-15. The pack has no version number; its newest file is
  dated 2026-07-29. The author updates it, so a new download can change any
  fact below.
- **Size:** 31 MB, 5,823 files: 5,747 PNG, 69 Aseprite sources and 7 text
  files.

## Licence

From the pack's `Documentation.txt`:

- The pack can be used in commercial and non-commercial projects.
- The assets can be modified freely.
- The pack cannot be resold or redistributed, even modified.
- Credit is mandatory, as "EmanuelleDev" with a link to the itch.io page.

Consequences:

- Our modifications are derived from the pack, so they stay out of git like
  the pack. They live in a local folder beside it.
- The project credits the author once the game can use the pack: in the
  README, and on a credits screen when one exists.
- A mapping from logical asset IDs to file paths and pixel rectangles contains
  no image data, so committing it does not redistribute the pack. This is our
  reading of the licence, not legal advice.

The folder that holds the pack can also hold a personal download link. It never
enters the repository.

## Tiles

**Tiles are 16×16.** Every sheet in `Tileset/` has dimensions that are
multiples of 16, and a single water tile is exactly 16×16. The camera's zoom
levels already assume 16 px.

**Transitions use the 47-tile blob scheme.** A tile's shape depends on all 8 of
its neighbours. Each transition is a block of 12×4 tiles. In the grass, grass
over water, path, tilled soil, water, cave water, frozen water and beach sheets,
every complete block holds the 47 shapes of the scheme and one empty tile
(checked by comparing the pixels of the 48 tiles of each block). In an inverted
block, described below, the full shape is itself empty.

**Every block puts the shapes in the same order** (mapped 2026-11-26). Each
slot holds the shape for one 8-neighbour mask, with bits N = 1, NE = 2, E = 4,
SE = 8, S = 16, SW = 32, W = 64 and NW = 128, where a corner counts only when
both of its edges are present. The slots, as they lie in the block (`-` is the
empty tile):

```text
 16  20  84  80 213  92 116  87  28 125 124 112
 17  21  85  81  29 127 253 113  31 119   - 245
  1   5  69  65  23 223 247 209  95 255 221 241
  0   4  68  64 117  71 197  93   7 199 215 193
```

The first four columns are the 16 shapes without inner corners. Checked by
reading each slot's mask from its transparency (a side is present when the
tile's outermost pixels on that side are all opaque, a corner when its pixel
is): all 47 shapes, in this order, in 48 blocks of the grass sheets of every
season and of deep forest, grass over water, cave grass, the second winter
grass sheet and the beach. The four blocks in a row of a grass over water
sheet differ only in the shore's pixels; one of them reads a few corners as
filled because a shore pixel lies on the corner. Path, water, cave water,
frozen water, tilled soil and dungeon blocks draw their shapes in colour, not
transparency: they have the same arrangement and empty slot by eye, not
checked pixel by pixel. Inverted blocks were not checked.

**Terrain is drawn in layers.** A lower terrain (water, dirt, sand, a dark
void) fills the tile. A higher terrain (grass) is drawn over it as a blob tile,
transparent outside its edge. Some blocks are inverted: grass edges around a
transparent hole, for paths cut into grass. Rendering therefore needs an order
of terrains and, for each layer of a tile, the 8-neighbour mask of that layer.

**Water edges are animated**, with 4 frames.

**Cliffs are terraces seen from a three-quarter view.** The grass top is a blob
tile; the rock face below it is about 2 tiles tall and faces south. Variants
have cave entrances or water at the foot. Representing cliffs needs discrete
elevation levels and placement rules, such as a minimum plateau width.

## Seasons

Seasonal variants come in two forms:

- **Another sheet with the same layout.** Grass, grass over water and cliffs
  each have a Spring, Summer, Fall and Winter sheet. The Spring, Summer and Fall
  sheets have identical transparency masks, pixel for pixel, so only colours
  change. Winter grass agrees on 98% of its pixels, because snow changes the
  edges. Winter grass over water and winter cliffs have other sizes, so they
  need their own regions.
- **Another region on the same sheet.** Trees and props keep their seasonal
  versions side by side.

A Deep Forest sheet, with the same layout as Summer, is a biome variant, not a
season.

## Sprites larger than a tile

- **Trees** are multi-tile sprites, about 3×3 tiles for a large tree. A sheet
  holds the seasonal versions, growth stages, a stump and white silhouettes, at
  irregular positions, with and without shadows. Each sprite therefore needs a
  rectangle and an anchor (the point at its foot), written by hand or found
  from its non-transparent pixels. Trees cover the tiles behind them, so
  sprites are drawn in order of their anchor's `y`.
- **Other large objects:** houses span 5–8 tiles in width and 6–8 in height;
  fallen logs are 8×2 or 2×8 tiles and large stumps about 4×2; statues, mineral
  rocks and altars also span several tiles. The image does not say which tiles
  an object blocks: a house blocks its base, not its roof.
- **Creatures** use 32×32 frames, one animation per file. For example, a deer's
  walk is 4 frames in each of 3 directions (side, front, back); the other side
  is presumably the side frames mirrored.
- **The player character is a paper doll:** skin, eyes, hair, clothes and
  accessories are separate layers. Each animation is a strip of 24 frames of
  32×32. The body fills a small part of the frame, about one tile tall.

## What the art can show

Ground: grass in four seasons, deep forest, beach and sand, water, frozen water,
caves, dirt paths and tilled soil. There is no desert, swamp or tundra, though
the winter look could serve a cold biome all year. Biomes beyond these need
new art.

## File names

Paths contain spaces, apostrophes, a typographic apostrophe (`’`) and typos
("Clothers", "Turttle", "copiar"). The mapping refers to them exactly as they
are.
