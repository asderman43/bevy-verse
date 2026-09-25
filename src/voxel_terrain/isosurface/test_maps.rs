//! Hand-made scalar fields for the demo scene and the tests.
//!
//! Each returns `(samples, isolevel, size)`. Remember the sign convention:
//! `> isolevel` is solid, so *setting* a sample fills it in.

use super::VoxelBuffer;

/// Convenience: the field as a buffer, plus its isolevel.
pub fn buffer(map: (Vec<i8>, i8, usize)) -> (VoxelBuffer, i8) {
    let (samples, isolevel, size) = map;
    (VoxelBuffer::from_samples(samples, size), isolevel)
}

/// An 8^3 lump of terrain-ish data.
pub fn terrain() -> (Vec<i8>, i8, usize) {
    (
        vec![
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 3, 5,
            5, 5, 2, 3, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5,
            5, 2, 0, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 2, 1, 4, 5, 5, 5, 5, 2, 0, 0, 0, 3, 3, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 1, 5, 5, 5, 5, 5, 3, 0, 0, 0, 5, 5, 5, 5, 0, 0, 0, 0,
            1, 5, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 1, 4,
            5, 5, 5, 5, 5, 0, 0, 0, 3, 5, 5, 5, 0, 0, 0, 0, 0, 5, 5, 5, 0, 0, 0, 0, 0, 1, 5, 1,
            0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 3, 0, 5, 5, 5, 5, 0, 0, 0, 0, 5, 5, 5,
            5, 0, 0, 0, 0, 0, 5, 5, 0, 0, 0, 0, 0, 0, 5, 5, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 5, 5, 5, 5, 5, 5, 5, 0, 5, 5, 5,
            5, 5, 5, 3, 0, 2, 5, 5, 5, 5, 0, 0, 0, 0, 5, 5, 5, 0, 0, 0, 0, 0, 4, 5, 4, 0, 0, 0,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 3, 5, 5, 5,
            5, 5, 5, 5, 0, 5, 5, 5, 5, 5, 5, 5, 0, 3, 5, 5, 5, 5, 5, 5, 0, 0, 5, 5, 5, 5, 5, 0,
            0, 0, 4, 5, 5, 5, 1, 0,
        ],
        2,
        8,
    )
}

/// A solid cube filling everything but a one-sample border.
pub fn solid_cube(size: usize) -> (Vec<i8>, i8, usize) {
    let mut map = vec![0; size.pow(3)];
    for z in 1..size - 1 {
        for y in 1..size - 1 {
            for x in 1..size - 1 {
                map[x + size * y + z * size * size] = 4;
            }
        }
    }
    (map, 2, size)
}

/// A single solid voxel in the middle of a 3^3 field.
pub fn single_voxel() -> (Vec<i8>, i8, usize) {
    let size: usize = 3;
    let mut map = vec![0; size.pow(3)];
    map[1 + size * 1 + size * size * 1] = 5;
    (map, 2, size)
}

/// A scattering of solid voxels, including two that touch only at a corner.
pub fn scattered() -> (Vec<i8>, i8, usize) {
    const SIZE: usize = 3;
    let mut map = vec![0; SIZE.pow(3)];

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
        map[x + y * SIZE + z * SIZE * SIZE] = 5;
    }

    (map, 2, SIZE)
}

/// A horizontal slab: the surface cuts only Y edges, never an X edge. This is
/// the shape that a trim based on cut X edges throws away entirely.
pub fn flat_slab(size: usize) -> (Vec<i8>, i8, usize) {
    let mut map = vec![0i8; size.pow(3)];
    for z in 0..size {
        for y in 0..size / 2 {
            for x in 0..size {
                map[x + y * size + z * size * size] = 5;
            }
        }
    }
    (map, 2, size)
}

/// A solid ball, smooth enough to exercise interpolation properly.
pub fn sphere(size: usize, radius: f32) -> (Vec<i8>, i8, usize) {
    let mut map = vec![0i8; size.pow(3)];
    let centre = (size as f32 - 1.0) / 2.0;

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 - centre).powi(2)
                    + (y as f32 - centre).powi(2)
                    + (z as f32 - centre).powi(2))
                .sqrt();

                // Solid inside, so the field has to run high inside.
                let mut v = ((radius - d) * 3.0).clamp(-40.0, 40.0) as i8 + 2;
                // A sample sitting exactly on the isolevel interpolates to
                // t == 0, collapsing vertices onto grid corners and emitting
                // zero-area triangles. Legal output, but not useful to look at.
                if v == 2 {
                    v = 3;
                }
                map[x + y * size + z * size * size] = v;
            }
        }
    }

    (map, 2, size)
}

/// A deterministic pseudo-random field, for broad case coverage. Dense and
/// ugly on purpose: it hits marching cubes cases a tidy field never will.
pub fn pseudo_random(size: usize, seed: u32) -> (Vec<i8>, i8, usize) {
    let mut map = vec![0i8; size.pow(3)];
    let mut state = seed;

    for sample in map.iter_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *sample = ((state >> 16) % 5) as i8;
    }

    (map, 2, size)
}
