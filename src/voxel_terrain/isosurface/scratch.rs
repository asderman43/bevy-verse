//! The buffer pool the extractors work out of.
//!
//! Extraction is a per-chunk operation that runs over and over for the whole
//! life of the app, and every buffer it needs is sized from the chunk size or
//! from counts the passes themselves produce. Allocating those fresh each run
//! means the allocator does the same work thousands of times to hand back
//! memory of very nearly the same size.
//!
//! The one thing that does *not* live here is the output. A surface outlives
//! the extraction that made it and ends up owned by a `Mesh`, so its memory has
//! to be new every time whatever happens; pooling it would only add a zero-fill
//! before emission and a full copy after. Emission writes straight into the
//! surface's own buffers instead -- see [`super::output`].
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
}

impl ExtractionScratch {
    /// An empty pool. The first extraction grows it.
    pub fn new() -> Self {
        Self::default()
    }

    /// A pool already big enough for the per-row buffers of a `size`-per-axis
    /// chunk, so the first extraction of that size allocates nothing for them.
    ///
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
        let big = (
            scratch.corner_masks.capacity(),
            scratch.rows.capacity(),
            scratch.offsets.capacity(),
        );

        // A much smaller chunk afterwards must not hand the memory back.
        scratch.reset_rows(4);

        assert_eq!(scratch.corner_masks.len(), 16, "row buffer resized");
        assert!(scratch.corner_masks.capacity() >= big.0, "mask capacity lost");
        assert!(scratch.rows.capacity() >= big.1, "row capacity lost");
        assert!(scratch.offsets.capacity() >= big.2, "offset capacity lost");
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
