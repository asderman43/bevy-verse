//! Storage for the scalar field that gets turned into a surface.

/// A cubic block of voxel samples, laid out X-major so that a row of samples
/// along X is contiguous.
///
/// Flying edges scans the field one X row at a time, so that layout is not
/// incidental -- [`VoxelBuffer::row`] handing back a plain slice is what keeps
/// its first pass cache friendly.
///
/// A sample of `<= isolevel` is empty space and `> isolevel` is solid; see the
/// module docs for why that direction is forced by the triangle table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelBuffer {
    samples: Vec<i8>,
    size: usize,
}

impl VoxelBuffer {
    /// An all-zero buffer `size` samples along each axis.
    pub fn new(size: usize) -> Self {
        VoxelBuffer {
            samples: vec![0; size.pow(3)],
            size,
        }
    }

    /// Wraps existing samples.
    ///
    /// # Panics
    /// If `samples.len()` is not `size.pow(3)`.
    pub fn from_samples(samples: Vec<i8>, size: usize) -> Self {
        assert_eq!(
            samples.len(),
            size.pow(3),
            "a {size}^3 buffer needs {} samples, got {}",
            size.pow(3),
            samples.len(),
        );
        VoxelBuffer { samples, size }
    }

    /// Samples per axis. Extraction needs at least 2 to form a single cell.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Cells per axis, i.e. `size - 1` (saturating, so a degenerate buffer
    /// reports no cells rather than underflowing).
    pub fn cells_per_axis(&self) -> usize {
        self.size.saturating_sub(1)
    }

    pub fn samples(&self) -> &[i8] {
        &self.samples
    }

    pub fn samples_mut(&mut self) -> &mut [i8] {
        &mut self.samples
    }

    /// Flat index of a sample. X-major.
    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.size + z * self.size * self.size
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> i8 {
        self.samples[self.index(x, y, z)]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: i8) {
        let index = self.index(x, y, z);
        self.samples[index] = value;
    }

    /// The contiguous run of `size` samples along X at `(y, z)`.
    pub fn row(&self, y: usize, z: usize) -> &[i8] {
        let start = self.index(0, y, z);
        &self.samples[start..start + self.size]
    }

    /// The eight corners of the cell whose lowest corner is `(x, y, z)`,
    /// ordered so that corner `i` sits at
    /// `x + (i & 1)`, `y + ((i >> 1) & 1)`, `z + ((i >> 2) & 1)`.
    ///
    /// That order is what the triangle tables index by, so it must not change.
    pub fn cell_corners(&self, x: usize, y: usize, z: usize) -> [i8; 8] {
        [
            self.get(x, y, z),
            self.get(x + 1, y, z),
            self.get(x, y + 1, z),
            self.get(x + 1, y + 1, z),
            self.get(x, y, z + 1),
            self.get(x + 1, y, z + 1),
            self.get(x, y + 1, z + 1),
            self.get(x + 1, y + 1, z + 1),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::VoxelBuffer;

    #[test]
    fn row_is_contiguous_along_x() {
        let size = 4;
        let mut buffer = VoxelBuffer::new(size);
        for x in 0..size {
            buffer.set(x, 2, 3, x as i8);
        }
        assert_eq!(buffer.row(2, 3), &[0, 1, 2, 3]);
    }

    #[test]
    fn cell_corners_follow_the_table_convention() {
        let size = 2;
        let mut buffer = VoxelBuffer::new(size);
        // Store each corner's own index, so the returned array should come
        // back as 0..8 exactly when the ordering is right.
        for i in 0..8usize {
            buffer.set(i & 1, (i >> 1) & 1, (i >> 2) & 1, i as i8);
        }
        assert_eq!(buffer.cell_corners(0, 0, 0), [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    #[should_panic(expected = "needs 27 samples")]
    fn from_samples_rejects_a_bad_length() {
        VoxelBuffer::from_samples(vec![0; 26], 3);
    }
}
