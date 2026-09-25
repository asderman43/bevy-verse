//! The buffer pool the extractors work out of.
//!
//! Extraction is a per-chunk operation that runs over and over for the whole
//! life of the app, and every buffer it needs is sized from the chunk size or
//! from counts the passes themselves produce. Allocating those fresh each run
//! means the allocator does the same work thousands of times to hand back
//! memory of very nearly the same size.
//!
//! So nothing in here is ever freed while the pool is alive. Every buffer is
//! reset with `clear` + `resize`, which keeps the allocation and only grows it
//! when a chunk needs more than any chunk before it -- after a handful of
//! extractions the pool has reached its high-water mark and stops calling the
//! allocator entirely.
//!
//! That is also why there is no `shrink_to_fit` anywhere below, and why the
//! buffers are reset rather than reassigned: `self.rows = Vec::new()` would
//! quietly undo the whole point.

use bevy::prelude::*;

use super::scan::{RowMetadata, RowOffsets};

/// Scratch space for isosurface extraction, reused across runs.
///
/// Held as a resource by [`super::IsosurfacePlugin`] so the pool outlives any
/// single extraction. Outside the ECS, keep one next to whatever drives the
/// extractor and pass it to [`super::extract_with`] each time; a throwaway
/// `ExtractionScratch::new()` per call works but gives up the whole benefit.
#[derive(Resource, Default, Debug)]
pub struct ExtractionScratch {
    /// Scan, pass 1: one bitmask per sample row `(y, z)`, bit `x` set when
    /// sample `x` is `<= isolevel`. Indexed the same as [`Self::rows`].
    pub(crate) corner_masks: Vec<u64>,
    /// Scan, passes 1-2: per-row trim bounds and counts.
    pub(crate) rows: Vec<RowMetadata>,
    /// Scan, pass 3: per-row write offsets.
    pub(crate) offsets: Vec<RowOffsets>,
    /// Output vertices, written by emission into the slots pass 3 reserved.
    pub(crate) positions: Vec<Vec3>,
    /// Output indices.
    pub(crate) indices: Vec<u32>,
}

impl ExtractionScratch {
    /// An empty pool. The first extraction grows it.
    pub fn new() -> Self {
        Self::default()
    }

    /// A pool already big enough for the per-row buffers of a `size`-per-axis
    /// chunk, so the first extraction of that size allocates nothing for them.
    ///
    /// The output buffers are left alone: their size depends on how much
    /// surface the field actually contains, so they find their own high-water
    /// mark over the first few runs.
    pub fn with_capacity(size: usize) -> Self {
        let mut scratch = Self::new();
        scratch.reserve(size);
        scratch
    }

    /// Grows the per-row buffers to fit a `size`-per-axis chunk. Never shrinks.
    pub fn reserve(&mut self, size: usize) {
        let rows = size * size;
        grow(&mut self.corner_masks, rows);
        grow(&mut self.rows, rows);
        grow(&mut self.offsets, rows);
    }

    /// Roughly how much memory the pool is holding onto, for debug output.
    pub fn capacity_bytes(&self) -> usize {
        use std::mem::size_of;

        self.corner_masks.capacity() * size_of::<u64>()
            + self.rows.capacity() * size_of::<RowMetadata>()
            + self.offsets.capacity() * size_of::<RowOffsets>()
            + self.positions.capacity() * size_of::<Vec3>()
            + self.indices.capacity() * size_of::<u32>()
    }

    /// Resets the per-row buffers to `size * size` default entries, keeping
    /// whatever capacity they already have.
    pub(crate) fn reset_rows(&mut self, size: usize) {
        let rows = size * size;

        self.corner_masks.clear();
        self.corner_masks.resize(rows, 0);

        self.rows.clear();
        self.rows.resize(rows, RowMetadata::default());

        self.offsets.clear();
        self.offsets.resize(rows, RowOffsets::default());
    }

    /// Resets the output buffers to exactly the sizes pass 3 counted, so pass 4
    /// can write into them by index.
    pub(crate) fn reset_output(&mut self, vertices: usize, indices: usize) {
        self.positions.clear();
        self.positions.resize(vertices, Vec3::ZERO);

        self.indices.clear();
        self.indices.resize(indices, 0);
    }

    /// Empties the output buffers without giving up their capacity, for a
    /// field too small to hold a cell.
    pub(crate) fn clear_output(&mut self) {
        self.positions.clear();
        self.indices.clear();
    }

    /// Copies the finished surface out of the pool.
    ///
    /// This is the one copy the pool cannot avoid: a [`super::Surface`] outlives
    /// the extraction that produced it and ends up owned by a [`Mesh`], whereas
    /// the pool's buffers have to stay behind to be reused. It is two memcpys
    /// of already-sized, already-contiguous data, against passes that touched
    /// every cell in the chunk.
    pub(crate) fn surface(&self) -> super::Surface {
        super::Surface {
            positions: self.positions.clone(),
            indices: self.indices.clone(),
        }
    }
}

/// Reserves room for `len` entries without shrinking or changing the length.
fn grow<T>(buffer: &mut Vec<T>, len: usize) {
    if buffer.capacity() < len {
        buffer.reserve(len - buffer.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resetting_keeps_capacity() {
        let mut scratch = ExtractionScratch::new();

        scratch.reset_rows(16);
        scratch.reset_output(1000, 3000);
        let big = (
            scratch.corner_masks.capacity(),
            scratch.positions.capacity(),
            scratch.indices.capacity(),
        );

        // A much smaller chunk afterwards must not hand the memory back.
        scratch.reset_rows(4);
        scratch.reset_output(1, 3);

        assert_eq!(scratch.corner_masks.len(), 16, "row buffer resized");
        assert_eq!(scratch.positions.len(), 1, "vertex buffer resized");
        assert!(scratch.corner_masks.capacity() >= big.0, "row capacity lost");
        assert!(scratch.positions.capacity() >= big.1, "vertex capacity lost");
        assert!(scratch.indices.capacity() >= big.2, "index capacity lost");
    }

    #[test]
    fn reserve_does_not_change_length() {
        let mut scratch = ExtractionScratch::with_capacity(32);
        assert_eq!(scratch.rows.len(), 0);
        assert!(scratch.rows.capacity() >= 32 * 32);

        scratch.reserve(8);
        assert!(scratch.rows.capacity() >= 32 * 32, "reserve shrank the pool");
    }
}
