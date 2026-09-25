# Plan: hard edges for flying edges

Status: **designed, not started.** Branch: `isosurface_extractor`. This file
replaces the earlier "corner vs gradient" plan, which is kept below only as the
fallback (see Decisions).

## 1. Goal

Flying edges produces a fully welded, smooth surface. Add an optional
**crease angle**. Any mesh edge whose two adjacent faces have normals more than
that angle apart is **hard**. Its vertices are duplicated so that each side of
the edge gets its own normal. Everything else stays welded and smooth.

Marching cubes stays the fast, fully hard-faced option. Its algorithm is
unchanged; it only starts writing normals.

Every claim marked *verified* below was checked against the real `scan` /
`emit` code or by exhaustive enumeration of `TRIANGLE_TABLE`, using throwaway
code in the session scratchpad (`adj/`, `v/`, `fans.py`, `faces.py`,
`sim.py`, `review_fan.py`). No project source was changed. The checks that
matter become permanent tests in section 7.

## 2. Decisions

| Topic | Decision |
|---|---|
| Hard-edge test | **Face pairs.** For a mesh edge, compare the two faces that share it. The same two faces are compared at both ends of the edge, so the decision is consistent. |
| Face normals | **No buffer.** A face's normal is recomputed on demand from its 3 vertex positions, which are final after pass 4. |
| Finding a face's vertices | **Random-access vertex IDs** from the scan data (popcount on row masks). No stepping, no stored IDs. |
| Finding the faces around a vertex | **Quadrant ring.** Walk the up-to-4 cells around the vertex's grid edge in cyclic order. In each cell, walk that vertex's fan of triangles in chain order, using two small const tables (`FAN`, `CORNER_FAN`). |
| Copies per vertex | Closed ring: `max(1, hard edges)`. Open ring (volume boundary): `1 + hard edges`. Only hard edges add copies. |
| Where copies live | Copy 0 **is** the welded slot. Copies 1.. go in a per-vertex-row "extras" block after all welded vertices. Welded IDs never change, and there are no orphans. |
| Smooth normals | **Density gradient**, written by the owner cell in pass 4: central differences at the edge's two corners, lerped by `t`, negated, with a fallback to the edge direction. |
| Split-copy normals | **Area-weighted average** of the group's faces: the normalised sum of the raw cross products. This deviates from "normalised average of unit normals", with justification in 4.8. |
| Per-vertex split data | **Stashed in the split vertex's own normal slot** between passes 5 and 6: 0 bytes of scratch. Threading will need a 4 B/vertex side buffer instead (see 4.6). |
| Normals output | Written by us. `Surface` gains `normals`, and `into_mesh` no longer calls `with_computed_normals`. |
| Marching cubes | Algorithm unchanged. It writes each triangle's face normal onto its 3 vertices. It ignores `crease_angle`. |
| API | `ExtractionJob.crease_angle: Option<f32>`, in degrees. `None` gives today's smooth output, with gradient normals. |
| Degenerate faces | A zero-area face is **never** part of a hard edge, and has zero weight in group sums. There is no branch for this; it falls out of the test's form. |
| Principles kept | Count-then-write with exactly sized buffers. Each output slot has one owner. Row-contiguous spans that stay `split_at_mut`-partitionable. The `ExtractionScratch` pool never shrinks. |

### Rejected alternatives

| Alternative | Why rejected |
|---|---|
| **4 quadrant normals per vertex** (one slot per quadrant cell, 48 B/vertex, a 256-entry split table) | Too much memory for every vertex. It also only splits on quadrant boundaries, not on actual mesh edges. |
| **Per-face normal buffer** (12 B/triangle) | Extra memory. A row-rolling version still needs about one z-plane of faces, around 238 KB at 64³, which is effectively the same buffer. Recomputing a cross product from 3 cached positions is cheap. |
| **Triangle-walk adjacency tables** (`EDGE_ACROSS`/`WALL_ACROSS`, 9,984 B) | Work, but duplicate the job of the quadrant ring. The ring already crosses cell walls, because consecutive quadrant fans meet on their shared face segment. The exhaustive checks behind these tables are kept as regression tests. |
| **Shader-based creases** (e.g. flat shading from derivatives, or a crease flag attribute) | Moves the decision to the GPU on every frame, and can't give a smooth surface and hard edges in one mesh without extra attributes. It also breaks `Surface` as renderer-agnostic data. |
| **Corner vs gradient**, the old plan. **Kept as the FALLBACK.** | A triangle corner is hard when its face normal differs from the vertex's gradient normal by more than the angle. Each hard corner gets its own ungrouped vertex. It needs no adjacency at all, just a count pass and a write pass over the index spans. It is cheaper but gives ragged, facet-by-facet crease bands, can leave orphaned welded vertices, and decides each corner independently, so the two ends of an edge can disagree. Switch to it only if pass 5's cost turns out unacceptable (see section 5). |

## 3. API changes

```rust
pub struct ExtractionJob {
    pub buffer: VoxelBuffer,
    pub isolevel: i8,
    pub method: Method,
    pub interpolate: Interpolate,
    /// Flying edges only; marching cubes ignores it. Degrees.
    /// `None` (or >= 180, or NaN) = fully welded and smooth.
    /// Two faces sharing a mesh edge whose normals differ by more than this
    /// get separate vertices along that edge. See the doc note below on
    /// choosing a value.
    pub crease_angle: Option<f32>,
}

pub struct ExtractionResult { /* ... */ pub crease_angle: Option<f32>, /* echoed */ }

pub struct Surface {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,   // new: same length as positions, unit, finite
    pub indices: Vec<u32>,
}
```

- `Surface` invariant doc: `normals.len() == positions.len()`; every normal is finite and unit length within 1e-4; every vertex is referenced by at least one triangle, for both methods, crease on or off (the design has no orphans, see 4.5). Add: "with a crease, flying edges duplicates vertices along hard edges".
- `Surface::into_mesh` inserts `ATTRIBUTE_POSITION`, `ATTRIBUTE_NORMAL` and the indices, and drops `with_computed_normals`. Its doc no longer says normals are computed.
- `pub(crate) fn crease_cos(angle: Option<f32>) -> Option<f32>`:
  - `None`, NaN (with a `debug_assert!`) and `>= 180.0` all return `None`, so passes 5 and 6 are skipped. At exactly 180°, rounding would otherwise let folded faces (pseudo_random has exact folds) test hard.
  - Otherwise it returns `Some(a.max(MIN_CREASE).to_radians().cos())`, with `MIN_CREASE = 0.5`. At 0° every coplanar pair would split or not depending on rounding (review m1), so 0° is clamped and the doc says so.
- **Doc note on choosing an angle.** This is a real limitation, not a bug (verified, section 10 R1). Marching cubes can't represent an edge inside a cell: a φ° feature comes out as a chamfer strip bent by about φ/2 at each border. A 90° box edge becomes a 45° bevel, and the corner facets meet the bevels at 35.26°. Meanwhile, smooth curved surfaces show face-to-face angles of up to 50–55° from staircasing. So set the crease below φ/2 to catch φ° features, and expect false creases on curved surfaces below about 55°. **40° is the documented and demo default.**
- Literal call sites that need the new field (`ExtractionJob` has no `Default`): `tests.rs:152, 233, 256, 281, 296, 311, 342`, `debug.rs:37`, `examples/isosurface_bench.rs:86`, the doc example at `mod.rs:23`, and the `ExtractionResult` literal in `drain_extraction_queue` (`mod.rs` ~314).
- `extract_with` passes `job.crease_angle` to `flying_edges::extract_into`, which gains the parameter. `flying_edges::extract` gains it too, and existing test callers pass `None`.

## 4. Design

### 4.1 Vertex IDs by random access (verified correct, no correction needed)

`r = row_index(size, y, z) = y + z*size`, `m = corner_masks`, `low(n) = (1 << n) - 1`.
The row is always the row of the edge's **lower** endpoint.

| Axis | ID of the vertex on grid edge `(x,y,z)` | Valid for |
|---|---|---|
| X | `offsets[r].x + popcnt((m[r] ^ m[r] >> 1) & low(x))` | `x <= size-2` |
| Y | `offsets[r].y + popcnt((m[r] ^ m[r + 1]) & low(x))` | `y <= size-2`, any `x <= size-1` |
| Z | `offsets[r].z + popcnt((m[r] ^ m[r + size]) & low(x))` | `z <= size-2`, any `x <= size-1` |

- **Verified two ways.** First, the formula equals emission's stepped `ids[s]` for every cut slot of every in-trim cell: about 1.5 M references over 10 fields, 0 mismatches. Second, it is a bijection from all cut grid edges onto `0..V`. This covers the hand-off rows at `y = size-1` (Z edges only) and `z = size-1` (Y edges only), row `(size-1, size-1)` (X edges only), and the far-X Y/Z edges at `x = size-1` from `last_cell`.
- **Why it holds.**
  - Pass 3 lays each row out as X, then Y, then Z.
  - Pass 2 counts, and emission steps through, each row's Y/Z edges in ascending x. At `last_cell`, slot 5 comes before 9.
  - A hand-off row gets its edges only from the single adjacent cell row, also in ascending x.
  - The trim never drops a cut edge. A cut edge has a set corner, so some bounding row's `touched` mask covers its cell. The prefix count from x = 0 therefore equals emission's count from `left_trim`.
- **Notes.**
  - For X, `& edge_mask` is redundant, because `low(x)` with `x <= size-2` never reaches bit `size-1`.
  - For Y and Z, `edge_mask` must **not** be applied, because their far-X edges sit at `x = size-1`.
  - Masks have no bits at or above `size`.
  - `low(63)` is fine, so `size = MAX_SIZE = 64` works, and the existing `low_bits` guard is unneeded but harmless.
- **New function** in `scan.rs`:
  ```rust
  pub fn vertex_id(size: usize, masks: &[u64], offsets: &[RowOffsets],
                   axis: usize, x: usize, y: usize, z: usize) -> u32
  ```
  It is `#[inline]`. Callers only ever pass cut edges.

### 4.2 Slots, quadrants and lookup tables

Slot layout (`ORDER`, verified against `EDGE_VERTEX_INDICES`):

| Axis | Slots | Where each slot sits in the cell |
|---|---|---|
| X | 0 1 2 3 | 0 (y0,z0), 1 (y0,z1), 2 (y1,z0), 3 (y1,z1) |
| Y | 4 5 6 7 | 4 (x0,z0), 5 (x1,z0), 6 (x0,z1), 7 (x1,z1) |
| Z | 8 9 10 11 | 8 (x0,y0), 9 (x1,y0), 10 (x0,y1), 11 (x1,y1) |

- **Quadrant.** `q = s & 3` says where the edge sits relative to the cell. Bit j set means the edge is on the cell's far side along `OTHER_AXES[axis][j]`:
  ```rust
  const OTHER_AXES: [[usize; 2]; 3] = [[2, 1], [0, 2], [0, 1]]; // X: z,y   Y: x,z   Z: x,y
  ```
- **Cyclic order around an edge** (verified): 0 → 1 → 3 → 2 → 0.
  - It is a Gray code: stepping q → q ^ (1 << j) crosses the cell face through the edge that is perpendicular to `OTHER_AXES[axis][j]`, called the **bit-j face**.
  - The rank of quadrant q in the cycle is `rank(q) = q ^ (q >> 1)`, so q = 0, 1, 3, 2 has rank 0, 1, 2, 3.
  - Handedness differs per axis, and nothing below depends on it.
- **Slot geometry.** Already available: `EDGE_AXIS_ORIGIN[SLOT_EDGE[s]]` gives `(axis, dx, dy, dz)`, the edge's axis and its lower endpoint relative to the cell. Add a const `SLOT_GEOM: [(u8, u8, u8, u8); 12]` derived from it, next to `SLOT_EDGE` (review m5).
- **Slot row.** `SLOT_ROW[s] = 2*dy + dz`, which evaluates to `[0,1,2,3, 0,0,1,1, 0,0,2,2]`. It indexes `bounding_rows` and gives the vertex's row. Derive it from `SLOT_GEOM`.

New const tables in `tables.rs`, built with `while` loops like `EDGE_INTERSECTION`:

```rust
/// [case][slot]: the triangles of `case` that use `slot`'s vertex, in CHAIN
/// order: consecutive entries share a mesh edge through that vertex. The chain
/// starts at the triangle whose free wall vertex lies on the slot's bit-1 face
/// and ends at the one on its bit-0 face. 3 bits per triangle index (0..=4),
/// up to 5 entries, terminator 7.
pub const FAN: [[u16; 12]; 256];        // 6,144 B

/// [case][corner i]: position of triangle i/3 within FAN[case][slot of corner i],
/// as j_fwd | j_rev << 3, where j_rev = len - 1 - j_fwd.
pub const CORNER_FAN: [[u8; 16]; 256];  // 4,096 B
```

- **The fans must be in chain order, not table order (review B1).** In table index order, 72 of the 652 fans with two or more triangles are not chains. The const build asserts, for every (case, slot):
  - consecutive entries share exactly one other vertex besides the slot's;
  - there are exactly two chain ends, one on each face;
  - the start is on the bit-1 face.
- **Exhaustive facts these rely on** (verified for all 256 cases, all 3 × 256 × 16 two-cell face pairs, and all 3 × 2¹⁸ four-cell configurations around an edge):
  1. In every case, each cut slot's vertex sits in **exactly one** chain of 1 to 5 triangles, never two. The chain's two ends are wall edges on the slot's two different faces, one on each.
  2. Every wall segment belongs to exactly one triangle of its cell. Two neighbouring cells always produce the same segments on their shared face, wound in opposite directions. Every ambiguous face (4 cut edges) is resolved the same way, by cutting off the empty corners, and that rule depends only on the face.
  3. The faces around any interior vertex form **one closed disk** of 4 to 20 faces. At the volume boundary it is one open fan. Every interior mesh edge has exactly 2 faces, consistently wound.
- **The "non-manifold" warning in `tables.rs`.** It does not apply to connectivity for this table, which is a closed 2-manifold with an open border only on the volume boundary. It most likely refers to cell topology that doesn't match the trilinear interpolant. The ring assumption is safe. A regression test (section 7, T20–T22) pins this down in case the table is ever edited.
- **What is not safe is geometry.** A sample exactly at the isolevel gives `t = 0` or `t = 1` and zero-area faces (pseudo_random(32): 16,472 of 94k). They are handled by the form of the test in 4.3.

### 4.3 Hard-edge test

Faces are raw cross products **in table corner order**, `a = (p1 - p0) × (p2 - p0)`. Never rotate the corners so that v comes first. Then both endpoints of an edge compute bit-identical vectors for the same face.

Per job: `c = crease_cos(angle)`, `c2 = c * c`, and `c_pos = c >= 0`.

```rust
#[inline]
fn is_hard(a: Vec3, aa: f32, b: Vec3, bb: f32, c2: f32, c_pos: bool) -> bool {
    let d = a.dot(b);
    let ab = aa * bb;                 // product first: commutative, so symmetric (review M1)
    if c_pos { d < 0.0 || d * d < c2 * ab } else { d < 0.0 && d * d > c2 * ab }
}
```

- **Equivalence.** This is equivalent to `â·b̂ < c` for non-degenerate faces, with no sqrt and no normalise.
- **Degenerate faces.** A zero-area face gives `d = 0` and `ab = 0`, so the test is false for either sign of c. Degenerate faces are never hard, and there is no NaN.
- **Symmetry.** `dot` in glam is `x*x' + y*y' + z*z'` in a fixed order, so `a·b == b·a` bitwise. `aa * bb == bb * aa`, and `c2 * ab` is the same at both ends. Rust does not contract to FMA. Therefore v's owner and w's owner decide the edge (v,w) identically. Test T17 checks it. A later `mul_add` would break this. The result would be cosmetic only, since each vertex groups consistently with itself, but it would fail T17.
- `aa = a.dot(a)` is computed once per face visit.

### 4.4 Ring walk and grouping (pass 5)

The owner cell `C` processes each of its owned cut slots `s` (`cut & owned`, exactly as in pass 4). Let:
- `A = s >> 2`, the edge's axis;
- `o = C + SLOT_GEOM[s].(dx,dy,dz)`, the edge's lower endpoint in grid coordinates;
- `(A0, A1) = OTHER_AXES[A]`.

**Quadrant cells.**
- The cell for quadrant q is `Q(q) = o - (q & 1)·ê[A0] - ((q >> 1) & 1)·ê[A1]`.
- It **exists** iff every coordinate is in `0..=size-2`.
- If it exists, it uses the vertex, because the edge is cut, and it is always inside its row's trim, so no trim check is needed.
- Its case is `cell_case(Q)`, reusing C's own case when `Q == C`. The edge's slot in Q is `A*4 + q`.
- **The owner is the largest existing quadrant cell.** Verified against `owned_slots`: C owns min-corner edges, plus far edges only when there is no neighbour. Every other cell using the vertex has coordinates ≤ C's in every axis, so it comes before C in z → y → x order. Pass 6 relies on this (4.6).

**Walk order.**
- Ranks run `r = 0, 1, 2, 3`, i.e. `q = [0, 1, 3, 2][r]`.
- Within rank r, the fan `FAN[case_Q][A*4 + q]` of length `k_r` is walked **forward for even r and reversed for odd r**. This follows from which face each quadrant is entered through: rank 0 enters over the bit-1 face and leaves over the bit-0 face, rank 1 enters over bit 0 and leaves over bit 1, and so on. It depends only on geometry, not on winding. Verified: consecutive fans then share their face segment.
- Face j of rank r (in walk order) has ring position `p = 5r + j`, where `j < k_r`. Positions `5r + k_r .. 5r + 4` are unused.

**Group mask** (a `u32`, 20 bits used). Bit p set means the ring edge *after* face p is a break.
- **Inside a fan** (`j < k_r - 1`): `bit = is_hard(face j, face j+1)`.
- **Leaving rank r** (`p = 5r + k_r - 1`):
  - if rank `(r+1) % 4` exists, `bit = is_hard(last face of r, first face of r+1)`;
  - if it does not, the bit is set **unconditionally**. This is the gap of an open ring.
- **At most one gap bit.** Quadrants present on a boundary edge are 1 or 2 adjacent ones in the cycle: {0,1}, {1,3}, {3,2} or {2,0}, the last across the wrap. A volume-edge vertex with a single quadrant also gets exactly one gap bit.

**Loop state.** O(1): the previous face's `(a, aa)`, rank 0's first face (for the wrap test from rank 3), the mask, and the previous exit position. No face array is needed.

**Shared vertices (M5a).** Along the walk, consecutive faces share one non-v vertex, the one across the edge between them. So a ring of F faces needs F random-access IDs and F position loads, not 2F. The walk carries the previous face's "leaving" vertex. This is an implementation detail; the per-face cross product is still computed from table order.

**Groups and copies.**
```
h      = popcnt(mask)
copies = max(h, 1)        // closed: #hard, min 1;  open: gap + #hard = 1 + #hard
split  = h >= 2
g(p)   = popcnt(mask & ((1 << p) - 1)); if g == h { g = 0 }   // last arc wraps into group 0
```

- **Closed ring with exactly one hard edge.** `copies = 1`, and the vertex stays welded. The crease fades out at its end over one edge. This is inherent to face-pair creases; check it visually on `box_sdf`.
- Verified against a union-find reference on random 7³ fields with hard fractions 0, 0.2, 0.5 and 1: 0 mismatches, 0 orphans.

### 4.5 Copies and buffer layout

```
positions / normals: [ welded: rows 0..R (pass 3, IDs unchanged) | extras: one block per vertex row, rows 0..R ]
indices:             [ one block per cell row, rewritten only by that cell row                               ]
```

- **Copy 0** is the welded slot. **Copy g ≥ 1** is at `welded + extra[vrow] + local + g - 1`, where `vrow = bounding[SLOT_ROW[s]]`, the vertex's own row.
- **`extra: Vec<u32>`** in the scratch, with `size² + 1` entries (review m4). Reset to 0 only on crease runs, never shrunk, 4 B per row.
  - **Pass 5 (count):** `local = extra[vrow]; extra[vrow] += copies - 1`, only for split vertices. `local` is row-local.
  - **Prefix (in place, exclusive):** for `r` in `0..R`: `let c = extra[r]; extra[r] = acc; acc += c;` then `extra[R] = acc` (total extras). A row's count is `extra[r+1] - extra[r]`.
  - Then `grow_output(welded + total)` resizes positions and normals. `resize` keeps the welded prefix and the pool's capacity.
- **Determinism.** Each vertex row gets its extras from exactly one cell row: row (y,z) from cell row (y,z), with rows (size-1, z), (y, size-1) and (size-1, size-1) from the adjacent last cell row. That is the same rule as pass 2's hand-offs. Within a cell row, cells are visited in ascending x and slots in bit order, so `local` is deterministic.
- **Bounds.** A row has at most 63 + 64 + 64 = 191 vertices, each with at most 19 extras: `local <= 3629`, far below 2²⁴ (exact in f32, see 4.6). Add a `const` assert tied to `MAX_SIZE`.
- **No orphans.** Set bits are distinct ring edges, with at least one face between any two of them, so every `g` in `0..copies` is used and copy 0 is always referenced. Verified by the random-field simulation. The old plan's orphan relaxation is dropped.

### 4.6 Ownership, the stash and partitioning

| Data | Written by | Pass | Why it has one owner |
|---|---|---|---|
| `positions[v]`, `normals[v]` (gradient), welded indices | v's owner cell / the triangle's cell | 4 | Unchanged from today. |
| `normals[v]` ← stash, if split | v's owner | 5 | One owner per vertex already (`owned_slots`). |
| `extra[vrow]` count | the one cell row that owns `vrow` | 5 | Same rule as pass 2's hand-offs. |
| `extra[]` → offsets, total | serial prefix | 5b | Single writer. |
| Rewritten corner indices | the triangle's cell | 6 | Index spans are per cell row already. |
| Extras' positions and normals; final `normals[v]` | v's owner | 6 | One owner per vertex; extras sit in v's row's block. |

**The stash.** A split vertex's gradient normal is never used, because pass 6 overwrites it. Pass 5 therefore parks its per-vertex result in that slot:

```rust
const SPLIT_TAG: f32 = 2.0;  // gradient normals are unit, |z| <= 1
normals[v] = vec3(mask as f32, local as f32, SPLIT_TAG);   // both < 2^24: exact
```

- Unsplit vertices keep their gradient normal, and `z <= 1` is the "not split" flag.
- Add `debug_assert!(n.z.abs() <= 1.0 + 1e-5)` on gradient output.

**Write discipline, stated as a trade-off (review M4).** A split vertex's welded normal slot is written three times: the gradient in pass 4, the stash in pass 5, the final value in pass 6. All three writes come from the same owner, in the same span, in pass order. This relaxes "each slot written exactly once" to "each slot written by exactly one owner" for split normals only. The alternative, not writing gradients until the split is known, would re-read 8 corners and repeat the divide for every unsplit vertex. It would also break "crease 180° == None" holding by construction.

**Partitioning.**
- The layout stays partitionable: a z-block of rows owns one contiguous welded span, one contiguous extras span and one contiguous index span.
- **But single-threaded pass 6 relies on "owner last".** Non-owner cells read `normals[id].z` of vertices in other rows before the owner overwrites them. Across z-block threads that would be a race.
- When threading arrives, pick one of these (the code must make the choice easy):
  - **(A) `vinfo: Vec<u32>`**, packed `mask | local << 20` (20 + 12 bits), 4 B per welded vertex, allocated only on crease runs. Pass 6 stays fused. **Recommended then.**
  - **(B) Keep the stash, but split pass 6** into 6a (corners, all threads) and 6b (owner writes), with a barrier between them.
- Passes 5 and 6 already only *read* other rows' positions and masks, which is safe behind the pass-4 barrier.

### 4.7 Pass 6: rewriting corners and writing copies

Pass 6 walks the cell rows in z → y → x order, trimmed as in pass 4, with an index cursor per row. It skips a whole cell row when its 4 bounding rows all have `extra[r+1] == extra[r]`. That skip is sound because any vertex a cell references lies in one of its bounding rows, and a split vertex's row always has extras. Pass 6 is skipped entirely when the total is 0.

Per cell, in this order:

1. **Corners.** For each triangle corner i: `id = indices[cur + i]` (still welded), `n = normals[id]`. If `n.z != SPLIT_TAG`, continue. Otherwise:
   - `s = ORDER[edge]`, `q = s & 3`, `r = q ^ (q >> 1)`;
   - `j = CORNER_FAN[case][i]`, taking the low 3 bits when r is even and the high 3 bits when r is odd;
   - `p = 5r + j`, `mask = n.x as u32`, `local = n.y as u32`, and g as in 4.4;
   - if `g > 0`, then `indices[cur + i] = welded + extra[bounding[SLOT_ROW[s]]] + local + g - 1`.
2. **Owner writes.** For each owned cut slot with `normals[id].z == SPLIT_TAG` (`id` from `vertex_id`):
   - decode mask and local;
   - walk the ring again (4.4), computing each face's raw cross `a` and adding it to `sum[g(p)]` in a stack `[Vec3; 20]`;
   - for each group, keep a fallback: the unit normal of the group's face with the largest `aa`;
   - write `positions[extra_g] = positions[id]` and `normals[extra_g] = sum[g].try_normalize().unwrap_or(fallback[g])` for g = 1..copies;
   - write `normals[id]` for group 0 **last**.

**Why a group's fallback always exists (review M3).** A split group is an arc bounded by at least one hard bit, and a hard edge has two non-degenerate faces, one of which is in the arc. The old idea of falling back to `normals[welded]` (the gradient) is impossible, because the stash has replaced it.

### 4.8 Normals

**Gradient normals (pass 4, flying edges, always).** Helpers go in `mod.rs`, next to `place_vertex`:

```rust
pub(crate) fn edge_t(edge: usize, corners: &[i8; 8], iso: i8, interpolate: Interpolate) -> f32 {
    if !interpolate { return 0.5; }
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    (iso as f32 - corners[a] as f32) / (corners[b] as f32 - corners[a] as f32)
}
pub(crate) fn place_at(cell: Vec3, edge: usize, t: f32) -> Vec3 {
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let (s, e) = (VERTEX_POSITIONS[a], VERTEX_POSITIONS[b]);
    cell + (s + t * (e - s))
}
// place_vertex(..) == place_at(cell, edge, edge_t(..)) bit for bit: (e - s) has
// components in {-1, 0, 1}, and s + 0.5*(e - s) == (s + e)/2 exactly. Keep
// place_vertex as the longhand reference's entry point, and pin this with a test.

fn sample_gradient(buf: &VoxelBuffer, x: usize, y: usize, z: usize) -> Vec3 {
    // Central difference inside (scale 0.5), one-sided on buffer faces (scale 1).
    // 6 i8 loads at strides 1, size, size².
}
pub(crate) fn gradient_normal(buf: &VoxelBuffer, cell: (usize, usize, usize), edge: usize,
                              corners: &[i8; 8], iso: i8, t: f32) -> Vec3 {
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let g = grad_at(a).lerp(grad_at(b), t);          // glam lerp: no FMA
    (-g).try_normalize().unwrap_or_else(|| edge_fallback(edge, corners, iso))
}
pub(crate) fn edge_fallback(edge: usize, corners: &[i8; 8], iso: i8) -> Vec3 {
    // Unit edge direction from its solid end to its empty end.
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let d = VERTEX_POSITIONS[b] - VERTEX_POSITIONS[a];
    if corners[a] > iso { d } else { -d }
}
pub(crate) fn face_cross(a: Vec3, b: Vec3, c: Vec3) -> Vec3 { (b - a).cross(c - a) }
```

- **Sign.** Density rises into the solid, so outward is `-∇f`. Verified: this agrees with the table winding (`winding_faces_away_from_the_solid`). On spheres, boxes and cubes, 0 corners have `dot(face, grad) <= 0`, and the worst angle is 43.6° (sphere64).
- **Fallback.** `try_normalize` rejects only zero or non-finite input. Each corner gradient's components are multiples of 0.5. The lerp can shrink them, but only by a factor of `t` or `1 - t`, where t is a ratio of small integers, so a nonzero result stays far from underflow and no epsilon is needed. The fallback fires only on ties, i.e. symmetric neighbours around an isolevel sample.
- **Call site in `emit`**, inside `while pending != 0`:
  ```rust
  let edge = SLOT_EDGE[slot];
  let t = edge_t(edge, &corners, iso, interpolate);
  positions[id] = place_at(cell, edge, t);
  normals[id] = gradient_normal(buffer, (x, y, z), edge, &corners, iso, t);
  ```
  It has the same owner and moment as the position write.
- **No gradient cache.** On curved fields each sample's gradient is reused about 1.8 times, and 1.1 on boxy ones. A cache would need 2 z-planes (96 KiB at 64³) and would break `split_at_mut`. Revisit only if step 1's bench says more than 30%.
- **Cheaper option, benched only if needed (review m6).** The trilinear gradient of the owner cell's 8 already-loaded corners would need no extra loads and about 12 flops. It is biased and one-sided per cell.

**Split-copy normals.**
- Each copy gets the area-weighted average of its group's faces: `normalize(Σ a)` over the raw crosses.
- Justification against "normalised average of unit normals":
  - it needs no normalise per face;
  - degenerate faces and slivers get (near) zero weight with no branch, which damps sliver noise;
  - it is exact on flat faces, where a box face-side copy gets exactly the axis normal;
  - within one group of faces less than the crease apart, it looks the same.
- The gradient can't be used per copy, because there is one gradient per vertex.

**Mixing gradient normals and face averages.**
- It is not a C0 discontinuity. Normals only jump where copies share a position, i.e. on hard edges.
- The first ring of triangles next to a crease interpolates from a face average to a gradient normal. Measured difference at unsplit vertices: 5–7° mean and 16–22° max on spheres, 0.0° on box faces.
- Eyeball it in the demo; nothing more is needed.

**Marching cubes.** In `emit`, loop per triangle:

```rust
for tri in TRIANGLE_TABLE[case][..TRIANGLE_COUNT[case] as usize * 3].chunks_exact(3) {
    let e = [tri[0] as usize, tri[1] as usize, tri[2] as usize];   // slices have no .map (review m2)
    let p = e.map(|e| place_vertex(cell, e, &corners, iso, interpolate));
    let n = face_cross(p[0], p[1], p[2]).try_normalize()
        .unwrap_or_else(|| edge_fallback(e[0], &corners, iso));
    positions[cursor..cursor + 3].copy_from_slice(&p);
    normals[cursor..cursor + 3].fill(n);
    // indices as today
    cursor += 3;
}
```

A zero-area triangle is invisible, so any finite unit normal works for it.

### 4.9 Pass list

| Pass | Walks | Reads | Writes | When |
|---|---|---|---|---|
| 1–3 scan | grid | samples | masks, rows, offsets | always (unchanged) |
| 4 emit | cell rows, trim | samples, masks, offsets | `positions[0..W]`, `normals[0..W]` (gradient), `indices` | always (+normals) |
| 5 decide | cell rows, trim; owned cut slots; ring of ≤ 4 quadrant cells | masks, offsets, positions (final after 4), `FAN` | `normals[v]` stash (split only), `extra[vrow]` counts | crease `Some` |
| 5b prefix | vertex rows | `extra` | `extra` → offsets, `extra[R]` = total; `grow_output(W + total)` | crease `Some` |
| 6 write | cell rows, trim; skip rows with no extras in their 4 bounding rows | indices (own span), `normals[id]` (z, and x/y if split), `CORNER_FAN`, `FAN`, masks, offsets, positions | own indices; for owned split v: `positions`/`normals` of extras, final `normals[v]` | crease `Some` and total > 0 |

**Why the passes can't fuse.**
- 5 into 4: a ring needs positions owned by cells up to `C + (1,1,1)`, which haven't run yet.
- 5 into 6: the extras need the global prefix and the resize first.
- Pass 5 *could* run lagged `size + 1` cell rows behind pass 4 in the same loop, for cache locality only. That is a later experiment, not part of the plan.

**Per-row hoisting in passes 5 and 6.** Quadrant cells span `[y-1, y] × [z-1, z]`, and the vertex IDs of their edges need sample rows `y-1..=y+1` × `z-1..=z+1`. Load those **9 masks and 9 `RowOffsets` triplets** once per cell row, with out-of-range rows unused.

## 5. Memory and compute budget

### Memory

| Item | Cost | Notes |
|---|---|---|
| `normals` output | +12 B per output vertex, both methods (MC: +36 B per triangle) | Inherent to writing normals. Bevy used to allocate them inside `with_computed_normals` anyway. |
| Extra copies | 24 B per extra copy (position + normal) | Output, inherent. |
| `extra: Vec<u32>` | 4 B per row, `size² + 1` (16 KiB at 64³) | Only touched on crease runs. The pool keeps it. Kept as a separate `Vec` rather than a `RowOffsets` field, because a `u32` there pads to 8 B per row. |
| Per welded vertex scratch | **0 B** (stash). 4 B with threading variant A. | |
| Per triangle scratch | **0 B** | No face buffer. |
| Rodata | `FAN` 6,144 B + `CORNER_FAN` 4,096 B = **10,240 B** | Plus 2 tiny slot tables. |
| Stack | pass 5: O(1), under 100 B; pass 6 owner write: 20 × (12 + 12 + 4) B ≈ 560 B | |

### Compute

These are estimates until benched. Build steps 1 and 4 measure them.

| Pass | Cost | Relative |
|---|---|---|
| 4, gradient | Per welded vertex: 12 i8 loads, 12 int→float, 6 sub, 6 mul, 1 lerp, 1 normalise (sqrt) ≈ 35 flops, against today's 1 divide + ~6 flops | +10–30% of FE total (unmeasured). Partly offset end-to-end by dropping `with_computed_normals` (a cross product per triangle, a scatter-add and a normalise per vertex). |
| 5, decide | Per owned vertex: ≤ 3 extra `cell_case` (from hoisted masks), F ≈ 6 faces (4–20). Per face: 1 popcount ID (~5 ops), 1 position load, 1 cross product + `aa` (~20 flops). Per ring edge: `is_hard` (~8 flops). Roughly **150–250 ops per vertex**, about 3 cross products per triangle overall. | **Likely 1–2× pass 4.** With a crease on, FE may roughly double. This is the dominant new cost, and the user must see the bench before accepting it. |
| 5b | O(size²) adds | noise |
| 6 | In touched rows only: a 12 B load per corner (z test). Per split vertex: F cross products + adds + a normalise per group. | Small: crease vertices are a 1D set on a 2D surface. |

**Optimisation held in reserve for pass 5 (review M5b).** Each triangle's cross product is currently computed once per owner of each of its 3 vertices, so up to 3 times.
- Remedy: a lazy per-cell-row stack cache of face crosses keyed by (quadrant cell, triangle). The quadrant cells of one owner are at `x-1..=x` × `y-1..=y` × `z-1..=z`, which is 7 distinct neighbours plus C.
- The cache would be `[[Vec3; 5]; 8]` plus valid bits, about 640 B, with a two-column window sliding along x.
- It is only worth it if step 4's bench shows pass 5 over about 1× pass 4.
- Rejected: a row-rolling face cache over whole rows, since that is the face buffer again.

**If pass 5 is still too expensive after that,** fall back to corner vs gradient (section 2). That fallback is one cross product and one dot per triangle, with no ring.

## 6. File-by-file changes

**`mod.rs`**
- `ExtractionJob.crease_angle`, and the `ExtractionResult` echo.
- `crease_cos` and `MIN_CREASE`.
- `Surface.normals`, the new invariant doc and `into_mesh`.
- The helpers from 4.8.
- `extract_with` / `drain_extraction_queue` plumbing and the doc example.
- `Method` docs: flying edges gets "gradient normals; optional crease splitting"; marching cubes gets "face normals written directly".

**`scan.rs`**
- `vertex_id` (4.1).
- `low_bits` unchanged.

**`tables.rs`**
- `FAN` and `CORNER_FAN`, with their const-build asserts.
- Tests T20–T22 (section 7).
- A one-line correction to the "non-manifold" comment, stating what was verified.

**`flying_edges.rs`**
- `SLOT_GEOM` and `SLOT_ROW` next to `SLOT_EDGE`, plus `OTHER_AXES`.
- `extract_into(scratch, buffer, isolevel, interpolate, crease_angle)`, and `extract` likewise.
- `emit` gains `normals: &mut [Vec3]` (7 parameters, at clippy's limit) and the gradient write.
- After `emit`: `if let Some(c) = crease_cos(crease_angle) { crease::apply(...) }`.
- Module docs: new "Normals" and "Hard edges" sections. The ownership section gets the 4.6 trade-off.

**New `crease.rs`** (a submodule of `isosurface`, keeping `flying_edges.rs` readable)
- `pass_5`, `prefix`, `pass_6`, `is_hard`, the ring-walk iterator (shared by passes 5 and 6 so they can't disagree), and the stash encode/decode.

**`marching_cubes.rs`**
- `emit` gains `normals`, with a per-triangle loop (4.8).
- The "Hard faces" docs now say normals are written per face.
- Its signature ignores the crease.

**`scratch.rs`**
- `normals: Vec<Vec3>`: `reset_output` resizes it with the vertex count, and `clear_output` clears it.
- `extra: Vec<u32>`:
  - `reset_extra(size)` does clear + resize to `size² + 1`, on crease runs only;
  - `reserve` grows it.
- `grow_output(total)`: resizes positions and normals, never shrinking capacity.
- **Debug fill.** Under `cfg(debug_assertions)`, `reset_output` fills `normals` and `grow_output` fills new slots with `Vec3::NAN`, so any unwritten slot fails the finite-normals tests. Release builds fill with `ZERO`.
- `capacity_bytes` counts `normals` and `extra`.
- `surface()` clones `normals`.
- `resetting_keeps_capacity` covers `normals` and `extra`.

**`test_maps.rs`**
- `box_sdf(size, half_extent)`: a signed-distance box scaled to i8.
  - `SCALE = 8`, `v = round(-d * 8).clamp(-60, 60) + 2`, with isolevel 2.
  - A tie (`v == 2`) is nudged to 3 inside and 1 outside.
  - Inside it is the min of the planes, so face gradients are exactly axis-aligned. Outside it is Euclidean, which gives a clean bevel.
- Use a fractional half extent so no face plane lands on a sample.
- `k = #{x : |x - c| < h}`.
- Tests use (16, 4.3) with k = 8, the bench (32, 9.3) with k = 18, the demo (10, 2.3) with k = 4.

**`debug.rs`**
- `demo_field()` becomes `box_sdf(10, 2.3)`.
- Queue FE `None`, FE `Some(40.0)` and MC.
- Columns keyed by `(method, crease_angle.is_some())`, with a third colour.
- `info!` logs the crease.

**`examples/isosurface_bench.rs`**
- Rows per field: `FlyingEdges/None`, `FlyingEdges/40°` and `MarchingCubes`, each cold and pooled.
- New columns:
  - `copies`: vertices minus the `None` run's vertices;
  - `+KiB`: for FE/40°, copies × 24 B; for every row, the 12 B/vertex normals.
- Add `box_sdf(32, 9.3)`. Keep `sphere(64, 24)`, which exposes false creases.
- Record a baseline before step 1.

**README**
- Update these sections:
  - Using the extractor
  - Smooth surfaces
  - Hard faces
  - The buffer pool
  - Performance
  - Test volumes
  - Tests
  - Project status
- Notes & gotchas:
  - replace the `with_computed_normals` NaN note with the fallback;
  - add the crease-angle / bevel / false-crease numbers (R1);
  - add the degenerate-glue limitation (R3).

## 7. Tests

**"Reference split"** is a brute force in `tests.rs`. For each welded vertex, collect its faces from the index buffer, then union-find over ring edges that are not hard, using the exact `is_hard`. Groups = copies, and each group's normalised raw-cross sum = the copy's normal. It is built first (step 3), and everything else is checked against it.

**Geometry and structure**

| # | Test |
|---|---|
| T1 | `compare` (longhand cross-validation) runs over crease ∈ {None, 30°, 40°, 60°}. Copies duplicate positions, so triangle-for-triangle equality still holds. `triangles_of` checks every vertex is referenced. |
| T2 | `crease_180_is_none_exactly`: `Some(180.0)`, `Some(1000.0)` and `Some(NAN)` (release only; debug asserts) equal `None` bit for bit on every field. |
| T3 | `crease_sizes_output_exactly`: `W + extra[R] == positions.len() == normals.len()`, and every index `< len`. With the debug NaN fill, no NaN survives. |
| T4 | `every_copy_is_referenced`: no orphans, welded slots included. |
| T5 | `indices_change_only_at_split_vertices`: against the `None` run, an index differs only where it pointed at a split vertex, it still points at the same position, and it stays in its own row span. |
| T6 | `place_at_matches_place_vertex`: bit for bit, both `interpolate` values, every field. |

**Normals**

| # | Test |
|---|---|
| T7 | `normals_are_unit_and_finite`: both methods, crease on and off, every field including `terrain` (ties), `pseudo_random` (degenerates), `single_voxel`, and a size-2 field. |
| T8 | `vertex_normals_point_outward`: sphere(24, 8), `n · (p - c) > 0`, both methods, None and 60°. |
| T9 | `gradient_agrees_with_faces`: spheres 16/24/64 and box_sdf, None: `dot(face_unit, normals[v]) > 0` for every non-degenerate corner (measured min 0.725). |
| T10 | `gradient_fallback`: 5³ zeros with (1,2,2) = 5, (3,2,2) = 5, (2,2,2) = 2 (a tie). The two vertices at (2,2,2) get exactly −X and +X. |
| T11 | `boundary_gradient_is_one_sided`: `flat_slab(6)` normals are exactly +Y. |
| T12 | `mc_normals_are_face_normals`: 3 equal normals per triangle, equal to the normalised `face_cross`, or the fallback when degenerate. |
| T13 | `box_faces_have_exact_normals`: box_sdf, None: vertices whose faces are coplanar carry the axis normal within 1e-5. |

**Box creases** (`box_sdf(16, 4.3)` with k = 8, and `box_sdf(32, 9.3)` with k = 18). The counts were measured in the prototype. T14–T16 first run against the reference split (step 3), then against the implementation.

| # | Crease | Copy histogram |
|---|---|---|
| T14 | 30° | {1: 6(k−2)², 2: 24(k−2), 4: 24} |
| T15 | 40° | {1: 6(k−2)², 2: 24(k−1)} |
| T16 | 50°, 60° | no copies |

Plus: coplanar-face vertices are never split, and face-side copies carry the exact axis normal. Box corners get 4 copies below 35.26° and 2 above; there is never a 3.

**Against the reference and between ends**

| # | Test |
|---|---|
| T17 | `split_matches_reference`: spheres 16/24/64, box, terrain, pseudo_random 10/32, at {20°, 30°, 40°, 60°, 89°}, and `interpolate: false` at 40°. Checks per-vertex copy counts, the multiset of (position, normal) per welded vertex, and the index rewrites. |
| T18 | `both_ends_agree`: for every mesh edge, v's ring decision equals w's ring decision (via a test hook that exposes each vertex's mask). Sphere64, box and pseudo_random at 30°, 40° and 89° (review M1). |
| T19 | `copy_rule_closed_vs_open`: `max(1, h)` inside, `1 + h` on the boundary. Includes a size-2 field (every ring is one quadrant plus a gap) and `size = 64` at 0.5°, which exercises the `local` bound. |
| T19b | `degenerate_face_glues_groups`: a tie field where a zero-area face separates two faces that are 90° apart. They end up in one group. This pins the accepted behaviour (R3). |
| T19c | `marching_cubes_ignores_crease`: MC output with `Some(30.0)` equals `None`, bit for bit. |

**IDs and tables**

| # | Test |
|---|---|
| T20 | `popcount_ids_match_stepped_ids`: `vertex_id` equals the stepped `ids[s]` for every cut slot of every in-trim cell, and is a bijection over all cut grid edges. Fields: sphere(MAX_SIZE), pseudo_random, flat_slab, single_voxel, scattered, box_sdf. |
| T21 | `fan_tables_are_chains` (in `tables.rs`): for every (case, slot), `FAN` entries share an edge consecutively, start on the bit-1 face and end on the bit-0 face, and `CORNER_FAN` matches, including `j_rev = len − 1 − j_fwd`. |
| T22 | `vertex_rings_are_closed_disks`: exhaustively over 3 × 2¹⁸ four-cell configurations, each cut centre edge has one component, every ring edge is used twice with consistent orientation, 4 to 20 faces, and walk order (4.4) visits them as a cycle. `#[ignore]` it if it takes more than about 2 s in debug, and run it with `--ignored`. Also checked: `quadrant_order_is_cyclic` (`s & 3` matches `EDGE_VERTEX_INDICES`, and 0 → 1 → 3 → 2 flips one bit per step). |

**Pool and partitioning**

| # | Test |
|---|---|
| T23 | `a_reused_pool_matches_a_fresh_one` and `a_warm_pool_stops_growing` run crease on, None, then on, in large/small/large order. They check `normals.len()` tracks `positions.len()` through `reset_output` and `grow_output`, and include `extra` in `capacity_bytes`. `Surface: PartialEq` now compares normals too. |
| T24 | `crease_writes_stay_in_row_spans`: instrument pass 6 (`cfg(test)` write log) and assert that every write of a z-block's cell rows lands in that block's welded, extras or index span. This documents the stash's cross-row *reads* as the known threading blocker. |

## 8. Bench and demo

- **Bench.** Rows `FlyingEdges/None`, `FlyingEdges/40°` and `MarchingCubes` (cold and pooled) on:
  - sphere(64, 24)
  - sphere(32, 12)
  - pseudo_random(32)
  - flat_slab(32)
  - solid_cube(16)
  - box_sdf(32, 9.3)

  Add `copies` and `+KiB` columns. Record the baseline before step 1, so the README can quote "normals cost X%" and "crease costs Y%".
- **Demo.** `box_sdf(10, 2.3)` in three columns:
  - FE `None`: smooth and bevelled;
  - FE 40°: flat faces, hard bevel borders, corner facets merged into the bevels;
  - MC: all faces flat.

## 9. Build order

Each step builds, passes `cargo test` and `cargo clippy`, and gets a bench run before the next one starts.

0. **Baseline, no behaviour change.**
   - Add `box_sdf`, `scan::vertex_id`, `SLOT_GEOM`/`SLOT_ROW`/`OTHER_AXES`, and tests T20 and T22.
   - Save the bench output, including box_sdf(32).
1. **Normals everywhere.**
   - `Surface.normals`, `into_mesh`, `scratch.normals` (with the debug NaN fill), the helpers from 4.8, pass-4 gradients, and MC face normals.
   - Tests T6–T13, plus updates to the existing `emit` callers and the pool tests.
   - Bench: FE should be about +10–30% and MC small. **Gate:** over 30%, look at 4.8's cheaper options first.
2. **Plumbing.**
   - `crease_angle` on the job and result, `crease_cos`, the flying-edges signatures, and all literal call sites (section 3).
   - The demo's third column, identical to the first for now.
   - T2 and T19c, and T1 extended over creases (trivially true for now).
3. **Reference first.**
   - The brute-force splitter in `tests.rs`.
   - T14–T16 and T19 run against it, to pin the expectations before any production code exists.
4. **Tables.**
   - `FAN` and `CORNER_FAN`, with their const asserts, and T21.
5. **Pass 5 and prefix, counting only.**
   - `crease.rs` with the ring walk, `is_hard`, the stash and `extra`.
   - `grow_output`, but the extras are left unreferenced for now. With the debug fill, extra slots stay NaN until step 6, so T7 is temporarily scoped to welded slots.
   - T18 (both ends agree), and copy counts against the reference.
   - Bench pass 5 alone. **Gate:** if it costs more than about 1× pass 4, try the face-cross cache (section 5) before going on.
6. **Pass 6.**
   - Corner rewrites and owner writes.
   - T3–T5, T17, T19b, T23 and T24, with T7 back to full scope.
   - Bench the FE/40° rows, copies and KiB on sphere64, box32 and pseudo_random32.
7. **Docs.**
   - The demo, bench columns and README (section 6).
   - Module docs for the ownership trade-off and the threading variants.

## 10. Bottlenecks and risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **Crease quality is limited by marching-cubes facets** (verified). Box edges become 45° bevels, corner facets meet them at 35.26°, and spheres show dihedrals up to 50–55°. sphere(64,24): median 6.7°, p90 35.3°, p99 45.5°, max 54.7°. At 30°, sphere64 gets +4092 copies (+37% vertices). No single angle both creases boxes and keeps spheres smooth. The fallback test has the same limit. | Document the φ/2 rule, default to 40°, and let the bench show the cost. The user must see this before implementation. |
| R2 | **Pass 5 cost** is unmeasured, probably 1–2× pass 4. | Gate at build step 5. Face-cross cache next. Corner vs gradient as the last resort. |
| R3 | **Degenerate faces glue groups.** A zero-area face is never hard, so faces 90° apart separated only by a degenerate face merge into one group. This only happens with ties (samples equal to the isolevel), which the README already tells users to nudge. | Accepted and pinned by T19b. |
| R4 | **Slivers** have noisy normals and cause spurious creases at low angles. | Visible in the pseudo_random bench rows. The area weighting damps their effect on copy normals. |
| R5 | **Stash fragility.** It relies on owner-last ordering and on gradient \|z\| ≤ 1. | `debug_assert`, T24, and threading variant A. |
| R6 | **Bit-identical decisions at both ends** break if someone adds `mul_add` or reorders `is_hard`. | T18. |
| R7 | **Gradient cost in pass 4** is paid even with `None`. | Gate at step 1, with the cheaper options in 4.8. |
| R8 | **Normal slot written up to 3 times** (split vertices only, same owner). | Stated trade-off (4.6). |
| R9 | **Crease ends fade** where a closed ring has exactly one hard edge. | Inherent to face pairs. Visual check on box_sdf. |
| R10 | **Const-fn generation of `FAN`** means chain walks in `while` loops, which is fiddly. | Const asserts plus T21 against brute force. |
| R11 | **Chunk seams.** One-sided gradients at buffer faces don't match across chunks, and crease rings are open at chunk borders (copies = 1 + hard there). | Out of scope. Needs a 1-sample border. |

## 11. Open questions

1. **Is the crease limit acceptable?** R1: face-pair creases can't separate box bevels from sphere staircasing. The user should see the sphere64 copy counts at 30° vs 40°, and the bench at step 6, and confirm 40° as the default.
2. **Is pass 5's cost acceptable?** The 1–2× pass-4 estimate is unmeasured. What cost ceiling should trigger the face-cross cache, or the fallback?
3. **Is the "normal slot written up to 3 times" relaxation OK?** The stricter alternative (a gradient write in pass 6 only) costs an extra divide and corner reload per unsplit vertex, and makes 180° == None no longer automatic.
4. **Area-weighted vs unit-average copy normals.** The plan uses area weighting (4.8). Confirm.
5. **`MIN_CREASE = 0.5°`.** Clamping hides the rounding-driven 0° behaviour. The alternative is to allow 0 and document it.
6. **Gradient cost.** It is estimated at +10–30%. Also, Bevy 0.16's `with_computed_normals` cost is unknown, so the end-to-end change is unknown too.
7. **T22's runtime** in debug builds is unmeasured. It was fast in release.
8. **Housekeeping.** A probe copy of the crate was built with `CARGO_TARGET_DIR` set to this project's `target/`. The next project build may rebuild more than usual; `cargo clean -p` fixes it. No source was changed.

## 12. Out of scope / later

- Threading passes 4–6 (variant A `vinfo` or the 6a/6b split, section 4.6).
- The pass-5 face-cross cache, unless step 5's gate triggers it.
- Running pass 5 lagged inside pass 4's loop.
- A per-sample gradient cache, or a trilinear-gradient option.
- A 1-sample chunk border for seamless gradients and closed rings at chunk seams.
- Corner vs gradient as a selectable cheap mode.
- Crease angles for marching cubes (it is all-hard by definition).
