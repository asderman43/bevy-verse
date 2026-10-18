# Water surface meshing

*Design. Nothing here is implemented yet. Rewritten 2026-10-13 from the original
proposal to fit this codebase's conventions, tables and configuration. Updated
2026-10-19 (§1.2): rock below the isolevel is ignored, and a solid point holds
the water standing under it.*

## Summary

The terrain is an isosurface of a solid field `s`. This adds a second
per-sample value, water `w`, and a mesher for the **top surface** of the water.
It has small lookup tables and does almost no arithmetic in open water.

Two ideas carry it:

- **Water height on a vertical edge.** Water fills bottom-up, so the surface
  height on a vertical cell edge is

  ```
  t = (o_lo + o_hi − W_ISO − 1) / W_MAX        o = solid ? W_MAX : w
  ```

  That is one add and one table read, with no division. It always lies in
  `[0, 1)`, and it is exactly volume-proportional: one more unit of water raises
  a settled surface by `1 / W_MAX` of a cell.
- **Shoreline on the terrain's own edges.** Each cell's top surface is a
  horizontal slice built from a 16-case table over its four vertical edges.
  Where the water meets a bank, the shoreline point goes where the flat water
  plane crosses the terrain mesh on that cell face. So the water stays flat up
  to the shore and meets the terrain on its surface.

The output is a separate mesh per chunk. Side faces (waterfalls, water standing
against air) are not part of this pass; see [Limitations](#limitations).

---

## 1. Configuration and data

### 1.1 Values

Each sample stores solid `s` and water `w`, both `u8`, in two separate buffers
with the same X-major indexing. The terrain and the water each have their own
range and isolevel, and the two pairs are independent. All four are
compile-time constants, not per-job or configured values:

| name | meaning |
|---|---|
| `S_MAX` | maximum solid value; **15** (`isosurface::S_MAX`) |
| `ISO` | terrain isolevel, **7** (`isosurface::ISO`); a sample is **solid when `s > ISO`**, as in the terrain mesher |
| `W_MAX` | maximum water value, **15**; one water unit raises the surface by `cs / W_MAX` |
| `W_ISO` | water isolevel, **7**; a non-solid sample is inside when `w > W_ISO` |

Chosen 2026-10-19: water uses the same scale as the terrain. `W_MAX` divides
`S_MAX`, so the capacity in §1.2 is exact (`15 − s`). Over a full wall with
empty space above, the terrain crosses at 1/2 (it interpolates to `ISO + 0.5`),
the water column's bottom is at 7/15 and the first unit of water at 8/15, just
above the terrain. One unit is 1/15 of a cell, and at most 97/240 ≈ 0.40 of a
cell can be hidden under the terrain (see Limitations). `W_EPS` should be 0 or
1.

In general the only checks are `1 ≤ W_MAX ≤ 255` and `0 ≤ W_ISO < W_MAX`. Rock is never converted into water
units, so nothing ties the two resolutions together. They stay compatible
because terrain vertices, and therefore shoreline points, only ever use `s` and
`ISO`, and water heights only ever use `w`.

**Default `W_ISO = (W_MAX + 1) / 2 − 1`** (integer division). That puts the
bottom of a water column halfway between a wall sample and the sample above it,
roughly where the terrain surface sits: 127 for `W_MAX = 255`, 4 for 10, 1 for 4.

### 1.2 Occupancy

```
solid  =  s > ISO                 // the terrain's own test
o      =  solid ? W_MAX : w       // for the inside test and as a lower corner
inside ⇔  o > W_ISO
```

- **A non-solid point holds only water.** Any rock in it is below the
  isolevel; the terrain never draws it, and the water ignores it: it takes no
  space and never moves the water's height.
- **A solid point (a ledge or wall) can hold the water standing *under* its
  rock**, up to the room its rock leaves:

  ```
  capacity(solid) = floor((S_MAX − s) · W_MAX / S_MAX)     // = W_MAX − ceil(s/K) when K = S_MAX/W_MAX is whole
  capacity(non-solid) = W_MAX
  ```

  Its `w` is only ever read as the **upper** corner of a vertical edge, by the
  ledge rule (§3.2). As a **lower** corner, a solid point counts as full
  (`o = W_MAX`): water above a ledge rests on the rock, so ledges can be fully
  submerged.

Why this shape:

- **The masks agree with the terrain.** `solid` is the terrain's own test, and a
  solid point has `o = W_MAX > W_ISO`, so the solid mask is a subset of the
  occupancy mask.
- **Filling never jumps.** A non-solid point's `o` is just its water, so a
  column rises exactly one step per unit, whatever invisible rock it holds
  (§5.2). The earlier `o = s + w` model added invisible rock to the height
  formula: it pushed water up by as much as half a cell, then dropped it back the
  moment the rock turned solid.
- **Pools stay flat under ledges.** The water under a ledge is stored at the
  ledge's own point, so the edge below the ledge can read the level directly
  (§3.2). Without it, the pocket under every ledge would be trapped air.

A point is **full** when its water reaches its capacity. Water can share space
with rock below the isolevel; that rock is invisible, so nothing shows it.
`W_EPS` is in water units.

### 1.3 Bottom-up invariant and the slab picture

**Settled water.** If a sample has `w > 0`, the sample directly below it is full.

The simulation can break this for a while (falling water, freshly dug terrain).
The mesher must stay well-defined when that happens, and the rules below do.

**Slab interpretation.** Each sample owns a vertical slab one cell (`cs`) tall.
The slab starts `δ = cs·(W_ISO + 1)/W_MAX` below the sample. A wall's slab is
full; a non-solid sample's slab fills from its floor with its water, so the
filled height is `cs·o/W_MAX`. In a settled column, the surface lies in the slab
of the topmost non-full sample `j`:

```
H = y_j − δ + cs·o_j/W_MAX  =  y_j + cs·(o_j − W_ISO − 1)/W_MAX
```

`H ≥ y_j` exactly when `o_j > W_ISO`, which is exactly when the sample counts as
inside. So the classification always puts the surface on the edge the slab
picture says it should be on.

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

For each cell, make one pass over its 8 corners and build three 8-bit masks.
Apply `W_EPS` first: treat `w < W_EPS` as 0.

| mask | bit `c` set when | used for |
|---|---|---|
| `S` | `s_c > ISO` | terrain contours on the faces (shoreline) |
| `O` | `o_c > W_ISO` | which vertical edges carry the surface; fallback contours |
| `M` | `w_c > 0` | wetness |

`S ⊆ O` always holds; debug-assert `S & !O == 0`. If `M == 0`, the cell has no
water: skip it.

### 3.2 Surface edges

A vertical edge **carries the water surface** when its lower corner is inside
`O`, its upper corner is outside `O`, and either corner holds water.

For each such edge, the surface vertex is `t = VERT_T[o_lo + o_hi]` up from the
lower corner. Call its height `H_k`. Collect these edges into a 4-bit mask `Q`.
If `Q == 0`, the cell has no top surface. This is the early out for almost every
cell.

**Water under a ledge.** An edge can have water at the bottom and a solid
point at the top. Both corners are inside `O`, so the rule above finds no
crossing. The ledge rule handles it when the upper corner is solid and the lower
corner is not solid, inside `O` (`w_lo > W_ISO`), and wet. The height ignores
the upper corner's rock and uses only its water, the water standing under it:

```
t = (w_lo + w_hi − W_ISO − 1) / W_MAX
```

This is the normal formula with the ledge's rock left out. Settled water only
puts water under the ledge (`w_hi > 0`) once the lower point is full, so as the
pocket fills, the level keeps rising one step per unit from the lower slab into
the space under the rock.

The height alone doesn't place the vertex; the terrain decides that. Compare
`t` with where the terrain draws the ledge on that edge,
`t_s = (ISO + 0.5 − s_lo)/(s_hi − s_lo)` up from the lower corner (the
terrain mesher interpolates to `ISO + 0.5`):

- `t < t_s`: the edge carries the surface at `t`, and the water stays flat under
  the ledge.
- otherwise the water reaches the drawn rock. Leave the edge out; the shoreline
  points on its faces (§3.3) put the waterline on the rock's surface. `t` can
  even exceed 1 when a thin ledge has a large pocket; it's dropped the same way,
  so no clamp is needed.

Do the comparison exactly, in integers:
`2·(w_lo + w_hi − W_ISO − 1)·(s_hi − s_lo) < W_MAX·(2·ISO + 1 − 2·s_lo)`, both
sides doubled to clear the half. In floats, near-ties come out one rounding step
off. With `W_MAX` odd, as at 15, the right side is odd and the left even, so
the two can never tie. `w_lo + w_hi < 2·W_MAX`, so `VERT_T` covers
it.

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

**Fallback.** Sometimes no terrain segment brackets `H_k`. This only happens in
unsettled states, for example when the neighbouring column's surface is in the
cell row above, or the neighbour is open air the water hasn't reached yet. In
that case, use the occupancy crossing on the edge that the `O` contour connects
to the wet edge. It comes from the same `FACE_SEGS` table, indexed by `O`'s face
bits, placed by plain interpolation on `o` from the inside corner `i`:
`t = (o_i − W_ISO) · RECIP[o_i − o_u]`. That point is on a cell edge, so the
cell across a level step computes the same one, and the surface stays watertight
when the level steps between rows.

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

### 3.5 Pseudo-code

```rust
for cell in chunk.cells_in_water_range() {
    let c = load_corners(cell);            // s, o, S/O/M masks; W_EPS applied first
    if c.m == 0 { continue; }
    debug_assert_eq!(c.s_m & !c.o_m, 0);

    // 1. vertical edges that carry the surface
    let mut q = 0u8;
    let mut v = [Vec3::ZERO; 4];
    for k in 0..4 {
        let (lo, hi) = VERT_EDGE[k];       // (4,6) (5,7) (1,3) (0,2)
        let crosses = bit(c.o_m, lo) && !bit(c.o_m, hi);
        let wet = bit(c.m, lo) || bit(c.m, hi);
        if crosses && wet {
            q |= 1 << k;
            let t = VERT_T[c.o[lo] as usize + c.o[hi] as usize];
            v[k] = cell.corner(lo) + Vec3::Y * t;
        }
        else if ledge_edge(&c, lo, hi) {   // §3.2: upper solid, lower not solid, inside, wet
            let t = VERT_T[c.w[lo] as usize + c.w[hi] as usize];
            if below_ledge(&c, lo, hi) {   // exact integer test against the terrain crossing
                q |= 1 << k;
                v[k] = cell.corner(lo) + Vec3::Y * t;
            }
        }
    }
    if q == 0 { continue; }

    // 2. shoreline points on faces with exactly one surface edge
    let mut fp = [Vec3::ZERO; 4];
    for f in 0..4 {
        let (ka, kb) = (f, (f + 1) % 4);
        if bit(q, ka) == bit(q, kb) { continue; }
        let k_wet = if bit(q, ka) { ka } else { kb };
        fp[f] = shore_point(cell, &c, f, k_wet, v[k_wet].y);
    }

    // 3. triangles
    for tri in MS_TRIS[q as usize] {
        emit(tri.map(|slot| match slot { Slot::V(k) => v[k], Slot::F(f) => fp[f] }));
    }
}

fn shore_point(cell, c, f, k_wet, h: f32) -> Vec3 {
    let mut best = None;
    for (ea, eb) in FACE_SEGS[face_bits(c.s_m, f)] {
        let a = terrain_vertex(cell, FACE_EDGES[f][ea]);   // canonical, §6
        let b = terrain_vertex(cell, FACE_EDGES[f][eb]);
        if (a.y - h) * (b.y - h) <= 0.0 && a.y != b.y {
            best = nearer_to_edge(best, canonical_lerp(a, b, h), k_wet);
        }
    }
    match best {
        Some(p) => p + SHORE_OVERLAP * toward_dry_edge(f, k_wet),
        None => occupancy_crossing_on_face(cell, c, f, k_wet),   // fallback
    }
}
```

---

## 4. Lookup tables

Generate every table, either in `const` or in the config constructor. Derive
them from `VERTEX_POSITIONS` and `EDGE_VERTEX_INDICES`, never from Bourke's
layout.

| table | size | contents |
|---|---|---|
| `VERT_EDGE[4]` | tiny | (lower, upper) corner per slot, as in §2 |
| `FACE_EDGES[4]`, `FACE_CORNERS[4]` | tiny | each vertical face's four table edges and four corners |
| `VERT_T` | `2·W_MAX + 1` × f32 | `VERT_T[k] = (k − W_ISO − 1) / W_MAX`; depends on the config, so build it per config |
| `FACE_SEGS[16]` | tiny | for a face's 4-bit inside mask, the pairs of face edges joined by a contour segment (at most 2) |
| `MS_TRIS[16]` | tiny | triangles over `V0–V3`, `F0–F3` (§3.4) |
| `RECIP` | `W_MAX + 1` × f32 | `1 / d` for `d ≥ 1`; fallback only |

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

### 5.1 The height formula

Take a vertical edge that carries the surface, so `o_lo > W_ISO ≥ o_hi`. Under
the invariant there are two settled situations:

- **The upper sample holds water.** Then the lower one is full
  (`o_lo = W_MAX`), and the surface is in the lower part of the upper sample's
  slab: `t = (W_MAX + o_hi − W_ISO − 1) / W_MAX`.
- **The lower sample is partly filled.** Then the upper one is empty
  (`o_hi = 0`), and the surface is in the upper part of the lower sample's slab:
  `t = (o_lo − W_ISO − 1) / W_MAX`.

Both are the same expression, `t = (o_lo + o_hi − W_ISO − 1) / W_MAX`, with no
case split and no division.

The second case relies on the upper sample being truly empty when the lower one
isn't full. Water guarantees that, since it falls. Rock wouldn't: it can sit in
the upper sample while the lower one is still filling. That is why non-solid
rock must not count toward `o` (§1.2): otherwise the formula adds it as if it
were stacked on the water.

### 5.2 Range and continuity

With `o_lo ∈ [W_ISO+1, W_MAX]` and `o_hi ∈ [0, W_ISO]`, the numerator lies in
`[0, W_MAX − 1]`. So `t ∈ [0, (W_MAX−1)/W_MAX]` on every surface edge, even when
the invariant is broken, and no clamp is needed.

As a column fills unit by unit, the surface rises exactly `cs / W_MAX` per unit.
That includes the moment the top sample becomes inside and the vertex moves to
the next edge up: at `o = W_ISO` it sits at `t = (W_MAX−1)/W_MAX` on the lower
edge, and at `o = W_ISO + 1` at `t = 0` on the upper edge, one unit higher.
Checked for every `W_MAX` from 1 to 255 on columns holding no rock, some rock
and the most invisible rock (`s = ISO`): always exactly one step per unit.

For comparison, plain interpolation on `o`, with a full lower corner, would give
`(W_MAX − W_ISO)/(W_MAX − o_hi)`. That isn't linear in volume, so surfaces would rise unevenly, and it
costs a division.

### 5.3 Other properties

- **Flat pools stay flat.** In settled water, every column's top sample has the
  same `o`, so every vertex, shoreline points included, is at the same height.
- **The water meets the terrain on its surface.** Every terrain-derived
  shoreline point lies on a terrain mesh edge. Where the terrain near the shore
  is one triangle (the usual bank-corner case), the chord between the two face
  points is exactly the plane's intersection with it.
- **No z-fighting.** The water is horizontal where the terrain slopes, so the
  two only meet along the waterline.
- **Cave pools and stacked layers in one column work**, because every rule is
  local.

---

## 6. Seams and vertex sharing

**Surface vertices.** Compute them as the integer corner position plus `t·Y`.
All four cells sharing a vertical edge then produce bitwise-equal points.

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
vertical edge, from that edge's `o_lo + o_hi` through the volume-of-fluid
formula, the same way a marching-cubes vertex comes from its edge's corners, only
without linear interpolation. So:

- **In a settled pool**, every column has the same `o`, all vertices sit at the
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

The gradient of `o` itself (the terrain's approach, applied to occupancy) is not
recommended. The bank's solid counts as occupancy too, so normals near the shore
would tilt away from the bank even in a flat pool.

---

## 7. Worked examples

All examples use `S_MAX = 255`, `ISO = 127`, `W_MAX = 255` and `W_ISO = 127`, so
a corner is solid from `s = 128` up, water is inside from `w = 128` up, and
`t = (o_lo + o_hi − 128) / 255`. Heights are in cells.

**A — open water.**

- Lower corners: `s = 140`, solid, so they are walls with `o = 255`. Upper
  corners: `s = 0, w = 80`.
- All four vertical edges carry the surface, so `Q = 1111`: one quad.
- Water: `t = (255 + 80 − 128)/255 = 207/255 ≈ 0.81`. Terrain on the same edges:
  `(140 − 127)/140 ≈ 0.09`.

The result is a flat quad with about 0.72 cells of visible depth.

**B — top sample just inside.** The sample below is full and the top sample has
`s = 0, w = 129`. It counts as inside, so the surface is on the edge above it:
`t = (129 + 0 − 128)/255 ≈ 0.004`.

**C — steep bank.** A face joins an open-water column A and a bank column B.

- A: lower `s = 0, w = 255`; upper `w = 100`, so `H = 227/255 ≈ 0.89`.
- B: lower `s = 255`, upper `s = 200`; both solid.
- The terrain polyline runs from the bottom-edge vertex (0.498 of the face
  width from A, height 0) to the top-edge vertex (0.635 from A, height 1).
- At height 0.89 that is 0.620 from A: the shoreline point.

Snapping to the top-edge vertex instead would lift the water's corner 0.11 cells
and tilt the surface up the bank.

**D — bank corner.** Columns A, B and C are open water at `H = 0.6` (lower
`w = 255`, upper `w = 26`). Column D is the bank: lower `s = 255`, upper
`s = 100`, so the terrain crosses D's vertical edge at 0.826. D has no water,
so it isn't in `Q`.

- `Q` = {A, B, C}: a pentagon.
- On face (A, D), only D's lower corner is solid, so the terrain is one segment.
  It runs from the bottom-edge vertex (0.502 from D, height 0) to D's
  vertical-edge vertex (height 0.826).
- At height 0.6 the shoreline point is 0.137 of the face width from D. Face
  (C, D) is symmetric.

Snapping to D's terrain vertex would raise one corner by 0.23 cells. Here two
shoreline points at the water height replace it, and because the terrain in that
corner is one triangle, the cut is exact.

**E — rock through the surface.** Inside corners are 0, 1, 3, 4 and 5 (case 196
in the terrain's empty-bit indexing): solid ground at 0, 1, 4 and 5, and a rock
at corner 3 = (1, 1, 0). Water stands on the ground up to `H`, so corners 2, 6
and 7 are wet.

- Slots 0 `(4,6)`, 1 `(5,7)` and 3 `(0,2)` carry the surface. Slot 2 `(1,3)`
  doesn't, because both of its corners are inside.
- The walk gives `V0, V1, F1, F2, V3`: a pentagon.
- Face 1 (x = 1) has corners 1, 3, 5 solid and 7 not, so its terrain segment
  joins edge 5 (ground, below `H`) and edge 10 (the rock's top edge, above `H`).
  `F1` is where the water meets the rock. Face 2 (z = 0) works the same way
  with edges 3 and 2.
- If corner 3 were inside because of water instead (unsettled), no terrain
  segment would bracket `H`. The fallback would then put `F1` on edge 10 and
  `F2` on edge 2, which is marching cubes' own pentagon for this case.

**F — ledge reaching into the water.** The bottom corners 0, 1, 4 and 5 are
full of water. Corners 2, 3 and 7 hold water up to `H`. Corner 6 = (0, 1, 1) is
solid, a ledge over the column at corner 4, and the terrain crosses edge 7
`(6, 4)` at height `h`.

- *Ledge dips into the water (`h < H`).* Slot 0 `(4,6)` doesn't carry the
  surface: the ledge rule of §3.2 finds the level at or above the drawn ledge
  and leaves the edge out. Faces 0 (z = 1) and
  3 (x = 0) each have only corner 6 solid, so each terrain segment runs from
  edge 7 (at `h`, below `H`) to the top edge, 6 or 11 (above `H`). `F0` and
  `F3` land on the ledge at the water height, and the walk gives
  `F0, V1, V2, V3, F3`: a pentagon.
- *Ledge above the water (`h > H`).* Corner 6 holds the water standing under
  it. The ledge rule reads `t = (w_4 + w_6 − 128)/255`, finds it below `h`, and
  slot 0 carries the surface at the pool's level: one flat quad continuing under
  the ledge. While the pocket is still filling (`w_6` small, or corner 4 not yet
  full), the level there is lower than the pool's and rises one step per unit;
  that is unsettled water, and the simulation levels it.

---

## 8. Contract with the simulation

The mesher needs only the capacity rule (§1.2) and the bottom-up invariant for
settled water: water at a point means the point below is full. A solid point
below counts as full.

- **Equalise the level** between neighbouring columns, using the mesher's own
  height `H = y_j + cs·(w_j − W_ISO − 1)/W_MAX`, where `j` is the topmost wet,
  not-full point of the column. If `j` is solid, `w_j` is the water standing
  under its rock, and the same formula gives that water's level. Rock never
  enters it.
- **Water under a ledge is part of the pool.** It flows in sideways from
  neighbouring columns, like any other water, up to the ledge point's capacity.
- **Rock below the isolevel changes nothing.** It doesn't take space, so it
  needs no displacement.
- Move water only between columns whose levels differ by at least 2 units. At
  low `W_MAX`, settled pools show `1/W_MAX`-cell terraces; raise `W_MAX` if they
  are visible.
- When terrain is dug out under water, the invariant breaks until the water
  falls. The mesher stays in range and continuous through that, and the §3.3
  fallback joins the level steps.
- If an edit adds rock to a solid point, or makes a point solid, its capacity
  drops; excess water must be displaced before the capacity check runs.
- `W_EPS` is in water units. At low `W_MAX` it should be 0 or 1: at
  `W_MAX = 4`, a value of 2 hides half a cell of water.

**Debug assertions:**
- `s ≤ S_MAX` and `w ≤ capacity(s)`
- for settled water, `w_above > 0` implies `full(below)`
- total water is conserved between steps, apart from sources and sinks
- `S & !O == 0` in every cell

---

## 9. Integration

- **Storage (done 2026-10-19).** `VoxelBuffer` holds `u8` samples on
  `0..=S_MAX`, and the terrain mesher classifies and interpolates against the
  constant `ISO`. The water mesher reads the same `s` buffer, so shoreline points
  sit on the terrain. `w` goes in its own buffer with the same X-major indexing,
  not interleaved with `s`: that keeps `VoxelBuffer::row` a contiguous slice for
  the scan's bitmask pass.
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

| test | expectation |
|---|---|
| Rock independence | changing `s` anywhere in `0..=ISO` changes no water vertex |
| Masks | for every `s`, `w`: a solid corner is inside `O` (`S ⊆ O`) |
| Height range | for every `o_lo ∈ (W_ISO, W_MAX]`, `o_hi ∈ [0, W_ISO]`: `VERT_T[o_lo + o_hi] ∈ [0, 1)` |
| Column fill sweep | filling one column unit by unit raises the surface `1/W_MAX` per unit, with no jump at the isolevel, including columns holding rock up to `s = ISO` |
| Ledge | §7 F, both cases; filling a pocket under a ledge rises one step per unit; the integer test agrees with exact arithmetic, ties included |
| Example A | one quad, all four vertices at exactly `t = 207/255` |
| Flat basin | in a settled basin, every water vertex, shoreline points included, has the same y |
| Normals | settled basin: every normal is exactly +Y, shore included. Sloped surface: every normal is unit length, has y > 0, and leans the same way as each face using it |
| Shoreline on terrain | with `SHORE_OVERLAP = 0`, every terrain-derived shoreline point lies on a segment of flying edges' own mesh (distance < 1e-5) |
| Vertex special case | water level exactly at a terrain vertex's height gives a shoreline point equal to that vertex |
| `FACE_SEGS` matches the terrain | for every cell case and vertical face, `FACE_SEGS[S bits]` gives the same segments as the in-face edges of `TRIANGLE_TABLE[!S]` |
| Examples C, D, E, F | the shapes and distances in §7 |
| Seams | shoreline points and surface vertices are bitwise equal between neighbouring cells and chunks |
| Level step | neighbouring columns whose surfaces are in different rows produce no crack |
| Convexity | every `MS_TRIS` polygon is convex in top view for random `F` positions, all 16 cases |
| Invariant | `S & !O == 0` never fails |

---

## Limitations

- **Thin water can be drawn under the terrain. This is accepted for now
  (2026-10-13).** Water heights come from the slab model, but terrain vertices
  use linear interpolation, and the two agree on classification, not position.
  The worst case is a full wall below and invisible rock right at the isolevel
  above (`s_lo = S_MAX`, `s_hi = ISO`): the terrain is drawn high on the edge,
  at 15/16, while one unit of water sits at 8/15. The most that can be hidden is
  97/240 ≈ 0.40 of a cell, checked exhaustively over vertical edges at
  15 / 7 / 15 / 7. (Interpolating the terrain to `ISO`, it was `W_ISO / W_MAX`
  = 7/15, with the terrain drawn at the very top of the edge.) With `interpolate = false` the terrain sits at the
  midpoint instead. If it ever needs fixing: lift `H_k` to the terrain crossing
  on that edge (this breaks flatness), or place the terrain's vertical-edge
  vertices by the slab model too (this changes how the terrain looks).
- **Side faces aren't drawn by this pass.** That covers waterfalls, falling
  sheets and water standing against air. Level steps are joined by a sloped
  strip. A separate side-face pass can come later: marching cubes on `O`,
  keeping only the triangles whose inside corner is water-only.
- **Diagonal `Q` cases** produce two separate pools.
- **Films** thinner than `W_EPS` aren't drawn.
- **Concave shores inside one cell.** A cove's waterline can bend into the solid
  between two face points. The chord then cuts across water and can leave a
  sliver of uncovered terrain; convex rocks come out exact through the depth
  test. If coves show artifacts, an exact mode can clip the cell's terrain
  triangles at `H` and ear-clip the resulting polygon, in shore cells only.
- **Sub-cell features.** Each edge holds one crossing per field. Two thin layers
  on one edge, such as a thin ledge with water above and below it, are below
  grid resolution.

---

## 11. Open questions

1. ~~**Storage migration** (§9)~~: done 2026-10-19, with `S_MAX = 15`,
   `ISO = 7` as constants; `W_MAX = 15`, `W_ISO = 7` chosen the same day, and
   the terrain now interpolates to `ISO + 0.5`. The worked examples in §7 still
   use 255 / 127 and interpolation to `ISO`, and need redoing on both counts.
