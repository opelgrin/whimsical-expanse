# Art authoring

How to create new pixel-art assets that match the licensed asset pack and fit
the `cook` pipeline. It does not catalogue the pack's files, tile slot order or
licence ([findings/asset-pack.md](findings/asset-pack.md)), state
version-control rules
([INVARIANTS.md](INVARIANTS.md#licensed-assets-stay-out-of-version-control)), or
define logical asset IDs ([ARCHITECTURE.md](ARCHITECTURE.md#assets)).

## Visual style

Every new sprite or tile should blend invisibly beside the pack's own sheets:

- **Grid and sharpness.** Work strictly on the `16×16` pixel grid with
  nearest-neighbour scaling, transparent PNG backgrounds and zero anti-aliasing.
  Avoid mixed pixel sizes ("mixels").
- **Tinted 1 px outlines.** Never use harsh pure black (`#000000`). Match the
  pack's outline tints: deep pine-teal (`#07232E`) for foliage and plants; dark
  plum-brown (`#38242E`) for stone, earth and wood.
- **Lighting and shadows.** Light comes from the top / top-left, with 3–4 clear
  value steps per material (warm sunlit highlight, saturated midtone, cooler or
  deeper shadow). Grounded props cast a subtle translucent `#38242E` drop shadow
  (around 25–30% alpha) hugging their bottom edge.
- **Form language.** Rocks and boulders use faceted flat tops with a 1 px bright
  bevel along the top-front edge and shaded vertical faces (matching the pack's
  stone sheets). Terrain transitions use soft wavy or tufted organic contours
  rather than straight tile borders.
- **Scale benchmark.** Humanoid characters stand roughly 21 px tall inside
  `32×32` frames. Small `1×1` shrubs and rocks sit around 10–15 px tall, `1×2`
  tall plants around 24–30 px, and `3×3` mature trees around 36–46 px with their
  trunk anchored in tile `[1, 2]`.

## Palettes

The pack's own palette file is incomplete (as its author notes), so sample
colours directly from neighbouring sprites in the relevant category rather than
relying on one global swatch. Keep each biome's ground hue distinct from
neighbouring biomes—especially coastal beach sand versus inland desert, and
temperate grass versus drier grasslands.

| Surface | Base | Supporting ramp / notes |
|---|---|---|
| Pack beach sand *(ref)* | `#FFBB6E` | `#FFD9B3` highlight, `#EE9D51` ripple |
| Pack spring grass *(ref)* | `#79BF56` | `#3D993D` / `#347349` edges |
| Pack warm stone / bank *(ref)* | `#BE6D47` | `#9D4C46` shadow, `#6E3539` crevice |
| Desert — golden ochre | `#DF9E53` | `#F8D090` dune crest, `#ECB872` light, `#C88340` ripple/hardpan, `#B06936` edge, `#804636` rim |

Record newly chosen biome palettes here as they are settled so later biomes
avoid colliding hues.

## Possible directions for a biome

The tiers below are suggestions, not a rigid checklist. What makes a biome feel
distinct—especially beyond Tier 1—depends heavily on the biome itself (dunes and
oases for a desert, bogs and reeds for a swamp, lichen and ice ridges for
tundra).

### Tier 1 — Playable ground and core obstacles

What the renderer needs first to display a biome convincingly beside others:

- **47-tile ground blob (`192×64` per block):** follows the exact `12×4`
  neighbour-mask order in [findings/asset-pack.md](findings/asset-pack.md),
  transparent outside the edge (plus an inverted row if paths or cutouts are
  needed). Check
  [`default_recipes.rs`](../tools/cook/src/default_recipes.rs) for whether the
  ground is constant or seasonal before multiplying work across seasons.
- **Interior ground variants (`16×16`):** 3–4 seamless variations (plain base,
  subtle texture/ripples, fine pebbles) so wide expanses do not show a grid.
- **Animated shore over water (`768×128`):** 4 animated frames (`4 × 192×64`,
  non-inverted plus inverted rows) matching the pack's grass-over-water sheets.
  Recolouring the land top of an existing shore sheet preserves pixel-exact
  water ripples and banks.
- **Core obstacles and scatter:** match the tile footprints and anchors in
  [`default_recipes.rs`](../tools/cook/src/default_recipes.rs) (for example
  `1×1` rocks and shrubs, `1×2` tall flora anchored at `[0, 1]`, `2×2` boulders,
  `3×3` trees anchored at `[1, 2]` with sapling and stump), plus non-blocking
  `1×1` ground scatter (grass tufts, pebbles, twigs).

### Tier 2 — Terrain depth and landmarks *(optional directions)*

- **Secondary ground layer:** a contrasting sub-terrain (cracked hardpan, wet
  mud, bare scree, packed snow) with transition tiles to break up large regions.
- **Cliffs and terraces:** 3/4-view elevation faces matching the pack's cliff
  sheet layout, including cave-mouth and water-foot variants.
- **Surface features and passages:** tapered ridges, worn paths, shoreline reeds
  or rocks, and biome-matched dungeon or ruin entrances (often recoloured or
  weathered from existing cave and temple sheets).

### Tier 3 — Ecology, items and settlements *(optional directions)*

- **Forageables and icons (`16×16`):** biome-specific fruit, herbs, ores and
  gear icons.
- **Settlement variants:** wall, roof and prop recolours of the pack's house
  sheets (for example adobe, timber or stone styles).
- **Wildlife and enemies (`32×32` animations):** favour recolouring or
  kitbashing existing creature sheets and paper-doll character layers before
  authoring brand-new multi-directional animation cycles from scratch, as
  full custom animations take the most iteration.

## Workflow and verification

Tooling is intentionally left to the environment: use whichever pixel-art editor
or scripting and image library works best on the current machine (Linux, macOS
or Windows).

1. **Settle hues early.** Before generating a full 47-tile sheet, render 2–3
   small palette swatches side-by-side with neighbouring pack terrain and a pack
   character to check contrast and biome separation.
2. **Keep custom art local.** Write new and derived PNGs to a local folder
   beside the licensed pack (such as `_assets_custom/<Biome>/`), excluded via
   `.git/info/exclude` so
   [licensed and derived art stays out of version control](INVARIANTS.md#licensed-assets-stay-out-of-version-control).
   Include a plain-text `README.txt` in each biome folder listing pixel
   rectangles, anchor tiles and palette hex codes for recipe authoring.
3. **Prefer exact pixel authoring.** Programmatic pixel grids and mask
   transforms are fast and reliable for 47-tile blobs, shore conversions, icons
   and `16×16`–`48×48` sprites. If generative tools are used for larger static
   art (such as portraits), always finish with a strict grid-snap,
   palette-quantisation and outline cleanup pass.
4. **Verify before finishing:**
   - Check every 47-tile blob sheet automatically against the transparency-mask
     rule and slot order in [findings/asset-pack.md](findings/asset-pack.md),
     except for the kinds of block it lists as misread by that rule.
   - Inspect **4× nearest-neighbour** previews of both the sprite sheets (over a
     dark checkerboard to spot stray pixels) and a multi-tile mockup containing
     a pack character and an adjacent pack biome tile.
   - Check `git status` to confirm no art files enter the repository.
