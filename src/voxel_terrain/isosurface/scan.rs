//! Counting the surface before writing it: flying edges' passes 1-3.
//!
//! These three passes never emit a triangle. They work out, for every row of
//! the field, where its geometry can be (the trim), how many vertices and
//! triangles it will produce, and where in the output those belong. What is
//! left afterwards is emission, which is the only part the two extractors
//! actually disagree about:
//!
//! - [`super::flying_edges`] walks the same rows again and writes a **welded**
//!   mesh into the slots the scan reserved, one vertex per cut edge shared by
//!   every cell that touches it.
//! - [`super::marching_cubes`] writes **unwelded** triangles, three fresh
//!   vertices each, which is what gives it hard faces.
//!
//! So the scan lives here rather than in either of them. Both get exactly
//! sized buffers out of it; neither can discover its output size while
//! producing it, which is the whole trick, and it is worth as much to a
//! cell-by-cell emitter as to flying edges' own.
//!
//! # Row bitmasks
//!
//! Pass 1 classifies a row of samples into a single `u64`: bit `x` is set when
//! sample `x` is `<= isolevel`. A chunk is at most [`MAX_SIZE`] samples along
//! an axis, so a row always fits in one word.
//!
//! That is a smaller buffer than a byte per edge -- `size * size` words against
//! `(size - 1) * size * size` bytes, 32 KiB against 252 KiB for a 64^3 chunk --
//! but the size is the lesser point. It is the representation the algorithm
//! wants: an X edge's 2-bit case is `(mask >> x) & 0b11` with no lookup, and
//! pass 1's whole per-edge loop -- cut edges, vertex counts, trim bounds --
//! collapses into three bit operations on the row as a whole. See
//! [`RowScan::pass_1`].

use super::{
    scratch::ExtractionScratch,
    tables::{EDGE_INTERSECTION, TRIANGLE_COUNT},
    VoxelBuffer, MAX_SIZE,
};

/// What passes 1 and 2 work out about a single row of X edges at some `(y, z)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowMetadata {
    pub yz: (usize, usize),
    /// First and last X edge that can bound geometry. `left_trim > right_trim`
    /// means the row is empty, which makes `left_trim..=right_trim` skip it.
    pub left_trim: usize,
    pub right_trim: usize,
    /// Vertices this row contributes, split by the axis of the cut edge. Only
    /// filled in by a scan that counts vertices; see [`RowScan::count`].
    pub x_vertices: u32,
    pub y_vertices: u32,
    pub z_vertices: u32,
    pub triangles: u32,
}

/// Where a row's output starts, after pass 3 has prefix-summed the counts.
#[derive(Clone, Debug, Default)]
pub struct RowOffsets {
    pub yz: (usize, usize),
    /// First vertex slot for this row's X-, Y- and Z-axis vertices. They sit
    /// in that order, contiguously. Welded emission only.
    pub x: u32,
    pub y: u32,
    pub z: u32,
    /// First index slot for this row's triangles. Also where an unwelded
    /// emission's vertices start, since it writes one vertex per index.
    pub indices: u32,
    pub left_trim: usize,
    pub right_trim: usize,
}

/// Exact output sizes for a field, from [`RowScan::count`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Vertices a welded emission produces: one per cut edge in the field.
    pub vertices: usize,
    /// Indices any emission produces: three per triangle.
    pub indices: usize,
}

/// Row-major index of the row at `(y, z)`.
pub fn row_index(size: usize, y: usize, z: usize) -> usize {
    y + z * size
}

/// The four sample rows bounding the cell row at `(y, z)`, in the order the
/// four X-edge slots of [`EDGE_INTERSECTION`] use -- bit 0 is this row's own X
/// edge, bit 1 the row behind it along Z, bit 2 the row above it along Y, bit 3
/// the diagonal.
///
/// This is *not* the order [`RowScan::cell_case`] reads its rows in; a cell's
/// case bits and its edge bits are numbered differently. Keeping the two apart
/// is why they are separate functions.
pub fn bounding_rows(size: usize, y: usize, z: usize) -> [usize; 4] {
    [
        /* (y,   z  ) */ row_index(size, y, z),
        /* (y,   z+1) */ row_index(size, y, z + 1),
        /* (y+1, z  ) */ row_index(size, y + 1, z),
        /* (y+1, z+1) */ row_index(size, y + 1, z + 1),
    ]
}

/// Anything carrying a row's trim bounds. Passes 2 and 3 produce them and
/// emission consumes them, out of two different structs.
pub trait Trim {
    fn trim(&self) -> (usize, usize);
}

impl Trim for RowMetadata {
    fn trim(&self) -> (usize, usize) {
        (self.left_trim, self.right_trim)
    }
}

impl Trim for RowOffsets {
    fn trim(&self) -> (usize, usize) {
        (self.left_trim, self.right_trim)
    }
}

/// Widest trim across a set of rows. Sound because a cell can only hold
/// geometry if at least one bounding row's edge there is non-uniform.
pub fn combined_trim<T: Trim>(rows: &[T], bounding: &[usize; 4]) -> (usize, usize) {
    let (mut left, mut right) = rows[bounding[0]].trim();
    for &row in bounding {
        let (l, r) = rows[row].trim();
        left = left.min(l);
        right = right.max(r);
    }
    (left, right)
}

/// Bit `n` of a cut mask, as a count to add.
pub fn bit(cut: u16, n: u32) -> u32 {
    ((cut >> n) & 1) as u32
}

/// The low `n` bits set. `n` is never above 63 here, but the branch keeps a
/// shift of 64 (which is UB-adjacent and panics in debug) impossible.
fn low_bits(n: usize) -> u64 {
    if n >= 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

/// The counting half of extraction, over one field.
pub struct RowScan<'a> {
    buffer: &'a VoxelBuffer,
    isolevel: i8,
}

impl<'a> RowScan<'a> {
    /// # Panics
    /// If `buffer.size()` exceeds [`MAX_SIZE`]; a row of corners has to fit in
    /// a `u64`.
    pub fn new(buffer: &'a VoxelBuffer, isolevel: i8) -> Self {
        assert!(
            buffer.size() <= MAX_SIZE,
            "a row of corners is packed into a u64, so a chunk can be at most \
             {MAX_SIZE} samples per axis, got {}",
            buffer.size(),
        );
        RowScan { buffer, isolevel }
    }

    pub fn buffer(&self) -> &VoxelBuffer {
        self.buffer
    }

    pub fn isolevel(&self) -> i8 {
        self.isolevel
    }

    /// Runs all three passes, counting vertices as well as triangles, and
    /// leaves the row masks, metadata and offsets in `scratch`.
    ///
    /// For welded emission, which needs every per-row vertex offset.
    ///
    /// # Panics
    /// If the field is too small to hold a cell. Callers guard for that before
    /// they get here, since they have to return an empty surface anyway.
    pub fn count(&self, scratch: &mut ExtractionScratch) -> Counts {
        self.scan::<true>(scratch)
    }

    /// Runs all three passes, counting only triangles, and returns how many
    /// indices the field will produce.
    ///
    /// For unwelded emission, which writes one vertex per index and so needs
    /// no vertex counts at all -- skipping them is most of pass 2's per-cell
    /// work. The vertex offsets left in `scratch` are meaningless afterwards;
    /// only [`RowOffsets::indices`] and the trim bounds are valid.
    ///
    /// # Panics
    /// As [`Self::count`].
    pub fn count_triangles(&self, scratch: &mut ExtractionScratch) -> usize {
        self.scan::<false>(scratch).indices
    }

    fn scan<const VERTICES: bool>(&self, scratch: &mut ExtractionScratch) -> Counts {
        let size = self.buffer.size();
        assert!(size >= 2, "a scan needs a field big enough to hold a cell");

        scratch.reset_rows(size);

        // Each pass reads some of the pool's buffers and writes others, so the
        // passes take their buffers individually and the destructuring here is
        // what splits the borrow of the pool between them.
        let ExtractionScratch {
            corner_masks,
            rows,
            offsets,
            ..
        } = &mut *scratch;

        self.pass_1(corner_masks, rows);
        self.pass_2::<VERTICES>(corner_masks, rows);
        self.pass_3(rows, offsets)
    }

    /// Pass 1: classify the X edges of every row, and record its trim bounds
    /// and X-vertex count.
    ///
    /// A row becomes one `u64` with bit `x` set when sample `x` is
    /// `<= isolevel`, which makes everything else a bit operation on the whole
    /// row at once. Shifting it down by one lines each edge's high corner up
    /// with its low corner, so against `mask`:
    ///
    /// - `mask ^ (mask >> 1)` marks the edges whose corners disagree, i.e. the
    ///   ones the surface crosses. Its population count is the row's X-vertex
    ///   count.
    /// - `mask | (mask >> 1)` marks the edges with at least one corner set,
    ///   which is what the trim has to bound; the first and last set bit are
    ///   the bounds.
    ///
    /// An individual edge's 2-bit case is still in there as
    /// `(mask >> x) & 0b11` -- bit 0 its low corner, bit 1 its high corner --
    /// and that is how [`Self::cell_case`] reads it back.
    fn pass_1(&self, corner_masks: &mut [u64], rows: &mut [RowMetadata]) {
        let size = self.buffer.size();
        let edges_per_row = size - 1;
        let edge_mask = low_bits(edges_per_row);

        for z in 0..size {
            for y in 0..size {
                let samples = self.buffer.row(y, z);

                let mut mask = 0u64;
                for (x, &sample) in samples.iter().enumerate() {
                    mask |= ((sample <= self.isolevel) as u64) << x;
                }

                // Only an edge whose corners disagree produces a vertex.
                let cut = (mask ^ (mask >> 1)) & edge_mask;

                // The trim has to bound every cell that can produce geometry,
                // not just cells with a *cut* X edge: a cell is uniform only
                // when all eight corners agree, so the bound is "this edge has
                // at least one corner set". Trimming on cut edges alone loses
                // entire surfaces -- a flat slab cuts only Y edges -- and
                // desyncs pass 2's counts from emission's writes, which then
                // runs off the end of the buffer.
                let touched = (mask | (mask >> 1)) & edge_mask;

                // An empty row comes out as `left > right` so downstream ranges
                // skip it. Using 0 as the "unset" marker instead is wrong twice
                // over: an empty row looks like it spans cell 0, and a row whose
                // first cut really is at 0 is indistinguishable from it.
                let (left, right) = if touched == 0 {
                    (edges_per_row, 0)
                } else {
                    (
                        touched.trailing_zeros() as usize,
                        63 - touched.leading_zeros() as usize,
                    )
                };

                let row = row_index(size, y, z);
                corner_masks[row] = mask;
                rows[row] = RowMetadata {
                    yz: (y, z),
                    left_trim: left,
                    right_trim: right,
                    x_vertices: cut.count_ones(),
                    ..Default::default()
                };
            }
        }
    }

    /// Pass 2: for each row of cells, count the triangles it produces and, with
    /// `VERTICES`, the Y/Z vertices it is responsible for.
    ///
    /// A row owns the Y and Z edges hanging off its own corner of each cell.
    /// The edges on a cell's far side belong to the next row along, and are
    /// added straight onto it -- except at the volume boundary, where there is
    /// no next row and this row has to account for them.
    fn pass_2<const VERTICES: bool>(&self, corner_masks: &[u64], rows: &mut [RowMetadata]) {
        let size = self.buffer.size();
        let last_row = size - 2;
        let last_cell = size - 2;

        for z in 0..=last_row {
            for y in 0..=last_row {
                let bounding = bounding_rows(size, y, z);
                let (left_trim, right_trim) = combined_trim(rows, &bounding);

                let mut y_vertices = 0;
                let mut z_vertices = 0;
                let mut triangles = 0;

                for x in left_trim..=right_trim {
                    let case = self.cell_case(corner_masks, x, y, z);
                    triangles += TRIANGLE_COUNT[case] as u32;

                    // Unwelded emission writes a vertex per index, so the
                    // triangle count already tells it everything.
                    if !VERTICES {
                        continue;
                    }

                    let cut = EDGE_INTERSECTION[case];

                    // Near-side Y and Z edges: always ours.
                    y_vertices += bit(cut, 4);
                    z_vertices += bit(cut, 8);

                    // Far side along X: no cell to our right to claim them.
                    if x == last_cell {
                        y_vertices += bit(cut, 5);
                        z_vertices += bit(cut, 9);
                    }
                    // Far side along Y: hand the Z edges to the row above.
                    if y == last_row {
                        rows[bounding[2]].z_vertices += bit(cut, 10);
                        if x == last_cell {
                            rows[bounding[2]].z_vertices += bit(cut, 11);
                        }
                    }
                    // Far side along Z: hand the Y edges to the row behind.
                    if z == last_row {
                        rows[bounding[1]].y_vertices += bit(cut, 6);
                        if x == last_cell {
                            rows[bounding[1]].y_vertices += bit(cut, 7);
                        }
                    }
                }

                let row = &mut rows[bounding[0]];
                row.triangles += triangles;
                row.y_vertices += y_vertices;
                row.z_vertices += z_vertices;
            }
        }
    }

    /// Pass 3: prefix-sum the per-row counts into write offsets, and report the
    /// exact output sizes so the buffers can be sized once.
    fn pass_3(&self, rows: &[RowMetadata], offsets: &mut [RowOffsets]) -> Counts {
        let mut vertices_so_far = 0;
        let mut triangles_so_far = 0;

        for (row, offset) in rows.iter().zip(offsets.iter_mut()) {
            // Within a row the vertices are grouped X, then Y, then Z.
            *offset = RowOffsets {
                yz: row.yz,
                x: vertices_so_far,
                y: vertices_so_far + row.x_vertices,
                z: vertices_so_far + row.x_vertices + row.y_vertices,
                indices: triangles_so_far * 3,
                left_trim: row.left_trim,
                right_trim: row.right_trim,
            };

            vertices_so_far += row.x_vertices + row.y_vertices + row.z_vertices;
            triangles_so_far += row.triangles;
        }

        Counts {
            vertices: vertices_so_far as usize,
            indices: triangles_so_far as usize * 3,
        }
    }

    /// The marching-cubes case for a cell, assembled from the four X-edge
    /// cases that bound it rather than from its eight corners.
    ///
    /// Each bounding row contributes the 2 bits of its corner mask at `x`: the
    /// cell's near and far corner along X in that row. The row order here is
    /// what fixes the case bit layout: bits 0-1 come from `(y, z)`, 2-3 from
    /// `(y+1, z)`, 4-5 from `(y, z+1)`, 6-7 from `(y+1, z+1)`, which reproduces
    /// the corner numbering in tables.rs.
    ///
    /// Four shifted loads against eight scattered ones for the corners, and no
    /// corner values are touched at all unless the cell turns out to produce
    /// geometry -- which for any realistic field is a small minority of cells.
    pub fn cell_case(&self, corner_masks: &[u64], x: usize, y: usize, z: usize) -> usize {
        let size = self.buffer.size();
        let rows = [
            row_index(size, y, z),
            row_index(size, y + 1, z),
            row_index(size, y, z + 1),
            row_index(size, y + 1, z + 1),
        ];

        let mut case = 0usize;
        for (i, &row) in rows.iter().enumerate() {
            case |= (((corner_masks[row] >> x) & 0b11) as usize) << (i * 2);
        }
        case
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel_terrain::isosurface::test_maps;

    /// The row mask is the field, bit for bit: set where the sample is empty.
    #[test]
    fn pass_1_packs_a_row_into_its_bits() {
        let size = 8;
        let mut buffer = VoxelBuffer::new(size);
        // Samples default to 0, which is `<= isolevel`, i.e. empty. Make
        // x = 2 and x = 5 of row (1, 3) solid.
        buffer.set(2, 1, 3, 5);
        buffer.set(5, 1, 3, 5);

        let scan = RowScan::new(&buffer, 0);
        let mut masks = vec![0u64; size * size];
        let mut rows = vec![RowMetadata::default(); size * size];
        scan.pass_1(&mut masks, &mut rows);

        let row = row_index(size, 1, 3);
        assert_eq!(masks[row], 0b1101_1011, "empty samples of row (1, 3)");
        // Edges 1,2 and 4,5 straddle a solid sample, so four X vertices.
        assert_eq!(rows[row].x_vertices, 4);
        // Every edge of this row has an empty corner, so nothing is trimmed.
        assert_eq!((rows[row].left_trim, rows[row].right_trim), (0, size - 2));

        // A uniformly empty row has no cuts, but is still fully in trim: its
        // cells can hold geometry driven by the rows around it.
        let uniform = row_index(size, 0, 0);
        assert_eq!(masks[uniform], 0b1111_1111);
        assert_eq!(rows[uniform].x_vertices, 0);
        assert_eq!(
            (rows[uniform].left_trim, rows[uniform].right_trim),
            (0, size - 2),
        );
    }

    /// A row with no set corners at all is the one the trim has to reject, and
    /// it has to do it as an empty range rather than as cell 0.
    #[test]
    fn a_fully_solid_row_is_trimmed_away() {
        let size = 6;
        let mut buffer = VoxelBuffer::new(size);
        for (x, y, z) in crate::voxel_terrain::util::XYZ::new(size) {
            buffer.set(x, y, z, 9);
        }

        let scan = RowScan::new(&buffer, 0);
        let mut masks = vec![0u64; size * size];
        let mut rows = vec![RowMetadata::default(); size * size];
        scan.pass_1(&mut masks, &mut rows);

        let row = &rows[row_index(size, 2, 2)];
        assert_eq!(masks[row_index(size, 2, 2)], 0);
        assert!(
            row.left_trim > row.right_trim,
            "an all-solid row must produce an empty range, got {}..={}",
            row.left_trim,
            row.right_trim,
        );
        assert_eq!((row.left_trim..=row.right_trim).count(), 0);
    }

    /// Skipping the vertex counting must not change the triangle counting it
    /// shares a loop with.
    #[test]
    fn both_scans_agree_on_the_triangle_count() {
        for field in [
            test_maps::sphere(16, 5.0),
            test_maps::terrain(),
            test_maps::pseudo_random(10, 0xBEEF),
            test_maps::flat_slab(6),
        ] {
            let (buffer, isolevel) = test_maps::buffer(field);
            let scan = RowScan::new(&buffer, isolevel);

            let mut scratch = ExtractionScratch::new();
            let full = scan.count(&mut scratch);
            let triangles_only = scan.count_triangles(&mut scratch);

            assert_eq!(full.indices, triangles_only);
        }
    }

    /// A row has to fit in a `u64`, and the failure if it does not is a silent
    /// wrong answer, so it is worth being loud about.
    #[test]
    #[should_panic(expected = "at most 64 samples per axis")]
    fn a_chunk_over_the_size_limit_is_rejected() {
        RowScan::new(&VoxelBuffer::new(MAX_SIZE + 1), 2);
    }
}
