# Water surface meshing

> **Status (2026-10-19): the fill model below is Claude's proposal, not agreed.**
> It replaces the slab model, which floated water above every gentle shore (see
> [Why the model changed](#why-the-model-changed)). The user has a different
> model in mind. The slab-model version of this document, and the code before
> any of this, are on `master` (tag `checkpoint/before-fill-model`); this work
> is on branch `water-fill-model`.

## Summary

The terrain is an isosurface of a solid field `s`. This adds a second
per-sample value, water `w`, and a mesher for the **top surface** of the water.

One idea carries it: **rock and water are both fill.** Every sample owns a slab
one cell tall, centred on it. `s` is how much of the slab rock fills and `w` how
much water fills, out of 15, with `s + w ≤ 15`. On a vertical edge, the terrain
and the water are placed by the same volume formula:

```
terrain:  (s_lo + s_hi − 7.5) / 15      rock filled to there from below
water:    (b_lo + b_hi − 7.5) / 15      b = rock under the water + the water
```

With no water the two are the same number, so **the water rests exactly on the
drawn terrain**, and each unit of water lifts it 1/15 of a cell. A settled pool
is flat, and its shoreline is where that flat plane crosses the terrain mesh on
each cell face.

The output is a separate mesh per chunk. Side faces (waterfalls, water standing
against air) are not part of this pass; see [Limitations](#limitations).

---

## Why the model changed

The slab model (on `master`) placed water by volume but the terrain by linear
interpolation, `(s_lo − 7.5)/(s_lo − s_hi)`. Those land on different heights:
a bank sample just over the isolevel (`s = 8` under `s = 5`) was drawn at 0.17
above its sample, while the slab model counted it as a full wall up to 0.47.
A settled pool at, say, 0.27 then lies above the drawn bank but below the
model's wall, so no terrain segment brackets the level on that face, the
fallback fires, and the water ramps up and floats about a third of a cell above
the ground. That happened on every gentle shore, in settled water.

Flat pools, water meeting the drawn terrain, and integer storage can only hold
together if the terrain's vertical-edge heights land on the water's 1/15 grid.
So the terrain has to follow the same volume model as the water.

---

## 1. Values and fill

### 1.1 Constants

| name | value | meaning |
|---|---|---|
| `S_MAX` | 15 | a sample holds `0..=15` of rock and water together |
| `ISO` | 7 | terrain isolevel: **solid when `s > 7`** |
| `W_MAX` | `= S_MAX` | water uses the same units as rock, since it adds to the same fill |
| `W_ISO` | `= ISO` | a corner is inside the water when its bottom fill `b > 7` |

All four are compile-time constants (`isosurface::{S_MAX, ISO}`,
`water::{W_MAX, W_ISO}`). `W_MAX`/`W_ISO` are no longer free choices: they must
equal the terrain's, because the water's height formula adds rock and water.
Crossings are at 7.5, half a unit past the isolevel, so every vertex lies
strictly inside its edge.

### 1.2 The slab and the terrain

Sample `y` owns the slab `[y − ½, y + ½]`. In a **settled** column rock fills
each slab from the bottom, and a slab is full before the one above holds any:
a heightfield whose ground is at height `g` stores
`s_y = 15 · clamp(g − y + ½, 0, 1)`.

The terrain mesher (`isosurface::edge_t`) places a vertical-edge vertex at
`(s_solid + s_empty − 7.5)/15` from its solid corner, counting the empty
sample's rock as part of the same body. For a settled column that is exactly
`g`. Horizontal edges keep linear interpolation, which on the same data gives
exactly the linear interpolation between neighbouring columns' heights.
Both rules classify exactly as `s > 7` does. `edge_t` is two 256-entry tables.

Ceilings work the same way from the top: rock attached from above fills its
slab from the top, and the vertex is `(s_hi + s_lo − 7.5)/15` below the upper
corner.

### 1.3 Bottom fill

The water mesher reads, per corner, its **bottom fill** `b`: how much of the
slab is filled from the bottom, by rock and water together. Where the rock
sits depends on what it rests on:

| corner | its rock | `b` |
|---|---|---|
| sample below is solid (or the buffer's bottom) | rests on it, at the slab's bottom; water on top | `s + w` |
| solid, sample below not solid: a **ledge** | hangs at the slab's top; its water is underneath, in the edge below | `s` |
| not solid, sample below not solid | held from above or from the side: not under any water | `w` |

- `O`: inside when `b > 7`. A solid corner always is (`S ⊆ O`).
- Wet: `w > 0`, except a ledge, whose water belongs to the edge below it.
- Capacity: `s + w ≤ 15` everywhere. Rock takes room; there is no separate
  rule for solid points.

**Rock independence, revised.** The slab model's rule "rock below the isolevel
is ignored" is gone. Rock resting on the ground is drawn (it lifts the
vertical-edge vertex) and the water rests on it, so the two move together.
Rock not resting on anything below is still ignored by the water.

---

## 2. Cell geometry in this codebase

Corner `c` sits at `VERTEX_POSITIONS[c]`, where x is bit 0, y is bit 1 and z is
bit 2, with Y up. Edges are `EDGE_VERTEX_INDICES`. **Ignore the first ASCII
diagram at the top of `tables.rs`:** it is stale and gives different edge
numbers. In the real table:

- vertical (Y) edges are 1, 3, 5, 7;
- edges 0, 2, 4, 6 run along X;
- edges 8–11 run along Z.

Tables written for Bourke's layout cannot be reused without remapping.

Number the four vertical edges `k = 0..3` counter-clockwise seen from above, and
the four vertical faces so that face `f` lies between slot `f` and slot
`f + 1 (mod 4)`:

| slot `k` | vertical edge | column (lower, upper) | face to next slot | face bottom / top edge |
|---|---|---|---|---|
| 0 | 7 | (4, 6) | z = 1 | 4 / 6 |
| 1 | 5 | (5, 7) | x = 1 | 9 / 10 |
| 2 | 1 | (1, 3) | z = 0 | 0 / 2 |
| 3 | 3 | (0, 2) | x = 0 | 8 / 11 |

Triangles listed in this slot order are wound counter-clockwise from above, so
they face upward. That gives the winding only, not the normal: see §6.1.

Two codebase facts to keep in mind:

- The terrain's **case index sets a bit for an *empty* corner** (`s ≤ ISO`).
  This document's masks set bits for *inside* corners, so the terrain's case
  for a cell is `!S`.
- `edge_t` interpolates from the **first** corner in `EDGE_VERTEX_INDICES`, and
  some vertical edges run downward (3 is `(2, 0)`, 7 is `(6, 4)`). See
  [§6](#6-seams-and-vertex-sharing) for why that matters.

---

## 3. Top-surface algorithm

### 3.1 Corner masks

For each cell, make one pass over its 8 corners and work out `b` and three
masks. A lower corner needs the sample under it to know what its rock rests on.
Apply `W_EPS` first: treat `w < W_EPS` as 0.

| mask | bit `c` set when | used for |
|---|---|---|
| `S` | `s_c > ISO` | terrain contours on the faces (shoreline) |
| `O` | `b_c > W_ISO` | which vertical edges carry the surface; fallback contours |
| `M` | `w_c > 0`, not a ledge | wetness |

If `M == 0` and no ledge holds water, the cell has no water: skip it.

### 3.2 Surface edges

A vertical edge **carries the water surface** when its lower corner is inside
`O`, its upper corner is outside `O`, and either corner holds water. Its vertex
is `t = VERT_T[b_lo + b_hi] = (b_lo + b_hi − 7.5)/15` up from the lower corner.
Collect these edges into a 4-bit mask `Q`; if `Q == 0` the cell has no top
surface, the early out for almost every cell.

With `w = 0` this is exactly the terrain's own height on that edge, so the
water's floor is the drawn ground; each unit of water lifts it 1/15.

**Water under a ledge.** The upper corner is solid (a ledge), the lower one is
not solid but inside, and there is water. Both corners are inside `O`, so the
rule above finds no crossing. The water fills from the bottom: the lower
corner's `b`, then the water standing under the ledge's rock, `w_hi`:

```
t = (b_lo + w_hi − 7.5) / 15
```

The terrain draws the ledge's underside `(s_hi + s_lo − 7.5)/15` below the upper
corner. The edge carries the surface while the water is below it:

```
b_lo + w_hi + s_hi + s_lo < 30
```

exact in integers, and the same on a tie as the floats: water that touches the
ledge counts as reaching it. Otherwise the edge is left out and the shoreline
points on its faces put the waterline on the ledge. Since `s + w ≤ 15` per
sample, under a settled pool the test fails exactly when the ledge's slab is
full: the pocket fills one step per unit until the water meets the drawn
underside. Without interpolation the ledge is at the midpoint: `b_lo + w_hi < 15`.

### 3.3 Shoreline points

A shoreline point exists on every face with exactly one surface edge: one wet
edge `k` and one dry edge. The water ends somewhere on that face, where the
horizontal line `y = H_k` meets the terrain.

On a face, the terrain mesh is a polyline: at most two segments joining terrain
vertices on the face's four edges. `FACE_SEGS`, indexed by the face's four `S`
bits, gives the segments. Find the segment whose endpoints `A` and `B` lie on
opposite sides of `H_k`, and interpolate to that height:

```
P = A + (H_k − A.y) / (B.y − A.y) · (B − A)
```

`A` and `B` must be exactly the terrain mesher's vertices, evaluated
canonically as described in [§6](#6-seams-and-vertex-sharing), so `P` lies on a
terrain mesh edge. If two segments bracket `H_k`, take the one nearer the wet
edge. When the water level is exactly at a terrain vertex's height, `P` is that
vertex. No extra code is needed for this.

**Fallback.** Sometimes no terrain segment brackets `H_k`. In settled water this
can't happen: a neighbouring column whose drawn ground is below the level has
room, so settled water fills it and its edge carries the surface too (this is
what the fill model fixes; under the slab model it happened on every gentle
shore). It only happens in unsettled states, for example when the neighbouring
column's surface is in the cell row above, or the neighbour is open air the
water hasn't reached yet. In that case, use the occupancy crossing on the edge
that the `O` contour connects to the wet edge. It comes from the same
`FACE_SEGS` table, indexed by `O`'s face bits, and is placed the way the terrain
places its vertices, on the bottom fill `b` instead of the rock: by the volume
formula `(b_i + b_u − 7.5)/W_MAX` from the inside corner `i` on a vertical edge,
linearly to 7.5 on a horizontal one (`(b_i − 7.5) · RECIP[b_i − b_u]`). That
point is on a cell edge, so the cell across a level step computes the same one,
and the surface stays watertight when the level steps between rows. If the wet
edge isn't on the `O` contour at all (a slot kept by the ledge rule), the
shoreline point is the wet edge's own surface vertex.

**Shore overlap (recommended).** Move each terrain-derived shoreline point a
small distance `SHORE_OVERLAP` (about `0.02·cs`) horizontally along the face,
toward the dry edge, into the solid. Inside the cell, the true waterline can bend
where it crosses interior terrain triangles, while the water polygon's edge is a
straight chord between two face points. The overlap tucks that chord under the
terrain, so the depth test draws the exact waterline and float noise at the
shore stays hidden.

### 3.4 Polygon table

`MS_TRIS[16]` lists triangles over 8 slots: the surface vertices `V0–V3` and the
shoreline points `F0–F3`. Generate it; don't hand-author it:

1. Walk `k = 0..3`.
2. Push `V_k` if `k ∈ Q`.
3. Push `F_k` if face `k` has exactly one surface edge.

This gives the boundary loop in counter-clockwise order. Fan-triangulate it.

| surface edges in `Q` | shape |
|---|---|
| 1 | triangle (V, F, F) |
| 2 adjacent | quad |
| 3 | pentagon |
| 4 | quad (open water, the common case) |
| 2 diagonal (`0101`, `1010`) | two separate corner triangles: water does not bridge the cell centre |

**Convexity.** In top view, every vertex lies on the boundary of the cell's
square (corners for `V`, faces for `F`) and appears in boundary order. A polygon
inscribed in a convex boundary in that order is convex, so the fan is always
valid. In the fallback, `F` can land on a face's top or bottom edge, so it
leaves the water plane. The top view doesn't change, so the argument still
holds.

**Separate bodies of water.** One cell can hold two bodies of open space
that the terrain walls apart: with corners 0, 3, 4 and 5 solid, corner 1 is a
pocket under rock 3, sealed from 2, 6 and 7 (both faces it shares with them are
diagonal, and the terrain joins the solid corners across them). Water in the
pocket and a pool above stand at two levels. Meshed as one polygon, the surface
would slope between them. So the slots in `Q` are grouped by the body their
water is in -- the upper corner's, or under a ledge the lower corner's --
using `CORNER_BODY`, which joins open corners along cube edges only (the
terrain's own rule), and each group is its own polygon with its own shoreline
points. Here that gives a pentagon at the pool's level and a triangle in the
pocket, both flat.

### 3.5 Pseudo-code

```rust
for cell in chunk.cells() {
    let c = load_corners(cell);        // s, w, b, S/O/M; needs the sample under each lower corner
    if c.m == 0 { continue; }

    // 1. vertical edges that carry the surface
    for k in 0..4 {
        let (lo, hi) = VERT_EDGE[k];   // (4,6) (5,7) (1,3) (0,2)
        if in_o(lo) && !in_o(hi) && (wet(lo) || wet(hi)) {
            v[k] = corner(lo) + Y * VERT_T[b[lo] + b[hi]];
        } else if solid(hi) && !solid(lo) && in_o(lo) && has_water {
            let n = b[lo] + w[hi];
            if n + s[hi] + s[lo] < 30 { v[k] = corner(lo) + Y * VERT_T[n]; }
        }
    }
    if q == 0 { continue; }

    // 2. shoreline points on faces with exactly one surface edge (§3.3):
    //    the terrain segment bracketing the level, nearest the wet edge;
    //    else the occupancy fallback
    // 3. MS_TRIS[q], fan-triangulated, zero-area triangles dropped,
    //    face normal per triangle
}
```

`src/voxel_terrain/water/mesher.rs` is the real thing.

---

## 4. Lookup tables

Generate every table in `const` code. Derive
them from `VERTEX_POSITIONS` and `EDGE_VERTEX_INDICES`, never from Bourke's
layout.

| table | size | contents |
|---|---|---|
| `VERT_EDGE[4]` | tiny | (lower, upper) corner per slot, as in §2 |
| `FACE_EDGES[4]`, `FACE_CORNERS[4]` | tiny | each vertical face's four table edges and four corners |
| `VERT_T` | `2·W_MAX + 1` × f32 | `VERT_T[n] = (n − 7.5) / W_MAX`, indexed by `b_lo + b_hi` |
| `FACE_SEGS[16]` | tiny | for a face's 4-bit inside mask, the pairs of face edges joined by a contour segment (at most 2) |
| `MS_TRIS[16]` | tiny | triangles over `V0–V3`, `F0–F3` (§3.4) |
| `RECIP` | `W_MAX + 1` × f32 | `1 / d` for `d ≥ 1`; fallback only |
| `CORNER_BODY[256]` | 2 KiB | for a solid mask, which body of open space each corner is in (§3.4) |

**`FACE_SEGS` must resolve ambiguous faces the way the terrain does.** 14 of the
16 face cases are unambiguous. For the two diagonal cases, `TRIANGLE_TABLE`
always keeps the *empty* corners apart, so solid joins across the diagonal (see
`transvoxel-face-ambiguity.md`). `FACE_SEGS` must do the same: on a diagonal
face, the two segments each cut off one **outside** corner. Any other rule pairs
different edges than the terrain mesh does, and then `P` is no longer on the
terrain surface. Use the same rule for the `O` fallback, so both cells sharing a
face agree there too.

---

## 5. Why it works

### 5.1 One formula for both

For a settled column (rock, then water, each filling slabs bottom-up), the fill
reaches `units / 15` above the bottom of the lowest slab. On the vertical edge
where it crosses 7.5 units into a slab, `b_lo + b_hi` is that total minus the
slabs below, so `(b_lo + b_hi − 7.5)/15` is exactly the fill height relative to
the edge. The terrain computes the same with `s` alone. So:

- **The water rests on the drawn terrain.** With no water, the water's height
  formula is the terrain's. Checked for every rock/water split in a column
  (`water_rests_on_the_terrain`).
- **Flat pools stay flat.** Settled water fills every column to the same total,
  so every surface vertex is at the same height, and shoreline points are
  placed at that height.
- **Each unit of water lifts the surface exactly 1/15**, with no jump where a
  sample turns inside: at `b = 7` the vertex is at 14.5/15 on the lower edge, at
  `b = 8` it is at 0.5/15 on the next, one unit higher (`column_fill_sweep`, with
  0, 4 and 7 units of rock in the column).

### 5.2 Range

With `b_lo ∈ [8, 15]` and `b_hi ∈ [0, 7]`, `t ∈ [0.5/15, 14.5/15]`: strictly
inside the edge, so no clamp is needed and no vertex sits on a corner.

### 5.3 Other properties

- **Shoreline points are on the terrain mesh,** and in settled water never
  from the fallback: a column whose ground is below the level is filled, so the
  face between it and the pool has no shore; a column whose ground is above it
  brackets the level. Checked on bowls at 60 levels in two steepnesses
  (`basins_at_every_level`).
- **No z-fighting.** The water is horizontal where the terrain slopes, so the
  two only meet along the waterline.
- **Cave pools and stacked layers in one column work**, because every rule is
  local.

---

## 6. Seams and vertex sharing

**Surface vertices.** Compute them as the integer corner position plus `t·Y`.
All four cells sharing a vertical edge then produce bitwise-equal points.

**Zero-area triangles are dropped.** When the level is exactly at a dry corner's
drawn ground, both faces next to that corner put their shoreline point on the
same terrain vertex, and the fan triangle between them has no area.

**Terrain vertices on faces.** Calling `edge_t` with the cell's own table edge
is **not** enough. Neighbouring cells reach the same physical edge through
different table edges, and some of those run in opposite directions. For
example, a vertical edge on an x-face is edge 1 `(1 → 3)` from one side and
edge 3 `(2 → 0)` from the other. `t` and `1 − t'` generally differ in the last
bits.

So evaluate each terrain vertex through the edge's **owner**: the cell at the
edge's lower corner, using table edge 0, 3 or 8 for X, Y or Z, exactly as
flying edges does. `EDGE_AXIS_ORIGIN` already maps a cell's table edge to that
identity. The shoreline points then lie on bitwise the same vertices the welded
terrain mesh has.

**Shoreline points.** Interpolate `P` between `A` and `B` in a fixed order (for
example, from the endpoint with the smaller coordinates). Both cells sharing the
face then compute the same `P`.

**Welding.** As with the terrain, welding decides the shading (§6.1): unwelded
water is faceted wherever it slopes, and welded water can be smooth. Start
unwelded like `marching_cubes.rs`. Add welding with an ownership rule the way
flying edges does: vertical edges owned like its Y slot, plus one owner per face
for the `F` points.

### 6.1 Normals

> **Deferred (2026-10-13).** Waves are planned, and they will most likely move
> the surface and work out its normals in the shader, which would make mesh
> normals moot. Until then, emit unwelded face normals (one cross product per
> triangle) and don't build the smooth option below.

The surface is **not** always flat. Each vertex's height comes from its own
vertical edge, from that edge's `b_lo + b_hi` through the volume-of-fluid
formula, the same way a marching-cubes vertex comes from its edge's corners, only
without linear interpolation. So:

- **In a settled pool**, every column has the same fill total, all vertices sit at the
  same height, and the normal is exactly +Y.
- **Everywhere else** the corners can differ: flowing water, a level step, or a
  fallback shoreline point on a face's top or bottom edge. The surface then
  tilts. It always faces upward, because every vertex only moves vertically, but
  the normal isn't +Y.

The two options mirror the terrain's two methods:

- **Unwelded: face normal per triangle**, as in `marching_cubes.rs`. This is
  exact, costs one cross product per triangle, and looks faceted on sloped
  water. A zero-area triangle falls back to +Y.
- **Welded: height-field gradient per vertex (recommended for smooth water).**
  Treat the surface heights as a function `H(x, z)` over vertical edges and take
  `n = normalize(−∂H/∂x, 1, −∂H/∂z)` by central differences with the four
  neighbouring vertical edges.
  - A neighbour column's surface edge is found by checking its vertical edges
    in rows `y − 1 ..= y + 1`, using the same rule as §3.2. That covers level
    steps.
  - If a neighbour has no surface edge on one side (a shore), use a one-sided
    difference. If it has none on either side, use 0 on that axis.
  - Shoreline points `F` take the normal of their wet edge, so the water doesn't
    tilt at the bank.

  Flat pools come out exactly +Y, right up to the shore. The gradient reads only
  samples, like the terrain's `gradient_normal`, so the vertex's owner computes
  it and the seams agree, given the same apron as the terrain.

The gradient of `b` itself (the terrain's approach, applied to occupancy) is not
recommended. The bank's solid counts as occupancy too, so normals near the shore
would tilt away from the bank even in a flat pool.

---

## 7. Worked examples

These are presets in `src/voxel_terrain/water/presets.rs`; each is a test and
a case in the `water_cell` demo. Pools are mostly at `9.5/15 ≈ 0.633`: ground
`s = 15` (or water `w = 15`) below, `w = 2` above. Heights are in cells above
the cell's bottom.

**A — open water.** Rock `s = 9` below, topped up with `w = 6` water (`b = 15`),
`w = 5` above. One flat quad at `(15 + 5 − 7.5)/15 = 12.5/15`. The terrain on
those edges is at `(9 − 7.5)/15 = 0.1`.

**B — top sample just inside.** Full below. With `w = 7` above the surface is at
`14.5/15`, near the top; with `w = 8` the top corner is inside and the surface
moves to the cell above.

**C — steep bank.** Water column (`w = 15` below, `w = 6` above, `H = 13.5/15 =
0.9`) beside a bank (`s = 15` below, `s = 12` above). On the face the terrain
runs from 0.5 across the bottom edge to 7.5/12 = 0.625 across the top, so the
shoreline is at `0.5 + 0.125 · 0.9 = 0.6125`.

**D — bank corner.** Three open-water slots at 9.5/15 and a bank slot (`s = 15`
below, `s = 6` above), drawn at `(15 + 6 − 7.5)/15 = 0.9` on its vertical edge.
A pentagon; on each face next to the bank the terrain runs from the bottom
edge's midpoint to the bank's vertex, and the shoreline is
`0.5 · (1 − 0.633/0.9) ≈ 0.148` from the bank.

**E — rock through the surface.** Solid at corners 0, 1, 3, 4, 5, water `w = 2`
on the ground at 2, 6, 7. A pentagon at 9.5/15 whose waterline climbs onto the
rock at corner 3. If corner 3 is water (`w = 15`) instead, no terrain brackets
the level and the fallback puts `F1` and `F2` on the top edges: unsettled water,
sloped.

**F — ledge.** Bottom corners full of water, pool at 9.5/15, a ledge at corner 6
holding the pool's water under it.
- `s_6 = 14` (room 1, holds 1): drawn at 8.5/15, below the pool. `15 + 1 + 14 =
  30`: left out, the shore meets the ledge; a pentagon.
- `s_6 = 13` (room 2, holds 2): drawn exactly at the pool's level. `15 + 2 + 13 =
  30`: left out; both shoreline points are the ledge's own vertex, and the
  pentagon closes into a quad.
- `s_6 = 10` (room 5, holds 2): drawn at 12.5/15. `15 + 2 + 10 = 27 < 30`: the
  slot carries the surface, one flat quad under the ledge. Filling the pocket
  rises a step per unit up to `w_6 = 4`; at 5 the slab is full and the water
  meets the drawn underside.

**Pocket under a rock.** Solid 0, 3, 4, 5; a pocket at corner 1 holding `w = 10`
(`(10 − 7.5)/15 = 2.5/15` under rock 3) and a pool with `w = 1` at 2, 6, 7
(`8.5/15`). Two bodies: a flat pentagon over slots 0, 1, 3 and a flat triangle
on slot 2, not one quad sloping between them.

**Level at a terrain vertex.** Water at `(15 + 3 − 7.5)/15 = 10.5/15` next to a
bank drawn at `(15 + 3 − 7.5)/15`: the shoreline point is the bank's vertex,
bit for bit.

---

## 8. Contract with the simulation

- **Each sample holds `s + w ≤ 15`.** Rock takes room in its slab.
- **Rock is settled bottom-up** in each column (a slab full before the one
  above holds rock), as heightfield data is. Otherwise the terrain itself is
  drawn oddly; that is the generator's and the editing tools' job, as for
  water.
- **Water is settled bottom-up on top of the rock:** water in a slab means the
  slab below is full (rock plus water). A ledge's water is under its rock.
- **Equalise the level** between neighbouring columns using the mesher's own
  height: the column's fill total, rock included. Water under a ledge is part
  of the pool and flows in sideways, up to the ledge's room.
- Move water only between columns whose levels differ by at least 2 units;
  settled pools otherwise show 1/15-cell terraces.
- Adding rock to a sample reduces its room; excess water must be displaced.
- `W_EPS` is in water units; at 15 units per cell it should be 0 or 1.

**Debug assertions:** `s + w ≤ 15` (the mesher checks it), `S & !O == 0`
(likewise), settled water `w_above > 0` implies `full(below)`, and total water
conserved between steps apart from sources and sinks.

---

## 9. Integration

- **Storage (done 2026-10-19).** `VoxelBuffer` holds `u8` samples on
  `0..=S_MAX`. The water mesher reads the same `s` buffer, so shoreline points
  sit on the terrain. `w` is its own buffer with the same X-major indexing, not
  interleaved with `s`: that keeps `VoxelBuffer::row` a contiguous slice for the
  scan's bitmask pass.
- **Implemented (branch `water-fill-model`):** `src/voxel_terrain/water/`
  (`extract`, `extract_with_overlap`, `describe_cell`, presets, tables) and the
  `water_cell` example. Not yet: the plugin and job queue, chunk rendering,
  aprons, skipping.
- **Shape.** Mirror `IsosurfacePlugin`: a job in and a renderer-agnostic surface
  out, with nothing touching `Assets<Mesh>` inside the mesher, so it stays
  testable outside the ECS.
- **Rendering.** One water mesh per chunk, on its own entity next to the terrain
  chunk's. Use `AlphaMode::Blend` and `cull_mode: None`, so the surface is
  visible from underwater. Normals as in §6.1.
- **Chunk borders** need the same apron as the terrain. The terrain doesn't have
  one yet: its gradient is one-sided at buffer faces.
- **Skipping cells.** Most of the speed comes from not visiting cells at all:
  - keep a per-chunk water count, so dry chunks are skipped entirely;
  - keep a per-column wet y-range, so only `[min − 1, max + 1]` is visited;
  - remesh only chunks the simulation marks dirty.

  The scan's row bitmasks could also be reused: pack `M` per row in a `u64`, and
  a row with no wet sample is skipped in one test.
- **Cost per cell.** Open water costs four adds and four table reads.
  Shoreline points cost one division and two terrain vertices each, and only
  occur in the thin band of shore cells.

---

## 10. Tests

All in `src/voxel_terrain/water/tests.rs` and `tables.rs`, plus two in the
terrain's own tests.

| test | expectation |
|---|---|
| `a_settled_column_is_drawn_at_its_fill_height` (terrain) | a bottom-up rock column is drawn exactly at its fill, for every height on the grid |
| `place_at_lands_where_the_longhand_formula_does` (terrain) | horizontal edges linear to 7.5 bit for bit, vertical edges the fill formula |
| `water_rests_on_the_terrain` | for every rock/water split, the water is where the terrain would draw rock filled as far |
| `column_fill_sweep` | one step per unit, no jump, with 0, 4 and 7 units of rock in the column |
| `rock_not_resting_below_doesnt_move_the_water` | rock held from the side or above never moves the water |
| `ledge_test_matches_the_drawn_ledge` | the integer ledge test agrees with the terrain's drawn underside, ties included |
| `ledges` | example F, and filling a pocket one step per unit |
| `a_settled_basin_is_flat_to_the_shore` | every vertex, shore included, at one height; every normal exactly +Y |
| `basins_at_every_level` | two bowls × 60 levels: flat, watertight, every shoreline point on the terrain |
| `shoreline_points_lie_on_the_terrain` | within 1e-5 of flying edges' mesh, overlap off |
| `neighbouring_cells_agree_on_their_shared_faces`, `a_level_step_has_no_crack` | each in-face edge used by exactly two triangles |
| `chunks_agree_on_their_shared_face` | two chunks put the same points on their shared face (1e-5) |
| `a_level_at_a_terrain_vertex_lands_on_it` | the shoreline point is the terrain vertex, bitwise |
| `a_sealed_pocket_and_the_pool_above_are_meshed_apart`, `corner_bodies_follow_open_edges` | two bodies in one cell, each flat at its own level |
| examples A–F, `surface_cases_give_their_table_shapes`, `every_preset_meshes` | §7 and the 16 cases |
| `face_segs_match_the_terrain` | for all 256 cases × 4 faces, `FACE_SEGS` gives the terrain's in-face segments |
| `ms_tris_are_convex_and_face_up`, `diagonal_cases_do_not_bridge`, `vert_t_covers_the_surface_range` | tables |

---

## Limitations

- **Two rock bodies on one vertical edge** -- floor rock in the lower sample and
  a ceiling over it -- are below grid resolution. The terrain counts the lower
  sample's rock as hanging from the ceiling, the water as resting on the floor;
  the ledge test can be off there.
- **Unsettled rock** (rock in a slab over one that isn't full) is drawn by the
  fill formula as if it were settled. Linear interpolation also misbehaves on
  such data, but differently.
- **Blocky mode** (`interpolate = false`) draws the terrain at edge midpoints,
  which the water's fill heights don't follow; the shoreline guarantee holds
  for interpolated terrain only.
- **Buffer bottom:** a lower corner in the buffer's bottom row is assumed to
  rest on ground. Needs the apron.
- **Chunk borders:** the terrain evaluates far-face vertices through the
  boundary cell's own table edge, so two chunks can differ in the last bits
  there until chunks get an apron.
- **Side faces aren't drawn by this pass.** That covers waterfalls, falling
  sheets and water standing against air. Level steps are joined by a sloped
  strip. A separate side-face pass can come later: marching cubes on `O`,
  keeping only the triangles whose inside corner is water-only.
- **Diagonal `Q` cases** produce two separate pools.
- **Concave shores inside one cell.** A cove's waterline can bend into the solid
  between two face points; the chord then cuts across water. The shore overlap
  hides most of it.
- **Terrain looks:** vertical-edge heights are on a 1/15 grid and follow the
  fill values, so fields written as ramps (the old test maps) look different
  from linear interpolation. The generator must write fill fractions.

---

## 11. Open questions

1. **Does the user want this model at all?** They have a different one in mind
   (2026-10-19). Everything on `water-fill-model` is Claude's.
2. Ceiling/floor attachment on edges with two rock bodies (Limitations).
3. Plugin, chunk rendering (`AlphaMode::Blend`, `cull_mode: None`), aprons,
   skipping (§9).
