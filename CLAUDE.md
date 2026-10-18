# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Bevy 0.16 voxel-terrain prototype built around isosurface extraction. `README.md` documents the algorithms, benchmarks and test suite in depth. Read its "How it works" and "Notes & gotchas" sections before changing the extractors.

## Commands

```bash
cargo run                                        # demo app: one test volume, both extractors side by side
cargo run --features dev                         # same, with Bevy dynamic linking (faster incremental links; never ship it)
cargo test                                       # all tests
cargo test winding_faces_away_from_the_solid     # one test, by name substring
cargo test --lib voxel_terrain::isosurface::tests # one module's tests
cargo run --release --example isosurface_bench   # timings; release only, since dev builds this crate at opt-level 1
```

`avian3d` tracks a git `main` branch, so a build can break when upstream changes.

## Architecture

Everything that matters lives in `src/voxel_terrain/isosurface/`, plus the water mesher in `src/voxel_terrain/water/` (branch `water-fill-model`; design in `docs/water-surface.md`, demo `cargo run --example water_cell`). The rest of `voxel_terrain/` is stubs: `terrain.rs`, `voxel.rs` and `voxels/grid` are an unwired chunk-storage layer, `VoxelTerrainPlugin::build` is commented out, and `octree.rs` / `picking.rs` are empty.

- **Subsystem shape.** `IsosurfacePlugin` takes `ExtractionJob`s from `IsosurfaceQueue` and leaves `ExtractionResult`s on `IsosurfaceResults` during `ExtractSet`. The extractors never touch `Assets<Mesh>` or the ECS. `extract` / `extract_with` call them directly, which is how the tests and the bench use them.
- **One scan, two emitters.** `scan.rs` holds passes 1–3: X rows are packed into `u64` bitmasks, giving per-row trim bounds, exact vertex and triangle counts, and prefix-summed `RowOffsets`. Both `flying_edges.rs` (welded, gradient normals) and `marching_cubes.rs` (unwelded, face normals) emit into buffers sized exactly by that scan. The two methods must produce identical triangles and differ only in vertex sharing and normals.
- **Ownership invariant.** In flying edges, a cell writes only the edges at its minimum corner: table edges 0/3/8, slots 0/4/8. It writes more only where the neighbour that would own an edge is missing at the volume boundary (`owned_slots`). Each arm of `owned_slots` mirrors a counter in `scan.rs` pass 2, so change the two together.
- **Write-once output.** `output.rs` allocates output buffers uninitialized at the counted size, and `Output::finish` is the one `unsafe` step. Debug builds pre-fill a sentinel and panic if any slot is left unwritten, so a counting or ownership bug shows up as a test panic, not as garbage geometry.
- **Pooling.** `ExtractionScratch` (a resource) holds only the scan buffers, which are reused across runs; the output is always freshly allocated. `MAX_SIZE = 64` samples per axis is a hard limit because of the `u64` row masks.
- **Vertex placement.** Both emitters go through `edge_t` + `place_at` (or `place_vertex`) in `isosurface/mod.rs`. Anything that needs terrain vertex positions should call these rather than reimplement them. `edge_t` (two const tables) places horizontal-edge vertices linearly at `ISO + 0.5`, and vertical-edge vertices by the fill formula `(s_solid + s_empty − 7.5)/S_MAX` from the solid end, which the water mesher shares; `t` is always strictly inside (0, 1). It interpolates from the edge's first corner in `EDGE_VERTEX_INDICES`, so the same physical edge reached through a different table edge isn't bitwise identical.

## Conventions and traps

- **Sign convention:** a sample is solid when `> ISO` and empty when `<= ISO`; a case bit is set for an *empty* corner. The triangle-table winding forces this. Get it backwards and the mesh renders inside-out, which under back-face culling looks like nothing rendering at all.
- Samples are `u8` in `0..=S_MAX` (`VoxelBuffer`). `S_MAX = 15` and `ISO = 7` are constants in `isosurface/mod.rs`, not per-job or config values; keep it that way. Water `w` will get its own buffer, not be interleaved with `s`.
- **Corner index:** `i` = x bit 0, y bit 1, z bit 2 (`VERTEX_POSITIONS`). Trust `EDGE_VERTEX_INDICES` for edge numbering. The **first** ASCII diagram at the top of `tables.rs` is stale and gives wrong edge numbers. In the real table, vertical edges are 1, 3, 5, 7 and edges 8–11 run along z. Tables written for Bourke's layout can't be reused unremapped.
- On ambiguous faces, the triangle table keeps empty corners apart, so solid joins diagonally (see `docs/transvoxel-face-ambiguity.md`). Anything that walks face contours has to use the same rule.
- `isosurface/tests.rs` cross-validates every (method, interpolate) combination against a longhand reference marching cubes. Extend it whenever extraction logic changes.
- Some inline comments are in Hungarian.
- `docs/` holds design notes and plans. Agree a design with the user before implementing a new feature or iteration.
