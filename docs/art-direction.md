# Art direction

The target look for bevy-verse, in the user's words (2026-10-19):

> Ocarina of Time DS look: low-poly, colorful, good looking, but with an old feeling.

The reference is the **3DS remake, *The Legend of Zelda: Ocarina of Time 3D* (2011)**,
not the Nintendo 64 original or the DS Zelda games.

For the far distance the user added (2026-10-19):

> When looking in the distance I want more of that Breath of the Wild feeling of vast openness.

So the look is split by distance: **near is *Ocarina of Time 3D*, far is *Breath of the
Wild*.**

This is a goal, not a design decision. It is the yardstick for the open decisions
below: when two options are otherwise close, pick the one that fits this look.

## What the look means

- **Low-poly.** Few, large, visible faces. Facets are part of the style, not a
  defect to smooth away.
- **Colorful.** Saturated, readable colors. Surfaces read by color first,
  texture detail second.
- **Good looking.** Clean and deliberate, not noisy. Old hardware limits, done
  well, rather than a broken-looking retro filter.
- **Old feeling.** The look of the 3DS remake: a small screen (400×240 on the top
  screen), low-poly models with cleaner, more colorful textures than the N64
  original, simple lighting, and distance fading away instead of sharp detail
  all the way to the horizon. It is a polished handheld look, not a deliberately
  rough retro one.

## Near and far

| | near (around the player) | far (the horizon) |
|---|---|---|
| reference | *Ocarina of Time 3D* | *Breath of the Wild* |
| feel | chunky, colorful, readable | vast, open, calm |
| geometry | low-poly facets, visible steps | big silhouettes only: mountains, ridges, landmarks |
| color | saturated material colors | fades toward the sky color with distance (aerial perspective) |
| distance cue | — | haze that tints and lightens, not an opaque fog wall |

The two fit together because the low-poly style hides coarse LOD well: a far chunk
meshed at a coarse LOD still looks like the near terrain, only simpler. The haze does
the rest.

### Where they collide

- **Fog vs. view distance.** *Ocarina of Time 3D* hides the world's end and LOD
  swaps behind close fog. Vast openness needs a long view and thin haze, so LOD
  changes become visible. Mitigations: haze that grows with distance, LOD swaps far
  enough out to fall inside the haze, and a dithered cross-fade
  ([RND-9](rendering.md#decisions)) instead of a hard swap.
- **Low internal resolution vs. far readability.** A 400×240-style resolution makes
  thin far silhouettes shimmer and break up. Options: render at a higher resolution
  and keep the old feeling through color, geometry and lighting instead, or use a
  low resolution only up close (not practical in one frame), or accept some shimmer.
- **View distance vs. memory and streaming.** A long view means more LOD rings. This
  pushes toward far chunks being kept as meshes only, with no voxel grid
  ([chunks.md](chunks.md#decisions)), very coarse far LODs, and a cheap way to
  generate them ([GEN-1](generator.md#decisions)). World bounds
  ([X-5](architecture.md#6-cross-cutting-decisions)) set how far there is to see.

## How it bears on open decisions

None of these are decided. They show which way the look leans on each one.

| decision | question | how the look leans |
|---|---|---|
| [X-1](architecture.md#6-cross-cutting-decisions), [X-2](architecture.md#6-cross-cutting-decisions), [IE-1](isosurface-extractor.md#decisions) | sample encoding, max and isolevel | A small configurable max (for example 15 with isolevel 7) gives the coarse, stepped surfaces the look wants. |
| [IE-8](isosurface-extractor.md#decisions) | occupancy normals | Blocky, axis-leaning normals fit a low-poly look. Accepting them is the option closest to the style. |
| [RND-5](rendering.md#decisions) | material borders | A flat material ID per vertex gives hard color borders between materials, which fits. Smooth blending fits less. |
| [RND-4](rendering.md#decisions) | material ID transport | Whichever transport is chosen, color per material (a palette) is closer to the look than high-resolution textures. |
| [RND-9](rendering.md#decisions) | LOD swap | With a long view and thin haze, swaps are more visible: a dithered cross-fade, or swaps placed inside the haze, fits better than a hard swap. |
| [RND-6](rendering.md#decisions) | mesh compression | Low-poly, quantised geometry compresses well: chunk-local positions fit small integers. |

## Rendering ideas to evaluate

Candidates only, none designed yet:

- Flat or near-flat shading, possibly from face normals (marching cubes already emits them).
- A limited color palette per material, set by material ID.
- Low-resolution textures, if textures are used at all. Smooth (bilinear) filtering is closer to
  the 3DS remake; nearest-neighbour is more retro than the reference.
- Rendering at a low internal resolution (around the 3DS's 400×240) and upscaling.
- Aerial-perspective haze: blend toward the sky color with distance, lightly near and strongly at
  the horizon, instead of a close opaque fog.
- Simple lighting: ambient plus one directional light; baked or per-vertex shading instead of
  per-pixel detail.

## Open questions

- Should textures exist at all, or is everything vertex/material color? The remake does use
  textures, so this is a choice about how close to follow it.
- Is a fixed low internal resolution part of the look, or should it scale with the screen? A long
  view distance argues for scaling (see "Where they collide").
- How far should the player see? It sets the number of LOD rings, memory, and world bounds.
