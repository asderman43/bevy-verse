# Generator

*Design notes, 2026-10-14. Nothing here is implemented. Every section is an
open decision: options and trade-offs, not a plan.*

## Summary

The Generator produces procedural world data: given the seed, a chunk position
and a LOD, it fills that chunk's samples. It holds no world state and never
writes to storage itself; it returns data, and Chunks stores it. No generator
exists in the code today. The only fields are the hand-made test volumes in
`src/voxel_terrain/isosurface/test_maps.rs`, and `Cargo.toml` has no noise
crate. This doc proposes a contract and lists the open choices: output format,
whether it writes the apron, the occupancy ramp and how it scales with LOD,
noise crates, range analysis to skip uniform chunks, world bounds, running on
tasks, and how to test determinism.

See also: [chunks.md](chunks.md) (storage, apron, LOD data side),
[isosurface-extractor.md](isosurface-extractor.md),
[architecture.md](architecture.md).

---

## 1. Role

| Generator does | Generator does not |
|---|---|
| map (seed, chunk key, LOD) to samples | store anything (Chunks) |
| report when a chunk is uniform, if it can tell cheaply | decide what is loaded or when (Chunks) |
| write the material type of each sample | know about edits (Chunks applies them on top) |
| | touch the ECS or `Assets` |

The water layer is not generated here; the water design is in progress (see
[water-surface.md](water-surface.md)).

### What exists today

- `test_maps.rs` returns `(samples, isolevel, size)` tuples
  (`test_maps.rs:3-11`): a fixed 8³ terrain (`:15-42`), a solid cube
  (`:44-54`), a single voxel, scattered voxels, a flat slab (`:87-97`) and a
  sphere (`:100+`). All are `i8` with small values (0 to 5) around isolevel 2.
  They are test fixtures, not a generator.
- The sphere nudges samples off the isolevel, because a sample equal to the
  isolevel interpolates to `t = 0` and gives zero-area triangles (README,
  "Notes & gotchas"). Any generator has the same problem.
- `Cargo.toml` depends on `bevy`, `avian3d`, `bevy_panorbit_camera` and
  `mimalloc`. No noise or RNG crate.

---

## 2. Contract (proposed)

```rust
pub trait Generator: Send + Sync + 'static {
    /// Fill `out` for chunk `pos` at `lod`. Same inputs, same output, always.
    fn generate(&self, pos: IVec3, lod: u8, out: &mut ChunkOut) -> GenHint;
}
```

Properties:

- **Stateless.** `&self` only. All parameters (seed, noise settings) are fixed
  at construction. This is the contract godot_voxel uses.
- **Deterministic.** A sample depends only on its world position, the seed and
  the LOD. Never on the order chunks are generated in, or on neighbours.
- **`Send + Sync`.** One instance behind an `Arc`, shared by every worker.
- **Writes, does not allocate.** `out` is a buffer Chunks provides, so a pool
  can reuse it. Per-call scratch (noise intermediates) is the generator's
  business, e.g. thread-local.

The message pair from the system map: Chunks sends
`GenerateRequest { pos, lod, version }`; the Generator answers with
`ChunkData` (density + type). `version` is not an input to `generate`. Chunks
uses it to discard results for chunks that changed or unloaded meanwhile (see
chunks.md, section 8).

Options for the shape:

| option | pros | cons |
|---|---|---|
| `Arc<dyn Generator>` | swap generators at runtime (debug worlds, tests) | one virtual call per chunk, which is nothing |
| generic `G: Generator` on the plugin | inlined | one generator type per app build |
| plain `fn(seed, pos, lod, &mut out)` | simplest | parameters must go through globals |

Recommended: `Arc<dyn Generator>`, since the per-chunk call cost is negligible
and test worlds become easy.

---

## 3. Output format

| option | what `out` is | notes |
|---|---|---|
| (a) the chunk's storage type (SoA density + type) | Chunks' own buffers | no copy; couples the Generator to the storage type |
| (b) a `VoxelBuffer` plus a type array | today's IE input | density goes straight to the IE if no apron gather is needed; `i8` only (`buffer.rs:13-16`) |
| (c) `ChunkData` enum: `Uniform { density, ty }` or `Dense { … }` | either | lets range analysis (section 7) skip filling 32³ samples; maps onto per-channel uniform storage (chunks.md, 4.2) |

Recommended: (c), because a skipped chunk then never touches dense memory.

The sample type follows CH-3: `i8` density as today, or `u8` occupancy with a
configurable max and isolevel. Section 5 assumes occupancy where it matters.

---

## 4. Writing the apron or not

The IE needs `N + 3` samples per axis once the seam fix is in (chunks.md,
section 5).

| option | the Generator writes | trade-off |
|---|---|---|
| owned samples only, `N³` | `0..N` | Chunks gathers the apron from neighbours; every sample is generated once |
| padded block, `(N+3)³` | `-1..=N+1` | no neighbour wait for unedited chunks; generates about 31% more samples at `N = 32` (35³ / 32³ = 1.31); edited neighbours still force a gather, because the Generator does not see edits |

Because the Generator is a pure function of world position, both give
identical values at shared positions. The difference is only cost and who
waits for whom.

---

## 5. Occupancy ramp and LOD

### 5.1 Writing a ramp

Noise gives a value, not a distance. For occupancy, the generator turns a
signed value `d` (positive inside the solid, in LOD0 voxels) into a value in
`0..=MAX` with an explicit ramp of width `w` voxels. One choice whose
solid/empty test depends only on the sign of `d`:

```
occ = clamp(ISO + 1 + floor(d · MAX / w), 0, MAX)      solid ⇔ occ > ISO ⇔ d ≥ 0
```

Because classification depends only on `sign(d)`, changing `w` (or scaling it
per LOD) moves vertices along their edges but never changes which cells are
cut. Other formulas are possible; this property is what the tests in section
10 can check.

The IE interpolates to `ISO + 0.5` (decided 2026-10-19), so a sample exactly on
`ISO` is fine: it is empty, and its vertices land half a unit away from it.

`d` from noise is not a true distance. For a height field, `d = height(x, z) − y`
is close to a distance on gentle slopes and too large on steep ones, so the
effective ramp is narrower on cliffs. Options: accept it; divide by an
estimated gradient magnitude (needs noise derivatives; check which crates
provide them); or clamp the slope.

### 5.2 Normals with a narrow ramp

With a ramp of about one voxel, central-difference gradients saturate, so
normals snap toward the axes (blocky shading).

| option | cost |
|---|---|
| accept it | none; fits a choppy, low-resolution look |
| wider ramp (`w` of 2–4 voxels) | smoother normals; less dynamic range per voxel at low `MAX` |
| 3×3×3 gradient stencil | IE change; apron of 2, so 37 samples per axis at 32 cells |
| normals from mesh faces | IE or Render change; flat-ish look |

### 5.3 Scaling with LOD

A chunk at LOD `L` has samples `2^L` LOD0 voxels apart. A ramp sized in LOD0
voxels (`w = 1`) then covers less than one coarse sample, so the coarse field
goes binary and coarse surfaces become blocky. Scaling the ramp by the step
keeps the ramp about one sample wide at every LOD:

```
w_L = w · 2^L
```

With the formula in 5.1, this does not change which cells are cut.

**Generate coarse directly, or downsample?** For unedited chunks, the Generator
can sample the field at step `2^L` directly. That costs the same per chunk at
every LOD and needs no LOD0 data. Downsampling (the 1-2-1 tent in chunks.md,
11.2) is only needed where edits exist, which the Generator never sees.

---

## 6. Noise crates

| crate | notes |
|---|---|
| `noise` (noise-rs) | widely used, composable; slowest of these |
| `fastnoise-lite` | pure Rust port, one file's worth of API, easy to use |
| `fastnoise2` | bindings to the C++ FastNoise2: SIMD, node graphs; needs a C++ build |
| `simdnoise` | SIMD, pure Rust |

The SIMD crates are reported 5–10× faster than noise-rs. Maintenance status and
current versions are not checked here (see Open questions). For whole chunks,
range analysis (section 7) saves more than the choice of crate.

---

## 7. Range analysis

Most chunks in a terrain world are all air or all rock. If the Generator can
prove that, it returns `Uniform` (section 3, option c) and fills nothing.

| method | how | precision |
|---|---|---|
| height-field bounds | compute `height(x, z)` for the chunk's `N²` columns (or a coarse grid plus a slope bound); if the chunk's lowest `y` is above `max + ramp`, it is air; if its highest `y` is below `min − ramp`, it is solid | exact for pure height fields; costs `N²` instead of `N³` |
| amplitude bounds | the sum of octave amplitudes bounds 3D noise; combine with the height term | conservative; cheap |
| interval arithmetic over the noise graph | evaluate each node on an interval (godot_voxel's graph generator does this) | tighter; needs a generator built as a graph |

The skip must be conservative: it may say "dense" for a uniform chunk, never
the reverse. A test can check this by generating the chunk densely and
comparing (section 10).

"Uniform" here means uniform storage (every sample identical), which a
`Uniform` result is by construction. Uniform for meshing is weaker; Chunks
checks that separately (chunks.md, 4.2).

---

## 8. World bounds

| option | behaviour | effect |
|---|---|---|
| unbounded | generate any `IVec3` | streaming radius alone sets memory |
| height limit (e.g. ±256 voxels) | above: uniform air; below: uniform bedrock; no noise evaluated | vertical extent fixed, e.g. 512 / 32 = 16 chunks per column at LOD0; Interaction gets `OutOfBounds` |
| horizontal limits too | a finite map | edges of the world must be shown somehow |

The `±256` figure was mentioned once as an example, not agreed.

---

## 9. Running on tasks

- One generation per task, on `AsyncComputeTaskPool`, which in Bevy 0.16
  defaults to 25% of cores (min 1, max 4) and is shared with meshing. Chunks'
  scheduler decides how many run (chunks.md, section 9).
- The task holds an `Arc<dyn Generator>` and an output buffer, and returns
  `ChunkData`. No ECS access inside.
- Per-thread noise scratch: thread-local, or passed in with the buffer.
- Dropping a Bevy `Task` cancels it only before it starts. A started
  generation runs to the end and its result is discarded by version.

Option: run generation and meshing on separate pools, so a burst of streaming
cannot starve edit remeshes. That needs `TaskPoolOptions` tuning.

---

## 10. Determinism and testing

What "deterministic" has to mean is open (GEN-10):

| level | needed for | risk |
|---|---|---|
| same build, same machine | regenerating unloaded chunks, op-log replay | low |
| across machines and platforms | shared seeds, multiplayer, edit logs moved between machines | float transcendental functions and FMA differ; SIMD and scalar paths of one crate can differ |

Tests (proposed):

- **Repeatability.** Generate the same chunk twice, from fresh instances; the
  outputs are byte-identical.
- **Order independence.** Generate a set of chunks in two different orders and
  from several threads; outputs match.
- **Border agreement.** For two neighbours generated with padded blocks
  (section 4), the overlapping samples are equal.
- **LOD classification.** At points on the LOD-`L` grid, solid/empty at LOD `L`
  equals solid/empty at LOD0 (holds with the 5.1 formula).
- **Range analysis is conservative.** For random chunks, every `Uniform`
  result equals the dense result.
- **No samples on the isolevel**, if the IE keeps interpolating to `ISO`
  (section 5.1).
- **Golden hashes.** A hash per (seed, chunk) for a few chunks, to catch
  accidental changes to the world. Only meaningful at the chosen GEN-10 level.

---

## Decisions

| ID | question | options | recommended | affects |
|---|---|---|---|---|
| GEN-1 | Coarse LOD data for unedited chunks | generate at step `2^L`; downsample LOD0 | generate directly (same cost per chunk, no LOD0 needed) | CH-17, memory |
| GEN-2 | Contract shape | `Arc<dyn Generator>`; generic; plain fn | `Arc<dyn>` (negligible cost, easy test worlds) | plugin API, tests |
| GEN-3 | Output format | storage type; `VoxelBuffer` + types; `ChunkData` enum with `Uniform` | `ChunkData` enum (skips dense fill) | CH-6, range analysis |
| GEN-4 | Write the apron | `N³` only; `(N+3)³` padded | — (follows CH-9) | cost, neighbour waits |
| GEN-5 | Ramp formula and width | sign-preserving floor formula; others; `w` of 1–4 | — | normals, CH-3, IE interpolation target |
| GEN-6 | World bounds | unbounded; height limit; finite map | — | memory, streaming shape, `OutOfBounds` |
| GEN-7 | Noise crate | `noise`; `fastnoise-lite`; `fastnoise2`; `simdnoise` | — | speed, build (C++), determinism |
| GEN-8 | Range analysis | height-field bounds; amplitude bounds; interval arithmetic | — | generation cost |
| GEN-9 | Task pool | shared `AsyncComputeTaskPool`; separate pools | — | latency under streaming |
| GEN-10 | Determinism level | same machine; cross-platform | — | crate choice, persistence, multiplayer |

## Open questions

- What should the terrain look like: height field, caves, overhangs, biomes?
  Nothing specifies it, and it decides most of the above.
- How are material types assigned (by depth, slope, noise)? How many types?
- Where does the seed come from, and is it saved?
- Current versions, licences and maintenance status of the four noise crates.
- Which crates give noise derivatives (for 5.1 gradient correction)?
- Is a generator built as a node graph (for interval range analysis) worth it?
- Does the Generator ever need to read edits (e.g. structures placed after
  edits)? The proposed contract says no.
- The water layer's initial contents (oceans, lakes): the water design is in
  progress ([water-surface.md](water-surface.md)).
