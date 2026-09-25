# bevy-verse

A Bevy-based voxel terrain prototype centered on **isosurface extraction**,
written in Rust. It turns a 3D scalar field (a grid of signed density values)
into a triangle mesh and renders it with [Bevy](https://bevyengine.org/), using
an orbit camera and an on-screen FPS overlay for inspection.

Two extractors are implemented and interchangeable. They share a counting scan
that sizes every buffer before anything is written, and differ in what they do
with it: **flying edges** welds shared vertices, for smooth surfaces like
terrain and water; **marching cubes** doesn't, for hard-faced volumetrics.
Either can run with or without isosurface interpolation, chosen per request.

> **Status: work in progress / prototype.** The app builds a single hardcoded
> test volume and renders it through both extractors side by side. The
> chunked-terrain layer (`VoxelTerrainPlugin`) is stubbed out, and flying edges
> still runs single-threaded. See [Project status](#project-status) for
> specifics.

---

## Requirements

To run the prebuilt Windows binary:

- **Windows** with a working GPU / graphics backend (wgpu picks DX12 or Vulkan
  automatically).

To build from source (required on macOS/Linux, optional on Windows):

- **Rust** (2021 edition). A recent stable toolchain is fine.
- **Bevy 0.16** system dependencies. On Linux you'll typically need ALSA, udev,
  and the usual graphics/Wayland-or-X11 dev packages; see Bevy's
  [setup guide](https://bevyengine.org/learn/quick-start/getting-started/setup/)
  for your platform.
- A working GPU / graphics backend (Vulkan, Metal, or DX12).

Dependencies pinned in `Cargo.toml`:

| Crate | Version | Purpose |
|---|---|---|
| `bevy` | `0.16.0` (features: `bevy_dev_tools`, `bevy_remote`) | Engine, rendering, FPS overlay, gizmos |
| `bevy_panorbit_camera` | `0.26` | Orbit/pan camera for inspecting the mesh |
| `avian3d` | git (`main`) | Physics (declared; not yet used by the meshing code) |
| `mimalloc` | `0.1` | Global allocator (app and benchmark), see [Performance](#performance) |

Because `avian3d` is pulled from a git branch, builds track upstream and may
occasionally break if that branch changes — pin a specific rev if you want
reproducibility.

---

## Run

### Windows — prebuilt binary

A prebuilt `bevy-verse.exe` ships in the project root, so you don't need to
compile anything. Run it from a terminal opened **in the project root** rather
than double-clicking, so the working directory is correct:

```powershell
.\bevy-verse.exe
```

The working directory matters because Bevy's `AssetServer` resolves assets
relative to where the process starts (it looks for an `assets/` folder), not
relative to the exe. The current build generates its mesh and uses a plain-color
material, so it loads no external assets — but launching from the root keeps
things correct if asset loading is added later.

> If the exe fails on startup with a missing `.dll` (e.g. a `bevy_dyn*` library),
> it was built with Bevy's `dynamic_linking` feature and isn't fully
> self-contained. Either copy the matching `bevy_dyn*.dll` next to the exe, or
> rebuild once without the `dev` feature (`cargo build --release`) for a portable
> static binary.

### Other platforms — build from source

There's no prebuilt binary for macOS or Linux; you'll need to compile it. From
the project root (where `Cargo.toml` lives):

```bash
cargo run
```

The first build is slow — Bevy is large. The dev profile is already tuned for
this: the workspace itself builds at `opt-level = 1` while all dependencies build
at `opt-level = 3`, so iteration stays fast without the meshing math crawling.
Note that a full `target/` directory for a Bevy project is large (tens of GB of
compiled dependency artifacts); that's all build cache, not runtime data.

#### Faster iterative builds (optional)

A `dev` feature enables Bevy's dynamic linking, which significantly cuts
incremental link time during development:

```bash
cargo run --features dev
```

Don't ship release builds with it — a dynamically linked binary isn't portable on
its own.
### What you'll see

On launch the app:
1. Spawns a `PanOrbitCamera` (orbit with the mouse, zoom with the scroll wheel).
2. Queues one hardcoded test volume through **both** extraction methods and
   spawns the two meshes side by side — flying edges in red at the origin,
   marching cubes in blue offset along X. Same silhouette, same triangles; the
   red one is smooth-shaded and the blue one flat, which is the whole difference
   between them.
3. Draws debug gizmos: a grey wireframe bounding box of the grid, RGB axis arrows
   at the origin (X red, Y green, Z blue), and a colored sphere at every grid
   sample — **green** where the sample is solid, **red** where it's empty.
4. Shows a green FPS counter in the corner.

Each extraction logs its triangle and vertex counts at `INFO`, which is the
quickest way to confirm the two methods agree.

---

## Project layout

```
src/
├── main.rs                  App setup: plugins, camera, FPS overlay
├── lib.rs                   Exposes `voxel_terrain`
├── prototype.rs             Spawns the orbit camera + ambient light
└── voxel_terrain/
    ├── mod.rs               VoxelTerrainPlugin
    ├── isosurface/          *** the extraction subsystem ***
    │   ├── mod.rs           Queue, results, job/result types, plugin
    │   ├── buffer.rs        VoxelBuffer: the scalar field
    │   ├── scan.rs         Passes 1-3: counting, shared by both extractors
    │   ├── flying_edges.rs  Pass 4: welded emission (smooth), by ownership
    │   ├── marching_cubes.rs  Unwelded emission (hard faces)
    │   ├── scratch.rs       ExtractionScratch: the buffer pool
    │   ├── tables.rs        Lookup tables (the heavy data)
    │   ├── test_maps.rs     Hardcoded fields for the demo and tests
    │   ├── debug.rs         Demo scene
    │   └── tests.rs         Cross-validation
    ├── terrain.rs           Chunked terrain: HashMap of grids + global↔local coords
    ├── voxel.rs             VoxelMap: flat Vec<i8> chunk storage
    ├── voxels/grid/grid.rs  Generic Grid<T> with neighbor/edge accessors
    └── utils/util.rs        XYZ iterator + bit-index helpers
```

Three plugins are registered in `main.rs`:

- **`IsosurfacePlugin`** — the subsystem. Adds the queue and results resources
  and the extraction system.
- **`IsosurfaceDebugPlugin`** — the demo scene above. Optional; drop it and the
  subsystem does nothing until you queue work yourself.
- **`VoxelTerrainPlugin`** — currently a no-op; its body (inserting a `Terrain`
  resource and the debug plugin) is commented out.

---

## Using the extractor

The subsystem is a queue in, finished surfaces out. Nothing in it touches
`Assets<Mesh>` or spawns entities, so the extractors stay usable — and testable —
outside the ECS entirely.

```rust
fn request(mut queue: ResMut<IsosurfaceQueue>) {
    queue.push(ExtractionJob {
        buffer: VoxelBuffer::new(32),
        isolevel: 2,
        method: Method::FlyingEdges,
        interpolate: true,
    });
}

// Order after `ExtractSet` to pick up results in the same frame.
fn collect(mut results: ResMut<IsosurfaceResults>, mut meshes: ResMut<Assets<Mesh>>) {
    for result in results.drain() {
        let handle = meshes.add(result.surface.into_mesh());
        // ...
    }
}
```

`push` returns a `JobId` that comes back on the `ExtractionResult`, so you can
match a surface to whatever asked for it. The result also echoes the `method` and
`interpolate` it was produced with.

Method and interpolation are chosen **per job**, so you can put both algorithms
in one scene and compare them — which is exactly what the demo does. To skip the
ECS altogether, call `extract(&job)` directly.

---

## How it works

Both methods convert a scalar field into a surface mesh. The field is sampled on
a regular 3D grid; the algorithms walk every **cell** (a cube formed by 8
neighboring samples) and, based on which corners are inside vs. outside the
surface, emit triangles approximating where the surface crosses that cube.

### Conventions used in this codebase

The field is a flat `Vec<i8>` indexed `x + y*size + z*size*size`, so a row of
samples along X is contiguous — which is what flying edges scans.

**A sample of `<= isolevel` is empty space; `> isolevel` is solid.** This is
forced, not a preference: `TRIANGLE_TABLE` winds each triangle so its normal
points toward the corners whose case bit is set, and the case bit is set for
`<= isolevel`. So for normals to point *out of* the solid — which back-face
culling and lighting both assume — the set bits have to be the empty side. Feed
in a field with the opposite sense and the mesh comes out uniformly inside-out:
invisible from outside, lit from within. `winding_faces_away_from_the_solid`
pins this down.

The corner/edge numbering is **not** the one from the original 1987 paper —
`tables.rs` uses a scheme where the cube index `i ∈ [0,7]` decomposes directly
into coordinates:

```
x = (i & 1) >> 0
y = (i & 2) >> 1
z = (i & 4) >> 2
```

with axes laid out as Y up, X right, Z toward the viewer. All the lookup tables
are generated for this scheme, and the triangulation cases favor rotations over
inversions to reduce non-manifold output.

### Counting first: `scan.rs`

The weakness of textbook marching cubes is that it can't know how big its output
is until it has produced it, so it grows buffers as it goes. Flying edges
([Schroeder, Maynard & Geveci
2015](https://www.researchgate.net/publication/282975362_Flying_Edges_A_High-Performance_Scalable_Isocontouring_Algorithm))
fixes that by **counting before it writes**, in three passes that emit nothing.

That counting isn't specific to how flying edges then emits, so it lives in
`scan.rs` and **both extractors use it**. Both get exactly sized buffers and the
same trim; what they do afterwards is the only place they differ.

- **Pass 1** (per X row): pack the row's samples into a single `u64`, bit `x` set
  when sample `x` is `<= isolevel`. A chunk is at most 64 samples per axis, so a
  row always fits in one word — and an edge's 2-bit case is then just
  `(mask >> x) & 0b11`, with no per-edge storage at all. Everything the pass
  needs falls out of two shifts: `mask ^ (mask >> 1)` marks the cut edges (its
  popcount is how many vertices the row's X edges contribute) and
  `mask | (mask >> 1)` marks the edges that can bound geometry, whose first and
  last set bit are the **left/right trim** — the columns later passes clamp to,
  so they skip empty space. Output: one `u64` per row plus per-row
  `RowMetadata`.

- **Pass 2** (per row of cells, from the four X-edge rows bounding it): combine
  four X-edge cases into one cell case, then use `TRIANGLE_COUNT` to accumulate
  the triangles the row produces and `EDGE_INTERSECTION` to accumulate the Y- and
  Z-axis vertices it owns. A row owns the Y/Z edges hanging off its own corner of
  each cell; edges on a cell's far side belong to the next row along and are
  added straight onto it — except at the volume boundary, where there is no next
  row and this row has to account for them.

  The vertex half is skipped entirely (`count_triangles`) for an emitter that
  doesn't weld, since it needs one vertex per index and the triangle count
  already tells it that. That's most of the pass's per-cell work.

- **Pass 3**: prefix-sum the per-row counts into absolute write offsets
  (`RowOffsets`). Within a row the vertices are grouped X, then Y, then Z. This
  also yields the exact final buffer sizes, so the outputs are sized once, to
  precisely the right length, before a single vertex is written.

### Smooth surfaces: `flying_edges.rs`

**Pass 4**, and the reason the scan counts vertices. For each cell within its
trim range, recompute the case and step the 12 per-edge vertex IDs along X using
the `EDGE_INTERSECTION` bitmask, so a shared edge resolves to the same ID from
either side. Write positions and indices straight to their reserved slots via
`TRIANGLE_TABLE` and the `ORDER` remap.

The output is a **welded** indexed mesh with no deduplication step: one vertex
per cut edge, shared by every cell touching it. That's what terrain and water
want.

**Normals are the density gradient.** When the owner cell writes a vertex, it
also writes its normal: the gradient at the edge's two corners (central
differences of the neighbouring samples, one-sided on the buffer's faces),
blended by the same `t` as the position and negated, since density rises into
the solid. It needs only the voxel buffer, so it has the same owner and moment as
the position write. Where the two gradients cancel, which only happens on a
sample sitting exactly on the isolevel, it falls back to the edge's direction
from its solid end to its empty end.

**Ownership: pass 4 writes exactly what pass 2 counted.** Pass 2 is already the
pass that decides which row a vertex belongs to, boundary hand-offs included,
and pass 3 turns those counts into spans — so pass 4 has no freedom left. A cell
writes the three edges meeting at its minimum corner (table edges 0/3/8, slots
0/4/8) and nothing else; everything else it touches belongs to a neighbour that
writes it itself, unless that neighbour is off the end of the volume, which is
what `owned_slots` handles. Each of its arms mirrors a counter in pass 2, case
for case, which is the argument that it's right.

Two things that buys. **Now:** a shared vertex was being computed and stored
once per cell that reached it — identical value, and a divide each time under
`interpolate: true`. Once each instead, worth ~25% on a dense field. **Later:**
it makes the vertex buffer partitionable. Rows are indexed `y + z * size`, so a
contiguous block of `z` is a contiguous block of rows, which pass 3 has already
turned into a contiguous span; hand each thread one `split_at_mut` slice and its
writes stay inside it, no `unsafe`. The only rows a thread reaches outside its
own cell rows are the far-face rows at `y = size-1` and `z = size-1`, and those
have no cell row of their own, so nothing contends for them. The concern there
is invalidation, not copying: two cores writing one cache line bounce it between
their L1s under MESI, hundreds of cycles a time, for writes that were redundant
anyway.

The ownership test has to be cheap or it eats the saving — asking "is this
triangle corner mine?" per corner is an unpredictable branch that costs more
than the divides it avoids (measured: 6% *slower* on dense noise). So the vertex
writes iterate `cut & owned` directly, running exactly as many times as the cell
has owned cut edges, and the triangle loop is left writing nothing but indices.

Unchanged by all this: the index writes still need all twelve `ids` entries,
since triangles reference edges the cell doesn't own, and the index buffer was
never the problem — `index_cursor` starts at the row's own offset and never
leaves its span.

### Hard faces: `marching_cubes.rs`

The classic cell-by-cell emission, on top of the same scan: for each cell in the
trim, look up `TRIANGLE_TABLE[case]` and place a vertex on each listed edge.

The difference is that it **doesn't weld** — every triangle gets three of its
own vertices, all carrying the triangle's face normal, so the surface reads as
flat panels. For volumetric shapes that's the look you want, and skipping the
sharing is what makes it the faster of the two. The costs are a vertex buffer
roughly 6x larger, and no smooth shading available.

Not welding also makes its output partitioned by construction: vertex slot is
index slot, so each row of cells owns one contiguous span of both buffers with
no ownership rule needed. Flying edges gets to the same place, but has to be
explicit about it — see ownership above.

Either way, `Surface` carries positions, normals and indices, and
`Surface::into_mesh` moves them into a Bevy `Mesh` (`TriangleList`,
`MAIN_WORLD | RENDER_WORLD`) as they are. Nothing is recomputed: Bevy's
`with_computed_normals` isn't called.

### The buffer pool (`scratch.rs`)

Extraction runs once per chunk, forever, and every buffer it needs is sized
either from the chunk size or from counts the passes produce — so allocating
them per run means asking the allocator for near-identical memory thousands of
times.

`ExtractionScratch` owns all of it instead: the row bitmasks, `RowMetadata`
and `RowOffsets`.
Nothing in it is ever freed while the pool lives. Each run resets the buffers
with `clear` + `resize`, which keeps the allocation and only grows it when a
chunk needs more room than any chunk before it, so after the first few
extractions the allocator is out of the loop entirely. `IsosurfacePlugin` keeps
one as a resource for the life of the app; driving the extractors yourself means
holding the pool yourself and calling `extract_with`.

The output isn't pooled. A `Surface` outlives its extraction and ends up owned
by a `Mesh`, so its memory has to be new every time anyway. Pooling it only
added a zero-fill before emission and a full copy after. Emission writes
straight into the `Vec`s the `Surface` will own, allocated at exactly the size
the scan counted, and they move into the result without a copy (`output.rs`).
They are left uninitialized until written, because every slot is written
exactly once. `Output::finish` is the one `unsafe` step. Debug builds pre-fill
a sentinel and panic if any slot survives unwritten, so `cargo test` checks the
invariant the release build relies on.

### Performance

```bash
cargo run --release --example isosurface_bench
```

Each field is run four ways: both methods, each with a fresh pool per extraction
("cold", what allocating per chunk costs) and with one reused ("pooled", what
the subsystem does). A separate column times `Surface::into_mesh` on the result,
including dropping the mesh. The benchmark uses the same allocator as the app,
mimalloc. Mean ms per extraction, one machine (i5-13500H, pinned to one
performance core), release build:

| field | FE cold | FE pooled | MC cold | MC pooled |
|---|---|---|---|---|
| sphere 64³ | 2.68 | **2.67** | 1.74 | **1.73** |
| sphere 32³ | 0.37 | **0.37** | 0.28 | **0.28** |
| noise 32³ | 1.71 | **1.68** | 2.30 | **2.25** |
| slab 32³ | 0.20 | **0.20** | 0.14 | **0.14** |
| cube 16³ | 0.071 | **0.070** | 0.062 | **0.062** |

`into_mesh` is 0.001–0.002ms on every field.

Reading it:

- **These times include the normals and the output allocation.** Until the
  extractors wrote normals themselves, Bevy computed them in `into_mesh`, which
  then took 0.28ms (FE) and 0.53ms (MC) on the 64³ sphere, and 1.2ms and 2.7ms
  on dense noise. So compare extraction + `into_mesh`, which is what a mesh
  really costs. Pooled, original code (Bevy's normals, glibc) → own normals on
  glibc → own normals on mimalloc:

  | field | FE | MC |
  |---|---|---|
  | sphere 64³ | 2.49 → 2.73 → 2.67 | 2.09 → 2.13 → 1.73 |
  | sphere 32³ | 0.33 → 0.37 → 0.37 | 0.29 → 0.28 → 0.28 |
  | noise 32³ | 2.24 → 2.16 → 1.68 | 4.41 → 4.27 → 2.25 |
  | slab 32³ | 0.18 → 0.19 → 0.20 | 0.14 → 0.14 → 0.14 |
  | cube 16³ | 0.061 → 0.070 → 0.070 | 0.072 → 0.062 → 0.062 |

- **Marching cubes is now up to twice as fast as the original** on big outputs,
  and never slower. Its face normal is one cross product per triangle, while
  Bevy's `compute_smooth_normals` made about five passes over the mesh: a
  zero-filled buffer, a `Vec<usize>` copy of every index, a scatter-add per
  corner, a normalize per vertex, and a format conversion.
- **Flying edges is faster on dense noise but ~7–10% slower on sparse fields.**
  That's the gradient, about 17–20ns per vertex: two central differences (12
  sample reads, with boundary tests), a blend, and a normalize. The cost per
  vertex is the same from 16³ to 64³, so it's arithmetic, not cache misses. On
  sparse fields it costs more than Bevy's face averaging did. Caching each
  sample's gradient (reused by ~1.8 vertices on curved surfaces) and skipping
  boundary tests for interior cells are the obvious ways to cut it.
- **Why the allocator matters.** The output is new memory for every mesh. glibc
  hands out large blocks as fresh pages from the OS, and the first write to each
  4KB page traps into the kernel, which zeroes it first. Freeing returns the block
  to the OS right away (`munmap`), which is most of what the old `into_mesh`
  column measured, since it includes dropping the mesh. mimalloc keeps freed
  memory and hands it out again, so a new mesh lands on pages that are already
  mapped. Small outputs don't change, because glibc was already reusing memory
  for them.

  This benchmark is the allocator's best case: it frees a mesh and immediately
  allocates one the same size. In the game, meshes stay alive while their chunks
  are visible, so memory is reused as old chunk meshes are dropped. With chunk
  streaming that is constant. While the world is still growing, first-touch
  faults remain.
- **Cold and pooled are now the same.** The pool only holds the scan's buffers
  (masks, row metadata, offsets). The output was always fresh memory, and now
  it's allocated at exactly the counted size and written once, instead of being
  zero-filled in the pool and copied out.
- **Neither method wins everywhere, and the crossover is the vertex count.**
  Marching cubes is ~1.3–1.5x faster on sparse and structured fields, where
  emission is cheap and not welding is pure saving. Flying edges is 1.3x faster
  on dense noise, where the surface reaches nearly every cell and marching cubes
  pays for 282k vertices against its 45k. Real terrain chunks look far more like
  the sphere than like the noise.
- Both are much faster than before the scan was shared. The old welding marching
  cubes took 4.95ms on dense noise, without normals. Flying edges took 3.45ms on
  the 64³ sphere and 1.40ms on dense noise, also without normals, before it
  stopped gathering corners for uniform cells and started writing only the
  vertices it owns.
- **Code shape matters in the marching-cubes loop.** Writing each triangle's
  three corners out one by one is 2.3ms faster on dense noise than collecting
  them in a `[Vec3; 3]` and looping over it. See the comment in `emit`.

So: flying edges for terrain and water, marching cubes for volumetrics — chosen
for how they shade, and roughly a wash on speed at the sizes that matter.

### Lookup tables (`tables.rs`)

The standard 256-case marching cubes data, adapted to this project's numbering:

- `VERTEX_POSITIONS` — the 8 cube-corner offsets (unit cube).
- `EDGE_VERTEX_INDICES` — for each of the 12 edges, the pair of corners it joins.
- `TRIANGLE_TABLE[256]` — per case, the sequence of edges to triangulate,
  `-1`-terminated.
- `TRIANGLE_COUNT[256]` — precomputed triangle count per case (validated in tests
  against `TRIANGLE_TABLE`).
- `EDGE_INTERSECTION[256]` — per-case bitmask of which edges carry a vertex;
  flying edges relies on this for both counting and vertex sharing.
- `ORDER` — remaps edge index to the slot used in pass 4's vertex-ID array.
- `EDGE_AXIS_ORIGIN` — per edge, its axis and lower corner; derived from
  `EDGE_VERTEX_INDICES` at compile time. An edge identity both sharing cells
  agree on. Nothing uses it now that welding is flying edges' job and it does it
  by counting, but it's the key any hash-based welding would need, so it stays
  (and stays tested).

### Helpers (`util.rs`)

- `XYZ` — an iterator yielding every `(x,y,z)` in a `size³` grid in `x`-fastest
  order, so loops stay flat.
- `position_from_index` — decodes a `0..8` corner index into its `(x,y,z)` bits.

---

## Test volumes

`test_maps.rs` ships hardcoded fields used by the demo and the tests. Remember
that `> isolevel` is solid, so *setting* a sample fills it in.

- `terrain()` — an 8³ grid with a carved shape.
- `solid_cube(size)` — a solid cube inset by a one-sample border.
- `single_voxel()` — one solid voxel in a 3³ grid.
- `scattered()` — a handful of solid voxels, including two touching only at a corner.
- `flat_slab(size)` — a horizontal slab. The surface cuts only Y edges, never an
  X edge, which is the shape a trim keyed on cut X edges throws away entirely.
- `sphere(size, radius)` — a solid ball; long runs of uniform interior, which is
  what the trim exists for.
- `pseudo_random(size, seed)` — a deterministic noise field, dense and ugly on
  purpose, to reach cases a tidy field never will.

Change `demo_field()` in `debug.rs` to render a different one.

---

## Tests

```bash
cargo test
```

The load-bearing test is the cross-validation in `isosurface/tests.rs`: it runs
each field through a deliberately naive marching cubes written out longhand in
the test file, then through **every (method, interpolate) combination**, and
requires all of them to agree triangle for triangle. That catches the class of
bug flying edges is prone to — a mis-counted vertex, a trim that clips real
geometry, an ID that drifts by one along a row — which is invisible in a mesh
that merely looks plausible.

Alongside it:

- **Buffer exactness** — every vertex slot the scan allocates must end up
  referenced. Over-counting leaves holes, under-counting runs off the end.
- **Ownership** — summing the cut edges each cell *owns* across the whole field
  must equal the vertex count the scan produced, checked without going through
  emission at all: pass 2 counts, `owned_slots` writes, and the two have to
  agree. One slot too few and some vertex is never written; one too many and two
  cells write the same one. A second test runs emission over NaN-filled buffers
  and requires no NaN to survive — zero is no sentinel, since the origin is a
  legitimate vertex position.
- **Welding exactness** — marching cubes emits every triangle corner separately,
  so the number of *distinct* positions it produces is by definition the number
  of cut edges the surface uses, which is exactly how many vertices flying edges
  should allocate. Weld one too few and the counts diverge; one too many and two
  surfaces get stitched together.
- **Unweldedness** — marching cubes' index buffer must be the identity, or a row
  wrote outside its own span.
- **Scan agreement** — skipping the vertex counting must not change the triangle
  counting it shares a loop with.
- **Winding** — normals must point away from the solid, for both methods.
- **Normals** — one per vertex, unit length and finite on every field, including
  noise full of zero-area triangles and a size-2 field that is all boundary.
  They must point out of the sphere. Flying edges' gradient normals must lean the
  same way as every face that uses them. A slab's normals must be exactly
  vertical, which pins the one-sided differences at the buffer's faces. A
  cancelled gradient must fall back to the edge direction, not NaN. Marching
  cubes' three normals per triangle must equal its face normal.
- **Table consistency** — `TRIANGLE_COUNT` matches `TRIANGLE_TABLE`, every edge a
  case uses is marked in `EDGE_INTERSECTION`, and `EDGE_AXIS_ORIGIN` agrees with
  `EDGE_VERTEX_INDICES`.
- **Degenerate inputs** — empty fields, buffers too small to hold one cell, and
  a chunk at the 64-sample row-bitmask limit (plus one over it, which must be
  rejected loudly rather than silently wrap).
- **Pool reuse** — a warm `ExtractionScratch` must give the same answer as a
  cold one across a large/small/large sequence of chunks (stale rows from a
  bigger chunk are the bug), and must stop reallocating once warmed up.

The coordinate-mapping logic in `terrain.rs` (`pos_div_size`, global↔local chunk
coordinates including negatives) and the generic `Grid<T>` accessors are tested too.

---

## Project status

What's done:
- Two interchangeable extractors over one shared counting scan — welded/smooth
  and unwelded/hard-faced — each with and without interpolation, per job.
- A self-contained subsystem: push buffers onto a queue, collect surfaces.
- A buffer pool that reaches its high-water mark and then stops allocating.
- A full set of lookup tables in this project's coordinate convention.
- Cross-validation tying every method against a longhand reference.
- A benchmark harness (`cargo run --release --example isosurface_bench`).
- Debug visualization (bounding box, axes, per-sample spheres) and FPS overlay.

What's stubbed or in flight:
- **Both extractors run single-threaded**, which gives up flying edges' main
  selling point. Emission is ready for it in both: partition by contiguous
  blocks of `z` and each thread gets one `split_at_mut` slice of each buffer
  that all its writes stay inside — marching cubes by construction, flying edges
  because of the ownership rule above. The remaining blocker is the scan's pass
  2 cross-row counter writes at the volume boundary, which the paper avoids by
  having each row own its counters and resolving boundaries in the prefix-sum
  pass.
- **The trim only skips one side.** It bounds edges with at least one *set*
  corner, so it skips solid interiors but not open air. Tracking two intervals
  per row — first/last non-empty and first/last non-solid — and intersecting them
  would skip both. Correctness doesn't depend on it; speed on mostly-air chunks
  does.
- Emission re-reads all 8 corners of any cell that produces geometry; flying
  edges normally carries the edge cases forward. Uniform cells no longer pay
  this, which is most of them.
- `VoxelTerrainPlugin::build` is commented out — no chunk streaming yet; the app
  meshes one hardcoded volume.
- `terrain.rs` / `voxel.rs` / `Grid<T>` define a chunked-storage layer that
  overlaps `VoxelBuffer` and isn't wired into the renderer. `Terrain` holds
  `Grid<u8>` while everything else is `i8`.
- `octree.rs` and `picking.rs` are empty placeholders.
- `avian3d` is a declared dependency but unused so far.

---

## Notes & gotchas

- **The sign convention is load-bearing.** `<= isolevel` is empty, `> isolevel`
  is solid. Get it backwards and the mesh is inside-out — and because back-face
  culling then hides it, the symptom is "nothing renders" rather than anything
  that points at the cause. See [Conventions](#conventions-used-in-this-codebase).
- **Inside test is `<=`, not `<`.** A sample exactly equal to the isolevel counts
  as empty, and interpolates to `t = 0` — which collapses vertices onto grid
  corners and emits zero-area triangles. That's legal marching cubes output, but
  a degenerate triangle has no face normal. Neither method produces NaN for it:
  marching cubes falls back to the edge direction, and flying edges' gradient
  only needs the samples. Nudge
  such samples off the isolevel if you generate fields analytically;
  `test_maps::sphere` does.
- **The methods produce the same triangles but different vertices.** Only flying
  edges welds, so switching method changes the shading — smooth to flat — and
  the vertex count, by design. The *geometry* must not change: if the triangles
  differ, that's a bug, and the cross-validation test is the place to reproduce
  it.
- Some inline comments are in Hungarian.
- Versions above reflect the committed `Cargo.toml`; the `avian3d` git dependency
  tracks `main`, so it can move under you.
