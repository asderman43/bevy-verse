# Rendering

*Design. Only section 1 describes code that exists. Written 2026-10-14. Every
choice below is an open decision unless it says "fact" and cites the code.*

## Summary

Render turns the IE's `Surface`s into Bevy meshes on chunk entities, gives them
materials, joins chunks of different LOD without cracks, and decides which
chunks are drawn. Today only the first step exists, as a demo: one surface per
method becomes one `Mesh3d` with its own `StandardMaterial`. Everything else
here is a set of options: entity layout, bounds, the CPU copy of meshes, a
per-vertex material ID, mesh compression, LOD meshing (clipmap or octree,
skirts or Transvoxel), LOD handover, culling, and draw-call batching. Bevy 0.16
claims were checked against the 0.16.1 sources in `~/.cargo/registry`.

---

## 1. What exists today (facts)

| fact | where |
|---|---|
| `Surface` holds `positions: Vec<Vec3>`, `normals: Vec<Vec3>`, `indices: Vec<u32>`. Positions are buffer-local: cell `(x, y, z)` starts at `vec3(x, y, z)`, one unit per cell. | `src/voxel_terrain/isosurface/mod.rs:217-221`, `flying_edges.rs:288`, `marching_cubes.rs:133` |
| `Surface::into_mesh` moves the three buffers into a `TriangleList` mesh, `Indices::U32`, with `RenderAssetUsages::MAIN_WORLD \| RENDER_WORLD`, so a CPU copy stays in `Assets<Mesh>`. Nothing is recomputed. | `src/voxel_terrain/isosurface/mod.rs:241-249` |
| `into_mesh` costs 0.001–0.002 ms on every bench field. | README, Performance |
| The demo collects results `.after(ExtractSet)` and spawns `Mesh3d`, `MeshMaterial3d<StandardMaterial>` and a `Transform` per result. It adds a **new** material for each entity. It sets no `Aabb`. | `src/voxel_terrain/isosurface/debug.rs:28`, `:73-80` |
| Sample gizmos (bounds, axes, a sphere per sample) are one retained `GizmoAsset`. | `src/voxel_terrain/isosurface/debug.rs:86-119` |
| The camera is a `PanOrbitCamera` with a `Transform`; `bevy_panorbit_camera` 0.26 requires `Camera3d`, so that is added implicitly. | `src/prototype.rs:13-16` |
| Lighting is one `AmbientLight` (brightness 1000). There is no directional or point light, so there are no shadows today. | `src/prototype.rs:18-21` |
| An FPS overlay is on, green, font size 42. | `src/main.rs:30-46` |
| mimalloc is the global allocator; it halves MC time on dense noise because new meshes land on mapped pages. | `src/main.rs:14-20`, README, Performance |
| `voxel_terrain/render/` and `voxel_terrain/chunk/` are empty directories. | `src/voxel_terrain/` |

Nothing else in this document exists.

---

## 2. Chunk mesh entities

Chunks hands Render a mesh, bounds and visibility as ECS components (proposed).

**Proposed components on a chunk mesh entity:**

| component | content |
|---|---|
| `Mesh3d(Handle<Mesh>)` | the surface |
| `MeshMaterial3d<M>` | one shared handle for all terrain (section 9) |
| `Transform` | translation = chunk origin in world units; scale = `2^L` for LOD `L`, since positions are in cells |
| `Aabb` | set by us on every remesh (section 3) |
| `Visibility` | inherited / visible; **not** used for occlusion culling (section 8) |
| `ChunkMesh { chunk: IVec3, lod: u8, epoch: u32 }` | proposed marker: which chunk and data version produced this mesh |

**Where the mesh lives:**

| option | trade-offs |
|---|---|
| **E1. On the chunk entity** | Fewest entities. LOD handover needs two meshes at once (section 7), which one entity cannot hold. |
| **E2. Child entity of the chunk** | Despawning the chunk despawns its meshes. Old and new meshes can coexist during handover. Transvoxel transition meshes become more children. |
| **E3. Separate entity, map from chunk** | Fully decoupled from Chunks' lifecycle. Needs its own cleanup. |

**Recommended: E2**, because handover and transition meshes both need more than
one mesh per chunk. A water mesh would be another child; the water design is in
progress, see [`water-surface.md`](water-surface.md).

An empty `Surface` (`Surface::is_empty`, `mod.rs:224`) should spawn no mesh.

---

## 3. Bounds (`Aabb`)

**Fact (Bevy 0.16.1):** `calculate_bounds` queries
`(Without<Aabb>, Without<NoFrustumCulling>)`
(`bevy_render-0.16.1/src/view/visibility/mod.rs:369-372`). It inserts an `Aabb`
once and never updates it. A remesh that swaps the `Mesh3d` handle keeps the old
box, so the new geometry can be frustum-culled wrongly.

| option | cost | trade-offs |
|---|---|---|
| **B1. Conservative chunk box** | none | Geometry cannot leave the meshed cell range, so the box `[0, N]³` (shifted by the IE's interior margin, if added) is always valid. Loose for flat terrain, fine for frustum culling, weak for GPU occlusion. |
| **B2. Min/max over positions in the meshing task** | one pass over vertices, off the main thread | Tight. Costs about as much as `into_mesh` itself. |
| **B3. From the scan** | almost none, but an IE change | Row trims give an X range (`scan.rs:44-47`), and non-empty rows give Y/Z. Cell-accurate, not vertex-accurate. |
| **B4. Remove `Aabb` and let Bevy recompute** | one pass on the main thread | No code, but needs the CPU copy (section 4) and runs in `PostUpdate`. |

**Recommended: B1 now, B2 if occlusion culling is adopted**, because B1 is free
and always correct.

---

## 4. CPU copy and memory

**Fact:** `into_mesh` uses `MAIN_WORLD | RENDER_WORLD` (`mod.rs:244`). Bevy's
docs: with `RENDER_WORLD` only, "the asset will be unloaded from the asset
server once it's been extracted and prepared in the render world"
(`bevy_asset-0.16.1/src/render_asset.rs:7-8`).

Today a welded vertex costs 24 B (position + normal, `Float32x3` each) plus
about 24 B of `u32` indices (roughly two triangles per vertex), so about 48 B
per vertex, held twice (CPU and GPU) with the current flag. The brief's estimate
is about 55 KB per chunk mesh; at about 2,000 meshed LOD0 chunks that is about
110 MB on each side.

| option | trade-offs |
|---|---|
| **M1. Keep both (today)** | CPU copy available for colliders (`Collider::trimesh_from_mesh`), picking, and B4. Doubles mesh memory. |
| **M2. `RENDER_WORLD` only** | Halves it. Colliders and bounds must be built from the `Surface` before `into_mesh` moves the buffers. `Assets<Mesh>::get` returns nothing afterwards. |
| **M3. `RENDER_WORLD` only, keep `Surface` for LOD0 near the player** | CPU data only where physics or picking needs it. |

**Recommended: M2 or M3**, because nothing reads the CPU mesh today and the
collider can be built from the `Surface` (see [`interaction.md`](interaction.md),
section 7). `into_mesh` would need a usage parameter; that is an IE API change.

---

## 5. Materials and a per-vertex material ID

The voxel type (stone, dirt, ...) has to reach the shader. Where the ID comes
from (for example the solid end of each vertex's edge) is an IE decision; this
section is about getting it to the GPU.

**Facts (Bevy 0.16.1):**

- `VertexAttributeValues` has no single-byte format. The smallest integer
  formats are `Uint8x2`, `Uint8x4`, `Uint16x2`, and `Uint32`
  (`bevy_mesh-0.16.1/src/vertex.rs:172-199`).
- `Mesh::insert_attribute` panics if the values' format differs from the
  attribute's declared format (`bevy_mesh-0.16.1/src/mesh.rs:228-232`).
  `ATTRIBUTE_COLOR` is declared `Float32x4` (`mesh.rs:178-179`), so reusing it
  costs 16 B per vertex. `ATTRIBUTE_UV_1` is `Float32x2` (`mesh.rs:163-164`).
- `StandardMaterial` multiplies `base_color` by `COLOR` when the mesh has it
  (`bevy_pbr-0.16.1/src/render/mesh.rs:2333-2335` adds it to the layout).
- `MaterialExtension` can override `vertex_shader`, `fragment_shader`,
  `prepass_vertex_shader`, `prepass_fragment_shader`, `deferred_vertex_shader`
  and `specialize` (`bevy_pbr-0.16.1/src/extended_material.rs:39-107`).

| option | format, bytes/vertex | shaders needed | trade-offs |
|---|---|---|---|
| **V1. Custom attribute** `MeshVertexAttribute::new("Vertex_Material", id, Uint8x4)` | 4 B, room for 4 IDs or 1 ID + 3 weights | `ExtendedMaterial<StandardMaterial, _>` with a vertex shader, `specialize` to add the attribute to the layout, and the prepass vertex shader (the prepass and shadow pass use the same layout, or they fail to specialize) | Smallest and clean. Most shader work. |
| **V2. Reuse `COLOR`** | 16 B (`Float32x4`) | fragment shader only, to read the colour as an ID or weights instead of a tint | No custom layout. 4x the size of V1. |
| **V3. Reuse `UV_0` / `UV_1`** | 8 B (`Float32x2`) | fragment shader | No custom layout. IDs as floats, exact up to 2^24. Conflicts with real UVs if triplanar mapping is not used. |
| **V4. Split the mesh by material** | 0 B | none | One sub-mesh per material per chunk. More entities and draws; welded vertices on material borders are duplicated. |

**Interpolation.** An integer ID must be declared `@interpolate(flat)` in WGSL;
the triangle then takes the provoking vertex's ID and borders are hard and
jagged along triangle edges. Smooth transitions need weights: either per-vertex
weights for up to N materials (for example `Unorm8x4` for 4 slots), or the
three corner IDs per triangle plus barycentrics (needs unwelded vertices or a
geometry trick). Texturing is most likely triplanar, since the surface has no
UVs; that is an open question.

**Recommended: V1 with `Uint8x4` (ID + weights)**, because it is the only option
that is both small and allows blending; V2 is the quick prototype path.

---

## 6. Mesh compression

| option | bytes/vertex | needs | trade-offs |
|---|---|---|---|
| **C0. Today** | 24 + indices | nothing | |
| **C1. `Indices::U16`** when a chunk has fewer than 65,536 vertices | saves ~12 B/vertex of indices | a `u32` fallback per chunk | No shader change. A 32³ chunk can exceed it only in noise-like worst cases. |
| **C2. Quantised positions** (`Unorm16x4` or `Uint16x4`, chunk-local) | 8 instead of 12 | custom vertex shader (+ prepass) to decode | Precision 1/2048 of a cell at 32 cells, more than enough. |
| **C3. Packed normals** (octahedral `Snorm16x2`, or `Snorm8x4`) | 4 instead of 12 | custom vertex shader (+ prepass) | 8-bit octahedral shows banding on smooth slopes; 16-bit does not. |

C2 + C3 + C1 bring a welded vertex from about 48 B to about 24 B. C2 and C3 share
the shader work with V1, so they are cheap to add once V1 exists. **Recommended:
C1 first** (no shader), C2/C3 with V1.

---

## 7. LOD meshing

### 7.1 Layout: clipmap or octree

| option | how | trade-offs |
|---|---|---|
| **L1. Clipmap rings** | Concentric shells around the viewer; ring `L` has chunks of `N` cells at step `2^L` | Simple, predictable chunk count per ring, fits a ring buffer per LOD. Wastes detail on empty sky; every ring has the same chunk count. |
| **L2. Octree** | Split nodes by distance (and optionally by content) | Adapts to content. More bookkeeping, irregular neighbour relations, harder streaming. |

Both need the **2:1 rule**: neighbours differ by at most one LOD, so a seam only
ever joins `L` and `L+1`. Chunk counts and memory per ring are in
[`chunks.md`](chunks.md).

### 7.2 Seams: skirts or Transvoxel

A chunk at LOD `L+1` places its border vertices from coarse samples, so its
border does not match the fine neighbour's. The gap must be closed.

| option | trade-offs |
|---|---|
| **S1. Skirts** | Extra triangles along each chunk border, extruded inward or down, that hide the crack. No new tables, no shader, no IE change beyond emitting border strips. Overdraw and visible shading steps at the seam; can poke out on thin features. |
| **S2. Transvoxel (Lengyel)** | Watertight seams. Large cost, listed below. |
| **S3. Overlap / no fix** | Slightly enlarge coarse chunks. Cracks remain where the surface slopes. Only for testing. |

**Transvoxel costs:**

1. Lengyel's transition tables: 512 cases in 56 equivalence classes.
2. A transition pass per chunk face: a 2D pass over 32×32 transition cells.
   Its rows have 65+ fine samples, so they do not fit the scan's `u64` row
   masks (`MAX_SIZE = 64`, `mod.rs:88`); it needs its own scan.
3. Shrunk boundary cells: regular cells next to a transition face are narrowed,
   so each vertex there needs a secondary position and a 6-bit mask of which
   faces it moves for.
4. A custom vertex shader (and its prepass variant) that picks primary or
   secondary position from the neighbour LOD mask.
5. Fine samples on the coarse chunk's transition faces, so the coarse chunk's
   meshing input must include the fine neighbour's border samples.
6. Edits must reach every LOD that is stored, or transition faces disagree
   with the fine side.
7. A seam-watertightness test: mesh a fine chunk beside a coarse chunk with its
   transition mesh, and require no open edge on the seam.

godot_voxel builds all six transition meshes per chunk and toggles them with a
uniform, so a neighbour's LOD change needs no remesh. That trades memory (six
extra small meshes per chunk) for no remesh on LOD changes.

**Face ambiguity (finding).** [`transvoxel-face-ambiguity.md`](transvoxel-face-ambiguity.md)
shows that our `TRIANGLE_TABLE` resolves all 192 ambiguous (case, face) pairs
the same way as Lengyel's regular table: set corners kept apart. Our table can
stay, if the transition case index uses our bit meaning (set = empty) and
every transition triangle's winding is reversed. The transition cells' own
faces (the coarse face and the 2×2 sub-faces of the fine face) are **not yet
verified**.

**Recommended: S1 for a first LOD prototype, S2 if seam artefacts are
visible**, because skirts need no shader or table work and the decision can
then be made on screenshots.

### 7.3 LOD handover

When a region changes LOD, the old mesh stays until **all** its replacements are
ready, then they swap in one frame. Otherwise holes appear for a few frames.

| case | what has to be ready before the swap |
|---|---|
| one coarse → eight fine | all 8 fine meshes (and their seams) |
| eight fine → one coarse | the coarse mesh |
| neighbour LOD change | with S1 nothing; with S2 per-face transitions either rebuilt, or toggled (godot_voxel style) |

Options for the swap moment: hard swap (pop), or a short cross-fade with dither
(two meshes drawn, needs a shader). E2 in section 2 makes both possible.

---

## 8. Culling

| option | what it skips | status in Bevy 0.16 | trade-offs |
|---|---|---|---|
| **K1. Skip uniform chunks** | chunks with no surface: never meshed, no entity | ours to build | Correct test: every row mask is all zeros, or every row mask is all ones, over the inner `(N+1)³` samples that are meshed. "AND of masks == OR of masks" is **wrong**: it passes a chunk of alternating solid and empty rows. Uniform for meshing is weaker than uniform storage. |
| **K2. Frustum culling** | chunks outside the view | automatic, uses `Aabb` | Needs correct bounds (section 3). |
| **K3. Open-air connectivity (Checchi)** | chunks behind solid rock (caves, underground) | ours to build | 15 bits per chunk: which pairs of its 6 faces are connected through air (flood fill over air samples). BFS from the camera's chunk over faces. Fixes needed: merge entry faces per BFS depth, treat ungraphed chunks as open, give all-air chunks every connection. Compute after generation, not after meshing. LOD0 only, since coarse fields close thin tunnels. 6- vs 26-connectivity for air: our table keeps empty corners apart, so 6 may be exact (untested). |
| **K4. GPU occlusion culling** | anything behind drawn geometry | **experimental**: add `OcclusionCulling` to the camera; needs `DepthPrepass`; incompatible with deferred; rare false culls (`bevy_render-0.16.1/src/experimental/occlusion_culling/mod.rs`) | No CPU work. Needs tight `Aabb`s to help. Try after K1–K3. |

**The `Visibility::Hidden` pitfall (fact).** Directional-light shadow visibility
skips entities whose `InheritedVisibility` is false
(`bevy_pbr-0.16.1/src/light/mod.rs:777`). Hiding a K3-culled chunk with
`Visibility::Hidden` therefore also removes it as a shadow caster, and a cave
behind it becomes sunlit. Instead, remove culled chunk entities from the
**camera's** `VisibleEntities` in a system ordered after
`VisibilitySystems::CheckVisibility` (`VisibleEntities::get_mut`,
`bevy_render-0.16.1/src/view/visibility/mod.rs:236`). That leaves shadow views
untouched. Whether this fully stops the draw with GPU preprocessing on must be
confirmed by a test.

**Recommended order: K1 and K2 first, K3 when underground scenes exist, K4 as an
experiment.**

---

## 9. Draw-call batching

**Facts (Bevy 0.16.1):** meshes are packed into shared GPU slabs by vertex
layout (`MeshAllocator`, `bevy_render-0.16.1/src/mesh/allocator.rs:45-63`;
slabs 1 MiB to 512 MiB). With GPU preprocessing and multi-draw indirect,
entities that share a material instance and a vertex layout are drawn in
batches (`bevy_render-0.16.1/src/batching/gpu_preprocessing.rs`).

Consequences:

- **All terrain chunks should share one material handle.** The demo's
  `materials.add` per entity (`debug.rs:75`) would put every chunk in its own
  batch. Per-chunk data (LOD mask for Transvoxel, fade) then goes into a
  storage buffer indexed per instance, or a few material variants.
- **One vertex layout for all chunks.** Mixing `U16` and `U32` indices (C1),
  or chunks with and without a material attribute, splits batches. Measure
  whether the split matters before forcing `U32`.
- **Chunk size trades draws for remesh cost.** Fewer, larger chunks mean fewer
  draws but slower remeshes after an edit (README: noise 32³ 1.68 ms vs sphere
  64³ 2.67 ms for FE). Chunk size is a CH decision.

---

## Decisions

| ID | question | options | recommended | affects |
|---|---|---|---|---|
| RND-1 | Where chunk meshes live | on chunk entity; child entity; separate entity | child entity: handover and transitions need several meshes | Chunks lifecycle, despawn |
| RND-2 | `Aabb` source | chunk box; min/max in task; from scan; Bevy recompute | chunk box now, min/max if occlusion is used | frustum/occlusion accuracy, IE API |
| RND-3 | CPU copy of meshes | keep both; render-world only; keep `Surface` near player | render-world only (+ `Surface` near player) | memory, colliders, `into_mesh` signature |
| RND-4 | Material ID transport | custom attribute; `COLOR`; UVs; split mesh | custom `Uint8x4` attribute | shaders, prepass, vertex size |
| RND-5 | Material borders | flat ID; per-vertex weights; per-triangle IDs | open: depends on the art direction ([art-direction.md](art-direction.md) leans to flat ID) | IE output, shader |
| RND-6 | Mesh compression | none; `U16` indices; quantised positions; packed normals | `U16` indices first | vertex layout, batching, shader |
| RND-7 | LOD layout | clipmap; octree | open (see chunks.md) | Chunks, streaming |
| RND-8 | LOD seams | skirts; Transvoxel; none | skirts first, Transvoxel if artefacts show | IE, shaders, tests |
| RND-9 | LOD swap | hard swap; dithered cross-fade | hard swap first | shader, entity layout |
| RND-10 | Occlusion culling | none; Checchi graph; GPU; both | uniform skip + frustum now; Checchi with caves | Generator/Chunks per-chunk flags |
| RND-11 | Hiding culled chunks | `Visibility::Hidden`; edit camera `VisibleEntities` | edit camera `VisibleEntities`: keeps shadows | shadows |

## Open questions

- What is the art direction: smooth (FE, gradient normals) or choppy (MC or
  saturated occupancy normals)? It decides RND-5 and the normal-quality options
  in the IE doc.
- Is texturing triplanar? If so, are UV attributes free for V3?
- Which lights will the game have? Shadows make RND-11 matter and add shadow
  passes per cascade, which multiplies draws.
- Is the render pipeline forward or deferred? K4 rules out deferred.
- Do Transvoxel's transition cells follow our face rule? The check in
  `transvoxel-face-ambiguity.md` covers regular cells only.
- Does removing entities from the camera's `VisibleEntities` stop the draw when
  GPU preprocessing is active? Needs a test.
- What frame budget does mesh upload get per frame (count or bytes)? Shared
  with the Chunks scheduler.
- Does a water mesh share the terrain's material and layout? The water design is
  in progress; see [`water-surface.md`](water-surface.md).
