# Water surface meshing

*Design. Nothing here is implemented yet. Rewritten 2026-10-13 from the original
proposal to fit this codebase's conventions, tables and configuration.*

## Summary

The terrain is an isosurface of a solid field `s`. This adds a second
per-sample value, water `w`, and a mesher for the **top surface** of the water.
It has small lookup tables and does almost no arithmetic in open water.

Two ideas carry it:

- **Water height on a vertical edge.** Water fills bottom-up, so the surface
  height on a vertical cell edge is

  ```
  t = (o_lo + o_hi − W_ISO − 1) / W_MAX        o = ceil(s / K) + w
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

Each sample stores solid `s` and water `w`, both `u8`.

| name | meaning |
|---|---|
| `S_MAX` | maximum solid value, `1..=255` |
| `ISO` | terrain isolevel; a sample is **solid when `s > ISO`**, as in the terrain mesher |
| `W_MAX` | maximum water value; one water unit raises the surface by `cs / W_MAX` |
| `K = S_MAX / W_MAX` | solid units per water unit |
| `W_ISO = ISO / K` | the isolevel on the water side |

All three are configurable. Check them once in a config constructor that
returns `Result`:

```rust
S_MAX % W_MAX == 0     // a water unit is a whole number of solid units
ISO % K == 0           // the isolevel lands on a water-unit boundary
ISO < S_MAX            // some value is solid
```

The isolevel does not have to sit in the middle of the range. Because of
`ISO % K == 0` and `ISO < S_MAX`, every non-solid sample has room for at least
one water unit.

| `S_MAX` | `ISO` | `W_MAX` | `K` | `W_ISO` | valid? |
|---|---|---|---|---|---|
| 16 | 8 | 4 | 4 | 2 | ✓ |
| 16 | 7 | 16 | 1 | 7 | ✓ (with `K = 1`, any `ISO` works) |
| 16 | 7 | 4 | 4 | — | ✗ (7 % 4 ≠ 0) |
| 240 | 120 | 8 | 30 | 4 | ✓ |
| 255 | 127 | 255 | 1 | 127 | ✓ (used in the examples) |

`S_MAX = 255` only divides by 3, 5, 15, 17, 51 and 85, so few water resolutions
fit it. 240 or a power of two gives many more choices; with powers of two, `K`
is also a shift.

### 1.2 Occupancy

The water side sees solid **rounded up** to water units:

```
o = ceil(s / K) + w          // in 0..=W_MAX
inside  ⇔  o > W_ISO
```

Rounding up gives two exact properties:

- **Same masks.** For integers, `ceil(s/K) > ISO/K` holds exactly when
  `s > ISO`. So a sample is solid on the water side exactly when the terrain
  mesher thinks it is, and the solid mask is a subset of the occupancy mask.
- **No overlap.** Water never shares volume with solid:
  `ceil(s/K)·K + w·K ≤ S_MAX` implies `s + K·w ≤ S_MAX`.

A sample's **capacity** is `W_MAX − ceil(s / K)`. It is **full** when
`o == W_MAX`. All water-side quantities, `W_EPS` included, are in water units.
Terrain vertices, and therefore shoreline points, still use full-resolution `s`
and `ISO`.

### 1.3 Bottom-up invariant and the slab picture

**Settled water.** If a sample has `w > 0`, the sample directly below it is full.

The simulation can break this for a while (falling water, freshly dug terrain).
The mesher must stay well-defined when that happens, and the rules below do.

**Slab interpretation.** Each sample owns a vertical slab one cell (`cs`) tall.
The slab starts `δ = cs·(W_ISO + 1)/W_MAX` below the sample. Material fills the
slab from its floor, solid first and then water, so the filled height in the
slab is `cs·o/W_MAX`. In a settled column, the surface lies in the slab of the
topmost non-full sample `j`:

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

**Water under an overhang.** An edge can have water at the bottom and solid at
the top with air in between. Occupancy sees both corners inside and finds no
crossing, so the water would seem to climb the overhang. The case applies when
the lower corner is wet and inside `O`, the upper corner is solid
(`s_hi > ISO`), and the lower corner is not solid. How to find the water height
on such an edge is still open; see [§11](#11-open-questions).

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
        // overhang edges: see §11
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

### 5.2 Range and continuity

With `o_lo ∈ [W_ISO+1, W_MAX]` and `o_hi ∈ [0, W_ISO]`, the numerator lies in
`[0, W_MAX − 1]`. So `t ∈ [0, (W_MAX−1)/W_MAX]` on every surface edge, even when
the invariant is broken, and no clamp is needed. This was checked exhaustively
for every valid configuration up to 255.

As a column fills unit by unit, the surface rises exactly `cs / W_MAX` per unit.
That includes the moment the top sample becomes inside and the vertex moves to
the next edge up: at `o = W_ISO` it sits at `t = (W_MAX−1)/W_MAX` on the lower
edge, and at `o = W_ISO + 1` at `t = 0` on the upper edge, one unit higher.

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

All examples use `S_MAX = W_MAX = 255`, `ISO = 127` (`K = 1`, `W_ISO = 127`), so
a sample is inside from 128 up and `t = (o_lo + o_hi − 128) / 255`. Heights are
in cells.

**A — open water.**

- Lower corners: `s = 140, w = 115`, so `o = 255`. Upper corners: `s = 0, w = 80`.
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
  surface, because both of its corners are inside. Faces 0 (z = 1) and
  3 (x = 0) each have only corner 6 solid, so each terrain segment runs from
  edge 7 (at `h`, below `H`) to the top edge, 6 or 11 (above `H`). `F0` and
  `F3` land on the ledge at the water height, and the walk gives
  `F0, V1, V2, V3, F3`: a pentagon.
- *Ledge above the water (`h > H`).* There is air between the water and the
  ledge, which is the overhang case of §3.2 and still open ([§11](#11-open-questions)).
  Without a rule for it, slot 0 drops out of `Q` and the fallback tilts the
  water up toward the ledge.

---

## 8. Contract with the simulation

The mesher needs only the capacity rule (`w ≤ W_MAX − ceil(s/K)`) and the
bottom-up invariant for settled water.

- **Equalise `o`, not `w`,** between neighbouring columns, using the mesher's own
  height `H = y_j + cs·(o_j − W_ISO − 1)/W_MAX`. Solid shares the slab, so equal
  `w` doesn't mean equal height.
- Move water only between columns whose levels differ by at least 2 units. At
  low `W_MAX`, settled pools show `1/W_MAX`-cell terraces; raise `W_MAX` if they
  are visible.
- When terrain is dug out under water, the invariant breaks until the water
  falls. The mesher stays in range and continuous through that, and the §3.3
  fallback joins the level steps.
- If an edit raises `s`, it lowers capacity, so excess water must be displaced
  before the capacity check runs.
- `W_EPS` is in water units. At low `W_MAX` it should be 0 or 1: at
  `W_MAX = 4`, a value of 2 hides half a cell of water.

**Debug assertions:**
- `s ≤ S_MAX` and `w ≤ W_MAX − ceil(s/K)`
- for settled water, `w_above > 0` implies `full(below)`
- total water is conserved between steps, apart from sources and sinks
- `S & !O == 0` in every cell

---

## 9. Integration

- **Prerequisite: storage.** `VoxelBuffer` holds `i8` today, and
  `edge_t` / `case_index` take an `i8` isolevel. Water needs `u8` `(s, w)`
  samples with the configurable `S_MAX` / `ISO` above, and the terrain mesher has
  to read the same `s` values, or shoreline points won't sit on the terrain.
  Store `(s, w)` as packed pairs, X-major like `VoxelBuffer`.
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
| Resolution rules | for every valid `(S_MAX, ISO, W_MAX)` and every `s`: `(s > ISO) == (ceil(s/K) > W_ISO)`, and `ceil(s/K)·K ≥ s` |
| Height range | for every `o_lo ∈ (W_ISO, W_MAX]`, `o_hi ∈ [0, W_ISO]`: `VERT_T[o_lo + o_hi] ∈ [0, 1)` |
| Column fill sweep | filling one column unit by unit raises the surface `1/W_MAX` per unit, with no jump at the isolevel |
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
  Take a solid corner `s` with empty air above: the terrain sits at
  `(s − ISO)/s`, while the slab model puts the solid top at
  `(s − ISO − 1)/S_MAX`, which is lower. Water thinner than that gap ends up
  inside the rock. The gap peaks at about 0.09 cells for `255 / 127` and about
  0.15 cells for `16 / 8`, and it is much larger with `interpolate = false`
  (terrain at the midpoint). If it ever needs fixing: lift `H_k` to the terrain
  crossing on that edge (this breaks flatness), or place the terrain's
  vertical-edge vertices by the slab model too (this changes how the terrain
  looks).
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

1. **Water height under an overhang (§3.2).** The original rule computed
   `(o_lo + w_hi − …)/W_MAX` from the edge alone. In the slab model, though, a
   solid upper sample can't hold the water *under* its solid, so that formula
   doesn't give the pool level unless the simulation stores under-ledge water in
   the ledge sample. One local alternative: in settled water all surface
   vertices share `H`, so take `H` from a surface edge of the same cell, and
   leave the edge out when there is none. That needs checking for agreement
   between the cells sharing the edge when the water is unsettled.
2. **Storage migration** (§9): the terrain moves to `u8` with configurable
   `S_MAX` / `ISO` before the water mesher can share its vertices.
