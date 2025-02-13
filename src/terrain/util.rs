pub struct XYZ {
    size: usize,
    index: usize,
}

impl XYZ {
    pub fn new(size: usize) -> XYZ {
        XYZ { size, index: 0 }
    }
}

impl Iterator for XYZ {
    type Item = (usize, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.size * self.size * self.size {
            return None;
        }

        let x = self.index % self.size;
        let y = (self.index / self.size) % self.size;
        let z = self.index / (self.size * self.size);
        self.index += 1;

        Some((x, y, z))
    }
}
/// Gets the position of the vert from the range of 0-7.
pub const fn position_from_index(index: usize) -> (usize, usize, usize) {
    ((index & 1) >> 0, (index & 2) >> 1, (index & 4) >> 2)
}