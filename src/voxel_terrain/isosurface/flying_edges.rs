//! Flying edges.
//!
//! Schroeder, Maynard & Geveci, *Flying Edges: A High-Performance Scalable
//! Isocontouring Algorithm* (2015).
//!
//! Marching cubes cannot know how big its output is until it has produced it,
//! so it grows buffers as it goes and emits duplicate vertices along every
//! shared edge. Flying edges instead makes four passes over the field and
//! counts before it writes. The first three are the scan in [`super::scan`],
//! which both extractors share:
//!
//! 1. Classify every edge running along X, one row at a time, recording each
//!    row's trim bounds and how many vertices its X edges contribute.
//! 2. Walk each row of cells, combining four X-edge rows into cell cases, and
//!    count the Y/Z vertices and triangles each row owns.
//! 3. Prefix-sum those counts into per-row write offsets, which also gives the
//!    exact final buffer sizes.
//!
//! What is left here is pass 4, [`emit`]: writing vertices and indices straight
//! to their final slots.
//!
//! Because every row's offsets are known before anything is written, the output
//! is a **welded** indexed mesh with no deduplication step -- one vertex per cut
//! edge, shared by every cell that touches it, which is what makes the computed
//! normals smooth. The per-row work in passes 1, 2 and 4 has no data
//! dependencies, which is where the paper's scalability comes from.
//!
//! # Ownership
//!
//! The rule pass 4 follows is: **write exactly what pass 2 counted.**
//!
//! Pass 2 is already the pass that decides ownership -- it is where a vertex
//! increments *this* row's counter rather than a neighbour's, hand-offs at the
//! volume boundary included -- and pass 3 turns those counts into each row's
//! span. So pass 4 has no freedom left: a cell writes the vertices its own row
//! was counted for, and nothing else.
//!
//! Concretely, a cell owns the three edges meeting at its minimum corner, one
//! per axis: table edges 0 (X), 3 (Y) and 8 (Z), which
//! [`super::scan::RowScan::cell_case`]'s slot numbering calls 0, 4 and 8.
//! Everything else it touches belongs to a neighbour that writes it itself --
//! its far-X edges are the next cell's near edges, its `y+1` edges belong to
//! cell row `(y+1, z)`, its `z+1` edges to `(y, z+1)` -- unless that neighbour
//! does not exist, which is the boundary case [`owned_slots`] handles.
//!
//! Two things this buys:
//!
//! 1. **Now.** A shared vertex used to be computed and stored once per cell
//!    that reached it, with the same value each time, which under
//!    `interpolate: true` is a divide each time. Ownership makes it once:
//!    strictly less work, single-threaded. On a dense field, where the surface
//!    passes through nearly every cell, that is about a quarter of emission.
//!
//!    It only pays if the ownership test is cheap, though. Asking "is this
//!    triangle corner mine?" per corner is a branch that cannot be predicted --
//!    it costs more than the divides it saves. Hence the shape of the loop
//!    below: the vertex writes iterate `cut & owned` directly, so the loop runs
//!    exactly as many times as the cell has owned cut edges (three at most,
//!    inside the volume), and the triangle loop is left doing nothing but
//!    indices.
//! 2. **Later.** It is what makes the vertex buffer partitionable. Rows are
//!    indexed `y + z * size`, so a contiguous block of `z` is a contiguous
//!    block of rows, which pass 3 has already turned into a contiguous
//!    `[start, end)` of the vertex buffer. Give each thread one `split_at_mut`
//!    slice and its writes stay inside it -- no overlapping `&mut`, no `unsafe`.
//!    The only rows a thread reaches outside its own cell rows are the far-face
//!    rows at `y = size - 1` and `z = size - 1`, and those have no cell row of
//!    their own, so nothing contends for them.
//!
//! The second point is about invalidation, not copying: two cores writing the
//! same cache line bounce it between their L1s under MESI, hundreds of cycles
//! at a time, for writes that were redundant to begin with. Exclusive
//! contiguous spans leave at most one shared line per thread boundary, which is
//! noise, and can be padded away if it ever shows up in a profile.
//!
//! Note what does *not* change. The index writes still need all twelve entries
//! of `ids`, because a triangle references edges the cell does not own, so the
//! ID-stepping arithmetic below is untouched -- only `positions[id] = ...` is
//! gated. And the index buffer was never the problem: `index_cursor` starts at
//! the row's own offset and never leaves its span.

use std::mem::MaybeUninit;

use bevy::{math::vec3, prelude::*};

use super::{
    edge_t, gradient_normal,
    output::Output,
    place_at,
    scan::{bit, bounding_rows, combined_trim, RowOffsets, RowScan},
    scratch::ExtractionScratch,
    tables::{EDGE_INTERSECTION, ORDER, TRIANGLE_COUNT, TRIANGLE_TABLE},
    Interpolate, Surface, VoxelBuffer,
};

/// Extracts `buffer`'s isosurface into `scratch`, and copies out the result.
/// See [`super::Method::FlyingEdges`].
///
/// # Panics
/// If `buffer.size()` exceeds [`super::MAX_SIZE`].
pub fn extract_into(
    scratch: &mut ExtractionScratch,
    buffer: &VoxelBuffer,
    isolevel: i8,
    interpolate: Interpolate,
) -> Surface {
    let scan = RowScan::new(buffer, isolevel);

    if buffer.size() < 2 {
        return Surface::default();
    }

    let counts = scan.count(scratch);
    let mut output = Output::new(counts.vertices, counts.indices);

    let slots = output.slots();
    emit(
        &scan,
        interpolate,
        &scratch.corner_masks,
        &scratch.offsets,
        slots.positions,
        slots.normals,
        slots.indices,
    );

    // SAFETY: `output` is sized to the scan's counts, and emission writes
    // every one of those slots: each vertex by its owner, each index by the
    // cell row whose span it is in. See the module docs on ownership, and
    // `ownership_partitions_the_vertices_exactly` /
    // `emission_leaves_no_slot_unwritten` below.
    unsafe { output.finish() }
}

/// Extracts with a throwaway pool. Convenient for one-off calls and tests;
/// prefer [`extract_into`] on any path that runs more than once.
pub fn extract(buffer: &VoxelBuffer, isolevel: i8, interpolate: Interpolate) -> Surface {
    extract_into(&mut ExtractionScratch::new(), buffer, isolevel, interpolate)
}

/// The twelve edge slots a cell is responsible for writing, as a bitmask.
///
/// A cell owns the three edges meeting at its minimum corner -- slots 0 (X),
/// 4 (Y) and 8 (Z), table edges 0, 3 and 8. The rest belong to the neighbour
/// on that side, which writes them itself, so they are only ours when that
/// neighbour is off the end of the volume:
///
/// - `last_x`: no cell to our right, so its near edges (our far-X Y and Z,
///   slots 5 and 9) are ours.
/// - `last_y`: no cell row above, so the X and Z edges along `y + 1` (slots 2
///   and 10) are ours.
/// - `last_z`: no cell row behind, so the X and Y edges along `z + 1` (slots 1
///   and 6) are ours.
///
/// The flags compose: the slots on two far faces at once (3, 7, 11) need both
/// of the corresponding neighbours to be missing.
///
/// Each arm mirrors, case for case, a counter in
/// [`super::scan::RowScan::pass_2`] -- `last_x` here is its
/// `if x == last_cell { y_vertices += bit(cut, 5); z_vertices += bit(cut, 9) }`,
/// and so on. That correspondence is the argument that this is right: the write
/// rule is the count rule.
/// Inverse of [`ORDER`]: the table edge each vertex-ID slot refers to.
///
/// Emission needs the mapping both ways -- edge to slot to look an ID up, slot
/// to edge to place the vertex -- and deriving this one from the other is what
/// keeps them from drifting apart.
const SLOT_EDGE: [usize; 12] = {
    let mut inverse = [0; 12];
    let mut edge = 0;
    while edge < 12 {
        inverse[ORDER[edge]] = edge;
        edge += 1;
    }
    inverse
};

const fn owned_slots(last_x: bool, last_y: bool, last_z: bool) -> u16 {
    let mut owned = (1 << 0) | (1 << 4) | (1 << 8);

    if last_x {
        owned |= (1 << 5) | (1 << 9);
    }
    if last_y {
        owned |= (1 << 2) | (1 << 10);
    }
    if last_z {
        owned |= (1 << 1) | (1 << 6);
    }
    if last_y && last_z {
        owned |= 1 << 3;
    }
    if last_x && last_z {
        owned |= 1 << 7;
    }
    if last_x && last_y {
        owned |= 1 << 11;
    }

    owned
}

/// Pass 4: write the vertices, their normals and the indices to the slots the
/// scan reserved.
///
/// The buffers must be sized to [`Counts`] from a [`RowScan::count`] over the
/// same field and isolevel; every slot in them is written exactly once. See
/// the module docs on ownership for why "exactly once" rather than "at least
/// once".
///
/// A vertex's normal is the density gradient where it sits, written by the
/// same owner at the same moment as its position. The gradient reads the
/// samples around the edge's two corners straight from the buffer, so it needs
/// nothing from any other cell -- which is why ownership carries over to it
/// unchanged.
fn emit(
    scan: &RowScan,
    interpolate: Interpolate,
    corner_masks: &[u64],
    offsets: &[RowOffsets],
    positions: &mut [MaybeUninit<Vec3>],
    normals: &mut [MaybeUninit<Vec3>],
    indices: &mut [MaybeUninit<u32>],
) {
    let buffer = scan.buffer();
    let size = buffer.size();
    let cells = size - 1;
    let last_cell = cells - 1;

    for z in 0..cells {
        for y in 0..cells {
            let bounding = bounding_rows(size, y, z);
            let (left_trim, right_trim) = combined_trim(offsets, &bounding);

            // Ownership only varies with X inside the row, so resolve the other
            // two axes once here.
            let (last_y, last_z) = (y == last_cell, z == last_cell);
            let owned_inside = owned_slots(false, last_y, last_z);
            let owned_at_end = owned_slots(true, last_y, last_z);

            // Vertex ID of each of the cell's 12 edges, laid out by the bit
            // positions in EDGE_INTERSECTION (see tables.rs). Slots 0..4 are X
            // edges, one per bounding row; 4..8 are Y edges and 8..12 are Z
            // edges, near side then far side along X.
            let mut ids = [
                /*  0 */ offsets[bounding[0]].x,
                /*  1 */ offsets[bounding[1]].x,
                /*  2 */ offsets[bounding[2]].x,
                /*  3 */ offsets[bounding[3]].x,
                /*  4 */ offsets[bounding[0]].y,
                /*  5 */ 0,
                /*  6 */ offsets[bounding[1]].y,
                /*  7 */ 0,
                /*  8 */ offsets[bounding[0]].z,
                /*  9 */ 0,
                /* 10 */ offsets[bounding[2]].z,
                /* 11 */ 0,
            ];

            let mut index_cursor = offsets[bounding[0]].indices as usize;

            for x in left_trim..=right_trim {
                let case = scan.cell_case(corner_masks, x, y, z);
                let cut = EDGE_INTERSECTION[case];

                // Each far-side edge sits one step further along X within its
                // own row than the near-side one, so its ID is the near-side ID
                // plus whether that near edge was cut.
                ids[5] = ids[4] + bit(cut, 4);
                ids[7] = ids[6] + bit(cut, 6);
                ids[9] = ids[8] + bit(cut, 8);
                ids[11] = ids[10] + bit(cut, 10);

                // Vertices: this cell's own cut edges, each written once. Three
                // of them at most inside the volume, and the loop runs exactly
                // as many times as there are bits -- no per-triangle-corner
                // test, which on a dense field is a branch that cannot be
                // predicted.
                let owned = if x == last_cell {
                    owned_at_end
                } else {
                    owned_inside
                };

                let mut pending = cut & owned;
                if pending != 0 {
                    let corners = buffer.cell_corners(x, y, z);
                    let cell = vec3(x as f32, y as f32, z as f32);

                    while pending != 0 {
                        let slot = pending.trailing_zeros() as usize;
                        pending &= pending - 1;

                        let id = ids[slot] as usize;
                        let edge = SLOT_EDGE[slot];
                        let t = edge_t(edge, &corners, scan.isolevel(), interpolate);

                        positions[id].write(place_at(cell, edge, t));
                        normals[id].write(gradient_normal(
                            buffer,
                            (x, y, z),
                            edge,
                            &corners,
                            scan.isolevel(),
                            t,
                        ));
                    }
                }

                // Indices: every corner of every triangle, pointing at whoever
                // owns the vertex.
                if TRIANGLE_COUNT[case] > 0 {
                    for (corner, &edge) in TRIANGLE_TABLE[case].iter().enumerate() {
                        if edge == -1 {
                            break;
                        }
                        indices[index_cursor + corner].write(ids[ORDER[edge as usize]]);
                    }

                    index_cursor += TRIANGLE_COUNT[case] as usize * 3;
                }

                // Step to the next cell along +X. The X-edge IDs advance by
                // whether *this* cell's X edge was cut, so this has to happen
                // after the triangles are written -- advancing at the top of the
                // loop shifts every X vertex by one slot.
                ids[0] += bit(cut, 0);
                ids[1] += bit(cut, 1);
                ids[2] += bit(cut, 2);
                ids[3] += bit(cut, 3);
                // This cell's far side is the next cell's near side.
                ids[4] = ids[5];
                ids[6] = ids[7];
                ids[8] = ids[9];
                ids[10] = ids[11];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel_terrain::isosurface::{scan::Counts, test_maps};

    /// The counts the scan hands emission have to be exactly what it writes:
    /// the buffers are sized from them and every slot is filled by index.
    #[test]
    fn the_scan_sizes_the_output_exactly() {
        let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(16, 5.0));
        let mut scratch = ExtractionScratch::new();

        let counts = RowScan::new(&buffer, isolevel).count(&mut scratch);
        let surface = extract_into(&mut scratch, &buffer, isolevel, true);

        assert_eq!(
            counts,
            Counts {
                vertices: surface.vertex_count(),
                indices: surface.indices.len(),
            },
        );
    }

    /// The ownership invariant, checked head on: summing the cut edges each
    /// cell *owns* over the whole field has to come out at exactly the vertex
    /// count the scan produced.
    ///
    /// Pass 2 counts and [`owned_slots`] writes; this compares the two without
    /// going through emission at all. One owned slot too few and some vertex
    /// slot is never written; one too many and two cells write the same slot,
    /// which is the duplicate work ownership exists to remove.
    #[test]
    fn ownership_partitions_the_vertices_exactly() {
        for field in [
            test_maps::sphere(16, 5.0),
            test_maps::terrain(),
            test_maps::pseudo_random(9, 0xC0FFEE),
            test_maps::flat_slab(6),
            test_maps::solid_cube(5),
        ] {
            let (buffer, isolevel) = test_maps::buffer(field);
            let scan = RowScan::new(&buffer, isolevel);

            let mut scratch = ExtractionScratch::new();
            let counts = scan.count(&mut scratch);

            let size = buffer.size();
            let cells = size - 1;
            let last_cell = cells - 1;
            let mut owned_cuts = 0;

            for z in 0..cells {
                for y in 0..cells {
                    for x in 0..cells {
                        let case = scan.cell_case(&scratch.corner_masks, x, y, z);
                        let cut = EDGE_INTERSECTION[case];
                        let owned = owned_slots(x == last_cell, y == last_cell, z == last_cell);
                        owned_cuts += (cut & owned).count_ones() as usize;
                    }
                }
            }

            assert_eq!(
                owned_cuts, counts.vertices,
                "{size}^3: cells own {owned_cuts} cut edges between them, but \
                 the scan counted {} vertices",
                counts.vertices,
            );
        }
    }

    /// And through emission: every reserved slot must actually get written.
    /// The output is allocated uninitialized, so this is what makes
    /// `Output::finish` sound.
    ///
    /// Debug builds pre-fill the output with a sentinel (NaN, `u32::MAX`) that
    /// nothing downstream can produce, which is what this counts. Zero would be
    /// no sentinel: the origin is a legitimate vertex position.
    #[cfg(debug_assertions)]
    #[test]
    fn emission_leaves_no_slot_unwritten() {
        for field in [
            test_maps::sphere(16, 5.0),
            test_maps::terrain(),
            test_maps::pseudo_random(12, 0xC0FFEE),
            test_maps::flat_slab(6),
        ] {
            let (buffer, isolevel) = test_maps::buffer(field);
            let scan = RowScan::new(&buffer, isolevel);

            let mut scratch = ExtractionScratch::new();
            let counts = scan.count(&mut scratch);

            let mut output = Output::new(counts.vertices, counts.indices);
            let slots = output.slots();
            emit(
                &scan,
                true,
                &scratch.corner_masks,
                &scratch.offsets,
                slots.positions,
                slots.normals,
                slots.indices,
            );

            assert_eq!(output.unwritten(), (0, 0, 0), "(positions, normals, indices) never written");
        }
    }
}
