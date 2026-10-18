//! Hand-made scalar fields for the demo scene and the tests.
//!
//! Remember the sign convention: `> ISO` is solid, so *raising* a sample fills
//! it in. Most fields write their samples relative to [`ISO`]: an empty
//! background of `ISO - 2`, solid at `ISO + 2` and up. They were written for an
//! `i8` field at isolevel 2 and moved onto the `u8` scale by a plain shift, so
//! every vertex and gradient comes out bit for bit where it did then.

use super::{VoxelBuffer, ISO, S_MAX};

/// The background most fields sit in: empty, two below the isolevel.
const EMPTY: u8 = ISO - 2;

/// An 8^3 lump of terrain-ish data, on `ISO - 2 ..= ISO + 3`.
pub fn terrain() -> VoxelBuffer {
    #[rustfmt::skip]
    let samples = vec![
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10, 10, 10, 10, 10, 10, 10, 10, 10,  7,  8, 10, 10, 10,  7,  8,
         5,  5,  5,  6,  8,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10,  9, 10, 10, 10,  7,  5,  7,  7,  5,  5,  6,  6,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,  7,  6,  9,
        10, 10, 10, 10,  7,  5,  5,  5,  8,  8,  8,  6,  5,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,  7,  6, 10,
        10, 10, 10, 10,  8,  5,  5,  5, 10, 10, 10, 10,  5,  5,  5,  5,
         6, 10, 10,  6,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
         5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10, 10, 10, 10, 10,  7,  6,  9, 10, 10, 10, 10, 10,  5,  5,  5,
         8, 10, 10, 10,  5,  5,  5,  5,  5, 10, 10, 10,  5,  5,  5,  5,
         5,  6, 10,  6,  5,  5,  5,  5,  5,  5,  6,  5,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,  7,  8,
         5, 10, 10, 10, 10,  5,  5,  5,  5, 10, 10, 10, 10,  5,  5,  5,
         5,  5, 10, 10,  5,  5,  5,  5,  5,  5, 10, 10,  5,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10, 10, 10, 10, 10, 10, 10, 10,  7, 10, 10, 10, 10, 10, 10, 10,
         5, 10, 10, 10, 10, 10, 10,  8,  5,  7, 10, 10, 10, 10,  5,  5,
         5,  5, 10, 10, 10,  5,  5,  5,  5,  5,  9, 10,  9,  5,  5,  5,
        10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        10, 10, 10, 10, 10, 10, 10, 10,  8, 10, 10, 10, 10, 10, 10, 10,
         5, 10, 10, 10, 10, 10, 10, 10,  5,  8, 10, 10, 10, 10, 10, 10,
         5,  5, 10, 10, 10, 10, 10,  5,  5,  5,  9, 10, 10, 10,  6,  5,
    ];
    VoxelBuffer::from_samples(samples, 8)
}

/// A solid cube filling everything but a one-sample border.
pub fn solid_cube(size: usize) -> VoxelBuffer {
    let mut buffer = filled(size, EMPTY);
    for z in 1..size - 1 {
        for y in 1..size - 1 {
            for x in 1..size - 1 {
                buffer.set(x, y, z, ISO + 2);
            }
        }
    }
    buffer
}

/// A single solid voxel in the middle of a 3^3 field.
pub fn single_voxel() -> VoxelBuffer {
    let mut buffer = filled(3, EMPTY);
    buffer.set(1, 1, 1, ISO + 3);
    buffer
}

/// A scattering of solid voxels, including two that touch only at a corner.
pub fn scattered() -> VoxelBuffer {
    let mut buffer = filled(3, EMPTY);

    let coords = [
        (2, 1, 0),
        (2, 1, 2),
        (0, 0, 0),
        (1, 0, 0),
        (2, 0, 0),
        (0, 0, 1),
        (0, 0, 2),
    ];
    for (x, y, z) in coords {
        buffer.set(x, y, z, ISO + 3);
    }

    buffer
}

/// A horizontal slab: the surface cuts only Y edges, never an X edge. This is
/// the shape that a trim based on cut X edges throws away entirely.
pub fn flat_slab(size: usize) -> VoxelBuffer {
    let mut buffer = filled(size, EMPTY);
    for z in 0..size {
        for y in 0..size / 2 {
            for x in 0..size {
                buffer.set(x, y, z, ISO + 3);
            }
        }
    }
    buffer
}

/// A solid ball, smooth enough to exercise interpolation properly.
///
/// The field rises 3 per sample into the ball and saturates at 0 and
/// [`S_MAX`], a little over two samples either side of the surface.
pub fn sphere(size: usize, radius: f32) -> VoxelBuffer {
    let mut buffer = VoxelBuffer::new(size);
    let centre = (size as f32 - 1.0) / 2.0;

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 - centre).powi(2)
                    + (y as f32 - centre).powi(2)
                    + (z as f32 - centre).powi(2))
                .sqrt();

                // Solid inside, so the field has to run high inside.
                let lift = ((radius - d) * 3.0).clamp(-(ISO as f32), (S_MAX - ISO) as f32) as i8;
                let mut v = ISO.saturating_add_signed(lift);
                // A sample sitting exactly on the isolevel interpolates to
                // t == 0, collapsing vertices onto grid corners and emitting
                // zero-area triangles. Legal output, but not useful to look at.
                if v == ISO {
                    v = ISO + 1;
                }
                buffer.set(x, y, z, v);
            }
        }
    }

    buffer
}

/// A deterministic pseudo-random field, for broad case coverage. Dense and
/// ugly on purpose: it hits marching cubes cases a tidy field never will.
pub fn pseudo_random(size: usize, seed: u32) -> VoxelBuffer {
    let mut buffer = VoxelBuffer::new(size);
    let mut state = seed;

    for sample in buffer.samples_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *sample = EMPTY + ((state >> 16) % 5) as u8;
    }

    buffer
}

/// A `size`^3 buffer with every sample set to `value`.
pub fn filled(size: usize, value: u8) -> VoxelBuffer {
    VoxelBuffer::from_samples(vec![value; size.pow(3)], size)
}
