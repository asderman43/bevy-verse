//! Isosurface extraction: turns a buffer of voxel samples into a triangle mesh.
//!
//! # Sign convention
//!
//! A sample of `<= isolevel` is **empty space**; `> isolevel` is **solid**.
//!
//! That is not arbitrary. [`tables::TRIANGLE_TABLE`] winds each triangle so its
//! normal points toward the corners whose case bit is set, and the case bit is
//! set for `<= isolevel`. Feeding in a field with the opposite sense produces a
//! mesh that is uniformly inside-out: invisible under back-face culling, and
//! lit from within. `winding_faces_away_from_the_solid` in `tests.rs` pins this
//! down.
//!
//! # Using it
//!
//! Push an [`ExtractionJob`] onto the [`IsosurfaceQueue`] resource. Each
//! [`Update`], [`drain_extraction_queue`] takes everything on the queue,
//! extracts it, and leaves the finished [`ExtractionResult`]s on
//! [`IsosurfaceResults`] for you to do whatever you like with:
//!
//! ```ignore
//! fn request(mut queue: ResMut<IsosurfaceQueue>) {
//!     queue.push(ExtractionJob {
//!         buffer: VoxelBuffer::new(32),
//!         isolevel: 2,
//!         method: Method::FlyingEdges,
//!         interpolate: true,
//!     });
//! }
//!
//! fn collect(mut results: ResMut<IsosurfaceResults>, mut meshes: ResMut<Assets<Mesh>>) {
//!     for result in results.drain() {
//!         let handle = meshes.add(result.surface.into_mesh());
//!         // ...
//!     }
//! }
//! ```
//!
//! Nothing in here touches `Assets<Mesh>` or spawns entities, so the extractors
//! stay usable (and testable) outside the ECS entirely -- see [`extract`].
//!
//! # Allocation
//!
//! Every working buffer extraction needs lives in an [`ExtractionScratch`],
//! which the plugin keeps as a resource for the life of the app. Nothing in it
//! is ever freed: each run resets the buffers in place and only grows them when
//! a chunk needs more room than any chunk before it, so after the first few
//! extractions the allocator is out of the loop for them. Driving the
//! extractors yourself means holding that pool yourself -- see
//! [`extract_with`].
//!
//! The output is the exception. A [`Surface`] outlives its extraction, so its
//! buffers are allocated fresh at exactly the size the scan counted, written
//! once, uninitialized until then, and moved -- not copied -- into the result.
//! See [`output`].

use bevy::{
    asset::RenderAssetUsages,
    math::vec3,
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};

pub mod buffer;
pub mod debug;
pub mod flying_edges;
pub mod marching_cubes;
mod output;
pub mod scan;
pub mod scratch;
pub mod tables;
pub mod test_maps;

#[cfg(test)]
mod tests;

pub use buffer::VoxelBuffer;
pub use scratch::ExtractionScratch;

use tables::{EDGE_VERTEX_INDICES, VERTEX_POSITIONS};

/// The largest chunk the extractors accept, in samples per axis.
///
/// Flying edges packs one row of corner classifications into a `u64`, so a row
/// cannot be longer than 64 samples. Chunks are nowhere near that big -- 32 is
/// already 32k cells -- and the limit buys the whole of pass 1 in bit
/// operations; see the `flying_edges` module docs.
pub const MAX_SIZE: usize = 64;

/// Which algorithm to extract with.
///
/// Both produce the same triangles, in the same places, for the same input.
/// What differs is whether the vertices along those triangles are shared, and
/// so which normals they can carry -- which decides how the surface is shaded,
/// and how much each one costs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Method {
    /// **Welded, smooth.** One vertex per cut edge, shared by every cell that
    /// touches it, each carrying the density gradient at that point as its
    /// normal, so the surface reads as a continuous skin. For terrain, water,
    /// anything organic.
    ///
    /// Four passes: the shared [`scan`], then an emission that walks the rows
    /// again and resolves each shared edge to the same vertex ID from either
    /// side. The sharing is what costs it -- it is the slower of the two.
    ///
    /// Schroeder, Maynard & Geveci, *Flying Edges: A High-Performance Scalable
    /// Isocontouring Algorithm* (2015).
    #[default]
    FlyingEdges,
    /// **Unwelded, hard-faced.** Three fresh vertices per triangle, all three
    /// carrying the triangle's own face normal, so the surface reads as flat
    /// panels. For volumetric shapes, where that is the look you want anyway.
    ///
    /// Same [`scan`] to size the buffers and trim empty space, then plain
    /// cell-by-cell emission. Not sharing vertices is what makes it the faster
    /// of the two; the cost is a vertex buffer roughly 6x larger.
    ///
    /// Lorensen & Cline, *Marching Cubes: A High Resolution 3D Surface
    /// Construction Algorithm* (1987).
    MarchingCubes,
}

/// Where along a cut edge to put the vertex.
///
/// `true` places it where the field actually crosses the isolevel; `false`
/// puts it at the edge midpoint, which gives the blocky look and skips a
/// divide per vertex.
pub type Interpolate = bool;

/// One unit of work for the extractor.
#[derive(Clone, Debug)]
pub struct ExtractionJob {
    pub buffer: VoxelBuffer,
    pub isolevel: i8,
    pub method: Method,
    pub interpolate: Interpolate,
}

/// Identifies a queued job so you can match a result back to what you asked
/// for. Handed out by [`IsosurfaceQueue::push`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

/// Buffers waiting to be extracted. Push here; the subsystem does the rest.
#[derive(Resource, Default, Debug)]
pub struct IsosurfaceQueue {
    pending: Vec<(JobId, ExtractionJob)>,
    next_id: u64,
}

impl IsosurfaceQueue {
    /// Queues a buffer for extraction on the next run of the subsystem.
    pub fn push(&mut self, job: ExtractionJob) -> JobId {
        let id = JobId(self.next_id);
        self.next_id += 1;
        self.pending.push((id, job));
        id
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Removes and returns everything queued.
    pub fn take(&mut self) -> Vec<(JobId, ExtractionJob)> {
        std::mem::take(&mut self.pending)
    }
}

/// A finished extraction, echoing back how it was produced.
#[derive(Clone, Debug)]
pub struct ExtractionResult {
    pub id: JobId,
    pub method: Method,
    pub interpolate: Interpolate,
    pub surface: Surface,
}

/// Extracted surfaces nobody has collected yet.
#[derive(Resource, Default, Debug)]
pub struct IsosurfaceResults {
    finished: Vec<ExtractionResult>,
}

impl IsosurfaceResults {
    /// Takes every finished surface, leaving the store empty.
    pub fn drain(&mut self) -> std::vec::IntoIter<ExtractionResult> {
        std::mem::take(&mut self.finished).into_iter()
    }

    pub fn len(&self) -> usize {
        self.finished.len()
    }

    pub fn is_empty(&self) -> bool {
        self.finished.is_empty()
    }
}

/// An extracted surface as raw, renderer-agnostic mesh data.
///
/// Every entry in `positions` is referenced by at least one triangle, whichever
/// [`Method`] produced it. Whether entries are *shared* between triangles is
/// the methods' one visible difference: flying edges welds, so `positions` holds
/// one vertex per cut edge; marching cubes does not, so it holds three per
/// triangle and `indices` is the identity.
///
/// `normals` runs parallel to `positions`, and every entry is finite and unit
/// length. Flying edges writes the density gradient, marching cubes the face
/// normal of the vertex's one triangle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
}

impl Surface {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Builds a renderable mesh from the surface as it is.
    ///
    /// The extractors already wrote the normals, so nothing is recomputed
    /// here: the buffers move straight into the mesh. See [`Method`] for what
    /// each one's normals are.
    pub fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Runs a job out of a pooled [`ExtractionScratch`]. Pure: no ECS, no asset
/// server, no globals -- the pool is the only state, and it is the caller's.
pub fn extract_with(scratch: &mut ExtractionScratch, job: &ExtractionJob) -> Surface {
    match job.method {
        Method::FlyingEdges => {
            flying_edges::extract_into(scratch, &job.buffer, job.isolevel, job.interpolate)
        }
        Method::MarchingCubes => {
            marching_cubes::extract_into(scratch, &job.buffer, job.isolevel, job.interpolate)
        }
    }
}

/// Runs a job with a throwaway pool, which is then dropped.
///
/// Fine for a one-off or a test. On any path that extracts more than once --
/// which is every real one -- hold an [`ExtractionScratch`] and call
/// [`extract_with`] instead, or the buffers are reallocated per chunk.
pub fn extract(job: &ExtractionJob) -> Surface {
    extract_with(&mut ExtractionScratch::new(), job)
}

/// Marching-cubes case index for a cell: bit `i` is set when corner `i` is
/// `<= isolevel`, i.e. empty. Cases 0 and 255 are uniform and emit nothing.
pub(crate) fn case_index(corners: &[i8; 8], isolevel: i8) -> usize {
    let mut case = 0;
    for (i, &corner) in corners.iter().enumerate() {
        if corner <= isolevel {
            case |= 1 << i;
        }
    }
    case
}

/// Positions a vertex on one edge of the cell whose lowest corner is `cell`.
///
/// Both methods route through here (or through [`edge_t`] and [`place_at`],
/// which it is made of), so they cannot disagree about where a vertex belongs.
pub(crate) fn place_vertex(
    cell: Vec3,
    edge: usize,
    corners: &[i8; 8],
    isolevel: i8,
    interpolate: Interpolate,
) -> Vec3 {
    place_at(cell, edge, edge_t(edge, corners, isolevel, interpolate))
}

/// How far along `edge` the surface crosses it: 0 at the edge's first corner
/// in [`EDGE_VERTEX_INDICES`], 1 at its second.
///
/// Split out of [`place_vertex`] because the gradient normal is blended along
/// the edge by the same amount, and working it out twice is a second divide.
pub(crate) fn edge_t(edge: usize, corners: &[i8; 8], isolevel: i8, interpolate: Interpolate) -> f32 {
    if !interpolate {
        return 0.5;
    }

    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let (from, to) = (corners[a] as f32, corners[b] as f32);
    // Only ever called for an edge the surface crosses, so the corner values
    // straddle the isolevel and `to - from` cannot be zero.
    (isolevel as f32 - from) / (to - from)
}

/// The point `t` of the way along `edge` of the cell whose lowest corner is
/// `cell`.
///
/// Every component of `end - start` is -1, 0 or 1, so `t * (end - start)` is
/// exact, and this lands on bit for bit the same point the longhand
/// `start + (isolevel - from) * (end - start) / (to - from)` does.
pub(crate) fn place_at(cell: Vec3, edge: usize, t: f32) -> Vec3 {
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let (start, end) = (VERTEX_POSITIONS[a], VERTEX_POSITIONS[b]);
    cell + (start + t * (end - start))
}

/// The smooth normal of the vertex `t` of the way along `edge`, in the cell
/// whose lowest corner is `(x, y, z)`: the density gradient, blended between
/// the edge's two corners.
///
/// Density rises into the solid, so the gradient points inward and the normal
/// is its negation. Where the two corners' gradients cancel -- only possible
/// on a tie, a sample sitting exactly on the isolevel -- it falls back to
/// [`edge_fallback`].
pub(crate) fn gradient_normal(
    buffer: &VoxelBuffer,
    (x, y, z): (usize, usize, usize),
    edge: usize,
    corners: &[i8; 8],
    isolevel: i8,
    t: f32,
) -> Vec3 {
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let at_corner = |i: usize| sample_gradient(buffer, x + (i & 1), y + (i >> 1 & 1), z + (i >> 2 & 1));

    let gradient = at_corner(a).lerp(at_corner(b), t);
    (-gradient)
        .try_normalize()
        .unwrap_or_else(|| edge_fallback(edge, corners, isolevel))
}

/// The density gradient at a sample: central differences inside the buffer,
/// one-sided on its faces, where the sample beyond is not there to read.
///
/// Works on the flat sample index directly -- one index computation, then the
/// six neighbours are a stride away each -- because this runs twice per
/// welded vertex and `VoxelBuffer::get` would redo the full index six times.
fn sample_gradient(buffer: &VoxelBuffer, x: usize, y: usize, z: usize) -> Vec3 {
    let size = buffer.size();
    let samples = buffer.samples();
    let i = buffer.index(x, y, z);

    // Difference along one axis, where `c` is the sample's coordinate on it
    // and `stride` the index distance to the next sample along it. Scaled by
    // the reciprocal of the distance the difference spans: 1/2 across a
    // central difference, 1 across a one-sided one.
    let along = |c: usize, stride: usize| {
        let has_lo = c > 0;
        let has_hi = c + 1 < size;
        let lo = if has_lo { i - stride } else { i };
        let hi = if has_hi { i + stride } else { i };
        let scale = if has_lo && has_hi { 0.5 } else { 1.0 };
        (samples[hi] as f32 - samples[lo] as f32) * scale
    };

    vec3(along(x, 1), along(y, size), along(z, size * size))
}

/// A normal for a vertex with no better one: the direction along its edge from
/// the solid end to the empty end. Axis-aligned, so already unit length, and
/// always out of the solid.
pub(crate) fn edge_fallback(edge: usize, corners: &[i8; 8], isolevel: i8) -> Vec3 {
    let (a, b) = EDGE_VERTEX_INDICES[edge];
    let along = VERTEX_POSITIONS[b] - VERTEX_POSITIONS[a];
    if corners[a] > isolevel {
        along
    } else {
        -along
    }
}

/// The unit normal of a triangle, wound the way the triangle table winds it
/// and the way Bevy's own `compute_normals` reads it. `None` for a zero-area
/// triangle, which has no direction to give.
pub(crate) fn face_normal(a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
    (b - a).cross(c - a).try_normalize()
}

/// The extraction step, so consumers can order their collection system after
/// it and pick up results in the same frame.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExtractSet;

/// Extracts everything on the [`IsosurfaceQueue`] into [`IsosurfaceResults`].
pub fn drain_extraction_queue(
    mut queue: ResMut<IsosurfaceQueue>,
    mut results: ResMut<IsosurfaceResults>,
    mut scratch: ResMut<ExtractionScratch>,
) {
    if queue.is_empty() {
        return;
    }

    for (id, job) in queue.take() {
        let surface = extract_with(&mut scratch, &job);
        results.finished.push(ExtractionResult {
            id,
            method: job.method,
            interpolate: job.interpolate,
            surface,
        });
    }
}

/// The extraction subsystem: a queue in, finished surfaces out.
///
/// Deliberately does not render anything -- add [`debug::IsosurfaceDebugPlugin`]
/// for a demo scene, or write your own collection system ordered
/// `.after(ExtractSet)`.
pub struct IsosurfacePlugin;

impl Plugin for IsosurfacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IsosurfaceQueue>()
            .init_resource::<IsosurfaceResults>()
            // One pool for the whole app: it grows to the largest chunk it has
            // seen and then stops allocating. See [`ExtractionScratch`].
            .init_resource::<ExtractionScratch>()
            .add_systems(Update, drain_extraction_queue.in_set(ExtractSet));
    }
}
