# bevy-verse

A Bevy-based voxel terrain prototype centered on a **Marching Cubes** isosurface
extractor written in Rust. The project turns a 3D scalar field (a grid of signed
density values) into a triangle mesh and renders it with [Bevy](https://bevyengine.org/),
using an orbit camera and an on-screen FPS overlay for inspection.

> **Status: work in progress / prototype.** The app currently builds a single
> hardcoded test volume and renders it. The chunked-terrain layer (`VoxelTerrainPlugin`)
> is stubbed out, and two parallel meshing implementations live side by side (a
> straightforward reference path and an optimized multi-pass pipeline). See
> [Project status](#project-status) for specifics.

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
2. Runs the Marching Cubes setup on a small hardcoded test volume and spawns the
   resulting mesh with a red, double-sided material.
3. Draws debug gizmos: a red wireframe bounding box of the grid, RGB axis arrows
   at the origin (X red, Y green, Z blue), and a colored sphere at every grid
   sample — **green** where the sample is above the isolevel, **red** where it's
   at or below it.
4. Shows a green FPS counter in the corner.

The mesh and the debug gizmos also print their intermediate buffers
(vertices/indices and per-cell metadata) to stdout, which is the main way to
inspect the pipeline right now.

---

## Project layout

```
src/
├── main.rs                  App setup: plugins, camera, FPS overlay
├── lib.rs                   Exposes `voxel_terrain`
├── prototype.rs             Spawns the orbit camera
└── voxel_terrain/
    ├── mod.rs               VoxelTerrainPlugin + MarchingCubesPlugin
    ├── march.rs             *** Marching Cubes — the core of this README ***
    ├── terrain.rs           Chunked terrain: HashMap of grids + global↔local coords
    ├── voxel.rs             VoxelMap: flat Vec<i8> chunk storage
    ├── voxels/grid/grid.rs  Generic Grid<T> with neighbor/edge accessors
    └── utils/
        ├── table.rs         Marching Cubes lookup tables (the heavy data)
        └── util.rs          XYZ iterator + bit-index helpers
```

Two plugins are registered in `main.rs`:

- **`MarchingCubesPlugin`** — active. Adds `march::setup` and `march::march_debug`
  as startup systems.
- **`VoxelTerrainPlugin`** — currently a no-op; its body (inserting a `Terrain`
  resource and the debug plugin) is commented out.

---

## How Marching Cubes works here

Marching Cubes converts a scalar field into a surface mesh. The field is sampled
on a regular 3D grid; the algorithm walks every **cell** (a cube formed by 8
neighboring samples) and, based on which corners are inside vs. outside the
surface, emits triangles that approximate where the surface crosses that cube.

### Conventions used in this codebase

The grid is stored as a flat `Vec<i8>` of density values, indexed
`x + y*size + z*size*size`. A sample is considered **inside** the surface when
its value is `<= isolevel`. The corner/edge numbering is **not** the one from the
original 1987 paper — `table.rs` uses a more convenient scheme where the cube
index `i ∈ [0,7]` decomposes directly into coordinates:

```
x = (i & 1) >> 0
y = (i & 2) >> 1
z = (i & 4) >> 2
```

with axes laid out as Y up, X right, Z toward the viewer. The lookup tables
(`VERTEX_POSITIONS`, `EDGE_VERTEX_INDICES`, `TRIANGLE_TABLE`, `TRIANGLE_COUNT`,
`EDGE_INTERSECTION`, `ORDER`) are all generated for this scheme, and the
triangulation cases favor rotations over inversions to reduce non-manifold
output.

### The reference path: `IsosurfaceExtractor::marching_cubes`

This is the simplest implementation and the easiest place to understand the
algorithm. For each cell (iterating `(x,y,z)` over `size-1` via the `XYZ`
iterator):

1. **Sample the 8 corners** of the cube from the grid.
2. **Build an 8-bit cube index** by setting bit `i` whenever
   `corner[i] <= isolevel`.
3. **Look up the triangulation** in `TRIANGLE_TABLE[cube_index]`, a list of edge
   indices terminated by `-1`.
4. **Place a vertex on each listed edge** by linearly interpolating between the
   edge's two corner positions, weighted by how far the isolevel sits between
   their density values (`interpolation()`):

   ```
   v = p0 + (isolevel - d0) * (p1 - p0) / (d1 - d0)
   ```

5. **Push vertices** into a buffer; indices are just `0,1,2,…`. The mesh is built
   as a `TriangleList` with duplicated vertices and **flat** normals
   (`with_computed_flat_normals`), giving a faceted look.

This path is correct and self-contained but produces no vertex sharing — every
triangle gets its own vertices.

### The optimized path: the 4-pass pipeline

`IsosurfaceExtractor::create_mesh` implements a more elaborate, cache-friendlier
extractor that shares vertices across triangles and computes smooth normals
(`with_computed_normals`). It runs four passes; the intermediate `Metadata` and
`EdgeID` structs carry per-row bookkeeping between them.

- **Pass 1 — `pass_1`** (per X-row): walk each row of samples along X and classify
  every X-edge into a 2-bit case (which endpoints are inside). Record, per `(y,z)`
  row, the **left/right trim** (the first and last columns where the surface
  actually crosses, so later passes can skip empty space) and a count of X-axis
  intersections. Output: a per-edge case buffer plus per-row `Metadata`.

- **Pass 2 — `pass_2`** (per cell, the four rows bounding it): combine the four
  X-edge cases of a cell's surrounding rows into a single case value, then use
  `EDGE_INTERSECTION` and `TRIANGLE_COUNT` to accumulate how many Y- and Z-axis
  vertices the cell needs and how many triangles it produces. Boundary columns/
  rows (`x_max`, `max_index`) get extra handling so edges on the far faces aren't
  dropped. This pass is explicitly marked for a rewrite (`TODO: ez ujra kell irni`).

- **Pass 3 — `pass_3`**: prefix-sum the per-row counts into absolute **vertex
  IDs** and **index offsets**. Each row's `EdgeID` gets the running base offset
  for its X/Y/Z vertices, and the global triangle count determines the final
  index-buffer length. The output index/vertex buffers are pre-sized here (via
  `set_len`) so pass 4 can write directly by ID.

- **Pass 4 — `pass_4`**: the actual emission. For each cell within its trim range,
  recompute the combined case, advance the per-edge vertex IDs using the
  `EDGE_INTERSECTION` bitmask (so shared edges resolve to the same vertex ID), and
  write interpolated vertex positions and triangle indices into the pre-allocated
  buffers via `TRIANGLE_TABLE` and the `ORDER` remap.

`final_output` wraps the resulting index/vertex buffers into a Bevy `Mesh`
(`TriangleList`, `MAIN_WORLD | RENDER_WORLD`, computed normals).

> **Note:** In the current `setup`, the spawned mesh actually comes from the
> simple `fe.marching_cubes(gd)` path; the 4-pass pipeline is run just above it
> and its buffers are printed to stdout for inspection rather than rendered. So
> the two paths can be compared directly while the optimized one is still being
> finished.

### Lookup tables (`table.rs`)

The tables encode the standard 256-case Marching Cubes data adapted to this
project's corner/edge numbering:

- `VERTEX_POSITIONS` — the 8 cube-corner offsets (unit cube).
- `EDGE_VERTEX_INDICES` — for each of the 12 edges, the pair of corner indices it
  connects.
- `TRIANGLE_TABLE[256]` — for each cube case, the sequence of edges to triangulate,
  `-1`-terminated.
- `TRIANGLE_COUNT[256]` — precomputed triangle count per case (validated in tests
  against `TRIANGLE_TABLE`).
- `EDGE_INTERSECTION` — per-case bitmask of which edges carry a vertex; the
  optimized pipeline relies on this for vertex sharing.
- `ORDER` — remaps edge index to the slot used in the pass-4 vertex-ID array.

### Helpers (`util.rs`)

- `XYZ` — an iterator yielding every `(x,y,z)` in a `size³` grid in `x`-fastest
  order, so meshing loops stay flat.
- `position_from_index` — decodes a `0..8` corner index into its `(x,y,z)` bits.

---

## Test volumes

`march.rs` ships several hardcoded fields used by `setup` and the unit tests:

- `test_map()` — an 8³ grid with a more complex carved shape (isolevel 2).
- `test_map_2(size)` — a thin vertical feature.
- `test_map_3()` — a single set voxel in a 3³ grid.
- `test_map_4()` — a small 3³ arrangement (currently the one `setup` renders).

Swap which one `test_test_test()` returns to render a different volume.

---

## Tests

The meshing passes have unit tests (run per-pass and end-to-end on the test maps),
plus table-consistency checks (e.g. verifying `TRIANGLE_COUNT` matches
`TRIANGLE_TABLE`, and that every case emits a multiple of 3 edges):

```bash
cargo test
```

The coordinate-mapping logic in `terrain.rs` (`pos_div_size`, global↔local chunk
coordinates including negatives) and the generic `Grid<T>` accessors are tested too.

---

## Project status

What's done:
- A working, correct reference Marching Cubes implementation.
- A full set of lookup tables in this project's coordinate convention.
- A largely complete optimized 4-pass extractor with trim-based empty-space
  skipping and vertex sharing.
- Debug visualization (bounding box, axes, per-sample spheres) and FPS overlay.

What's stubbed or in flight:
- `VoxelTerrainPlugin::build` is commented out — no chunk streaming yet; the app
  meshes one hardcoded volume.
- `pass_2` is flagged for a rewrite and the optimized pipeline isn't the one being
  rendered yet.
- `terrain.rs` / `voxel.rs` / `Grid<T>` define the chunked-storage layer, but it
  isn't wired into the renderer.
- `octree.rs` and `picking.rs` are empty placeholders.
- `avian3d` is a declared dependency but unused so far.

---

## Notes & gotchas

- **`unsafe { Vec::set_len(...) }`** is used in the optimized passes to pre-size
  buffers that are then fully written by ID. This is sound only as long as every
  slot is written before it's read; if you change the counting in passes 1–3, the
  index math in pass 4 must stay in sync or you'll read uninitialized memory.
- **Inside test is `<=`**, not `<`. Samples equal to the isolevel count as inside.
- **Two normal styles**: the reference path uses flat normals (faceted); the
  optimized path computes smooth normals. Expect the shading to differ when you
  switch which one renders.
- Some inline comments are in Hungarian (e.g. `TODO: ez ujra kell irni` — "this
  needs to be rewritten").
- Versions above reflect the committed `Cargo.toml`; the `avian3d` git dependency
  means upstream changes can affect your build.