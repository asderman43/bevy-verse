//! The buffers an extraction writes its surface into -- the ones that leave.
//!
//! Emission writes straight into the `Vec`s the finished [`Surface`] will own,
//! and those move on into the `Mesh` without being copied. Pooling them in
//! [`super::ExtractionScratch`] would not save anything: a surface outlives the
//! extraction, so its memory has to be new every time regardless, and a pooled
//! buffer only adds a zero-fill before emission and a full copy after it.
//!
//! # Uninitialized, on purpose
//!
//! The scan sizes the output exactly and emission writes every slot exactly
//! once -- that is the ownership rule in `flying_edges` and the one-cursor
//! rule in `marching_cubes`. Zero-filling first would be a pass over the whole
//! output whose every byte is then overwritten, so the buffers are allocated
//! uninitialized, written through [`MaybeUninit`] slices, and only given a
//! length by [`Output::finish`] once emission is done. That is the one
//! `unsafe` step, and its whole justification is the "every slot exactly
//! once" invariant.
//!
//! Debug builds don't take that on trust: they pre-fill every slot with a
//! sentinel no emission can produce, and `finish` panics if one survives. So
//! under `cargo test` an emission bug fails loudly instead of reading
//! uninitialized memory.

use std::mem::MaybeUninit;

use bevy::prelude::*;

use super::Surface;

/// Output buffers of an exact size, awaiting emission.
pub(crate) struct Output {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    indices: Vec<u32>,
    vertex_count: usize,
    index_count: usize,
}

/// The slots emission writes into: one per vertex in the first two, one per
/// index in the third.
pub(crate) struct Slots<'a> {
    pub positions: &'a mut [MaybeUninit<Vec3>],
    pub normals: &'a mut [MaybeUninit<Vec3>],
    pub indices: &'a mut [MaybeUninit<u32>],
}

impl Output {
    /// Allocates room for exactly `vertices` vertices and `indices` indices,
    /// without initializing any of it.
    pub(crate) fn new(vertices: usize, indices: usize) -> Self {
        let mut output = Output {
            positions: Vec::with_capacity(vertices),
            normals: Vec::with_capacity(vertices),
            indices: Vec::with_capacity(indices),
            vertex_count: vertices,
            index_count: indices,
        };

        #[cfg(debug_assertions)]
        {
            let slots = output.slots();
            slots.positions.fill(MaybeUninit::new(Vec3::NAN));
            slots.normals.fill(MaybeUninit::new(Vec3::NAN));
            slots.indices.fill(MaybeUninit::new(u32::MAX));
        }

        output
    }

    /// The uninitialized slots, sized exactly to the counts given to
    /// [`Self::new`]. (`with_capacity` may round up; the extra is not ours to
    /// write.)
    pub(crate) fn slots(&mut self) -> Slots<'_> {
        Slots {
            positions: &mut self.positions.spare_capacity_mut()[..self.vertex_count],
            normals: &mut self.normals.spare_capacity_mut()[..self.vertex_count],
            indices: &mut self.indices.spare_capacity_mut()[..self.index_count],
        }
    }

    /// Hands the written buffers over as a [`Surface`], without copying.
    ///
    /// # Safety
    /// Every slot handed out by [`Self::slots`] must have been written. Debug
    /// builds check this and panic; release builds trust it.
    pub(crate) unsafe fn finish(mut self) -> Surface {
        #[cfg(debug_assertions)]
        {
            let (positions, normals, indices) = self.unwritten();
            assert!(
                positions == 0 && normals == 0 && indices == 0,
                "emission left slots unwritten: {positions} positions, \
                 {normals} normals, {indices} indices",
            );
        }

        // SAFETY: the caller guarantees every slot in `..vertex_count` and
        // `..index_count` was written, and `new` reserved at least that much.
        unsafe {
            self.positions.set_len(self.vertex_count);
            self.normals.set_len(self.vertex_count);
            self.indices.set_len(self.index_count);
        }

        Surface {
            positions: self.positions,
            normals: self.normals,
            indices: self.indices,
        }
    }

    /// How many slots of each buffer still hold the debug sentinel, i.e. were
    /// never written. NaN is safe as a sentinel for both vertex buffers: no
    /// position or normal is ever NaN, which the normal tests pin down.
    #[cfg(debug_assertions)]
    pub(crate) fn unwritten(&mut self) -> (usize, usize, usize) {
        let slots = self.slots();
        // SAFETY: in debug builds `new` initialized every slot with the
        // sentinel, and emission only ever overwrites them with values.
        unsafe {
            (
                slots.positions.iter().filter(|v| v.assume_init_ref().is_nan()).count(),
                slots.normals.iter().filter(|v| v.assume_init_ref().is_nan()).count(),
                slots.indices.iter().filter(|i| *i.assume_init_ref() == u32::MAX).count(),
            )
        }
    }
}
