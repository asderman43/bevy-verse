//! Marching cubes.
//!
//! Lorensen & Cline, *Marching Cubes: A High Resolution 3D Surface
//! Construction Algorithm* (1987).
//!
//! One cell at a time: classify the eight corners, look the case up, emit its
//! triangles. Textbook marching cubes does that in a single pass, which forces
//! it to grow its output buffers as it goes and leaves it a choice between
//! duplicating every shared edge and welding them back together with a hash
//! lookup per triangle corner.
//!
//! This version takes neither cost. It runs the same scan flying edges does
//! ([`super::scan`]) to count the triangles first, so the buffers are sized
//! once and written by index, and it inherits the scan's per-row trim, so it
//! skips empty space instead of walking every cell in the volume. What is left
//! is the classic cell-by-cell emission, and nothing else.
//!
//! # Hard faces
//!
//! The vertices are **not welded**: every triangle gets three of its own, all
//! carrying the triangle's face normal, so the surface reads as flat panels
//! rather than a smooth skin. A welded mesh could not do this -- a shared
//! vertex has one normal for every face it touches. That is the point -- for
//! volumetric shapes, hard faces are what you want, and not welding is what
//! makes this the cheaper of the two extractors. Where a smooth surface is
//! wanted (terrain, water), use [`super::Method::FlyingEdges`], which welds by
//! construction.
//!
//! The consequence is a larger vertex buffer: three per triangle against
//! flying edges' one per cut edge, roughly 6x. Nothing is deduplicated, so a
//! field whose surface is huge relative to its detail costs more to upload.

use std::mem::MaybeUninit;

use bevy::{math::vec3, prelude::*};

use super::{
    edge_fallback, face_normal,
    output::Output,
    place_vertex,
    scan::{bounding_rows, combined_trim, RowOffsets, RowScan},
    scratch::ExtractionScratch,
    tables::{TRIANGLE_COUNT, TRIANGLE_TABLE},
    Interpolate, Surface, VoxelBuffer,
};

/// Extracts `buffer`'s isosurface into `scratch`, and copies out the result.
/// See [`super::Method::MarchingCubes`].
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

    // Unwelded, so there is exactly one vertex per index and the triangle
    // count sizes both buffers. The scan skips its vertex counting entirely
    // for this.
    let count = scan.count_triangles(scratch);
    let mut output = Output::new(count, count);

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

    // SAFETY: `output` holds exactly one slot per triangle corner the scan
    // counted, and emission walks every one of those corners, writing all
    // three buffers at the same cursor. `every_triangle_gets_its_own_vertices`
    // below checks the cursor covers the span.
    unsafe { output.finish() }
}

/// Extracts with a throwaway pool. Convenient for one-off calls and tests;
/// prefer [`extract_into`] on any path that runs more than once.
pub fn extract(buffer: &VoxelBuffer, isolevel: i8, interpolate: Interpolate) -> Surface {
    extract_into(&mut ExtractionScratch::new(), buffer, isolevel, interpolate)
}

/// Emission: three fresh vertices per triangle, straight into the slots the
/// scan reserved, each carrying the triangle's face normal.
///
/// Each row of cells owns the span starting at its [`RowOffsets::indices`], so
/// a row needs nothing from the row before it -- the same property that makes
/// flying edges' emission parallelizable, for the same reason.
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

    for z in 0..cells {
        for y in 0..cells {
            let bounding = bounding_rows(size, y, z);
            let (left_trim, right_trim) = combined_trim(offsets, &bounding);

            // One cursor for both buffers: unwelded means vertex `n` is only
            // ever referenced by index `n`.
            let mut cursor = offsets[bounding[0]].indices as usize;

            for x in left_trim..=right_trim {
                let case = scan.cell_case(corner_masks, x, y, z);

                // Cases 0 and 255 are uniform cells, the overwhelming majority
                // even inside the trim. Reading the case off the row masks
                // means this costs four shifts and no corner loads.
                if TRIANGLE_COUNT[case] == 0 {
                    continue;
                }

                let corners = buffer.cell_corners(x, y, z);
                let cell = vec3(x as f32, y as f32, z as f32);

                let edges = &TRIANGLE_TABLE[case];
                let listed = TRIANGLE_COUNT[case] as usize * 3;

                // Written out corner by corner rather than as a `[Vec3; 3]`
                // mapped and looped over: measured, the array form costs 2.3ms
                // on dense noise that this one does not.
                for first in (0..listed).step_by(3) {
                    let place = |k: usize| {
                        let edge = edges[first + k] as usize;
                        place_vertex(cell, edge, &corners, scan.isolevel(), interpolate)
                    };
                    let (a, b, c) = (place(0), place(1), place(2));

                    // A zero-area triangle -- a sample sitting exactly on the
                    // isolevel -- has no face normal. It is invisible, so any
                    // unit normal will do; the edge's own direction is one.
                    let normal = face_normal(a, b, c).unwrap_or_else(|| {
                        edge_fallback(edges[first] as usize, &corners, scan.isolevel())
                    });

                    positions[cursor].write(a);
                    positions[cursor + 1].write(b);
                    positions[cursor + 2].write(c);
                    normals[cursor].write(normal);
                    normals[cursor + 1].write(normal);
                    normals[cursor + 2].write(normal);
                    indices[cursor].write(cursor as u32);
                    indices[cursor + 1].write(cursor as u32 + 1);
                    indices[cursor + 2].write(cursor as u32 + 2);
                    cursor += 3;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel_terrain::isosurface::test_maps;

    /// Unwelded output is one vertex per index, and the index buffer is the
    /// identity. Anything else means a row wrote outside its own span.
    #[test]
    fn every_triangle_gets_its_own_vertices() {
        let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(16, 5.0));
        let surface = extract(&buffer, isolevel, true);

        assert_eq!(surface.positions.len(), surface.indices.len());
        assert!(
            surface.indices.iter().enumerate().all(|(i, &id)| id as usize == i),
            "the index buffer is not the identity, so vertices are shared",
        );
    }

    /// The trim is inherited from the scan, so an empty row of cells must cost
    /// nothing -- and, more importantly, must not be mistaken for a row
    /// spanning cell 0 and emit stray geometry.
    #[test]
    fn empty_rows_emit_nothing() {
        // A slab fills the lower half, so every row of cells above it is
        // uniformly empty and every row below is uniformly solid.
        let (buffer, isolevel) = test_maps::buffer(test_maps::flat_slab(8));
        let surface = extract(&buffer, isolevel, false);

        // The only surface is the single horizontal sheet at the slab's top.
        let ys: Vec<i32> = surface.positions.iter().map(|v| (v.y * 2.0) as i32).collect();
        assert!(!ys.is_empty(), "the slab produced no surface at all");
        assert!(
            ys.iter().all(|&y| y == ys[0]),
            "geometry escaped the slab's surface plane",
        );
    }
}
