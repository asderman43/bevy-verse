#[derive(Debug)]
pub struct Grid<T> {
    cells: Vec<T>,
    size: usize,
}

impl<T> Grid<T> {
    // TODO: Redo this when more logic is applied.
    pub fn new(cells: Vec<T>, size: usize) -> Self {
        Grid { cells: cells, size }
    }

    fn xyz_to_1d(&self, x: usize, y: usize, z: usize) -> usize {
        x + self.size * (y + z * self.size)
    }

    pub fn get_cell(&self, x: usize, y: usize, z: usize) -> Option<[&T; 8]> {
        // Actual size, because we are looking for neighbouring voxels.
        let size = self.size - 1;

        if x < size && y < size && z < size {
            let mut values: [&T; 8] = [&self.cells[0]; 8];

            for i in 0..8 {
                let xi = x + (i & 1);
                let yi = y + ((i >> 1) & 1);
                let zi = z + ((i >> 2) & 1);

                let index = self.xyz_to_1d(xi, yi, zi);
                values[i] = &self.cells[index];
            }

            return Some(values);
        }

        None
    }
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<&T> {
        let size = self.size;
        if x < size && y < size && z < size {
            let index = self.xyz_to_1d(x, y, z);
            return Some(&self.cells[index]);
        }

        None
    }

    pub fn get_mut(&mut self, x: usize, y: usize, z: usize) -> Option<&mut T> {
        let size = self.size;
        if x < size && y < size && z < size {
            let index = self.xyz_to_1d(x, y, z);
            return Some(&mut self.cells[index]);
        }

        None
    }

    pub fn get_edge_x(&self, y: usize, z: usize) -> Option<&[T]> {
        if y < self.size && z < self.size {
            let start = self.size * (y + z * self.size);
            let end = start + self.size;

            return Some(&self.cells[start..end]);
        }

        None
    }

    pub fn get_size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::Grid;
    const SIZE_FOR_TEST: usize = 16;

    fn dummy_grid() -> Vec<(usize, usize, usize)> {
        let mut vec = Vec::with_capacity(SIZE_FOR_TEST.pow(3));
        for z in 0..SIZE_FOR_TEST {
            for y in 0..SIZE_FOR_TEST {
                for x in 0..SIZE_FOR_TEST {
                    vec.push((x, y, z))
                }
            }
        }
        vec
    }

    #[test]
    fn new_grid() {
        let grid = Grid::new(dummy_grid(), SIZE_FOR_TEST);

        assert_eq!(grid.size, SIZE_FOR_TEST, "Test for Size");
        assert_eq!(
            grid.get(3, 2, 1),
            Some(&(3, 2, 1)),
            "Test for Value pairs at (3, 2, 1)"
        );
        assert_eq!(
            grid.get(SIZE_FOR_TEST, 0, 0),
            None,
            "Test for out of bounds"
        );
    }
    #[test]
    fn x_edge() {
        let grid = Grid::new(dummy_grid(), SIZE_FOR_TEST);

        let x_values: Vec<usize> = grid
            .get_edge_x(0, 0)
            .unwrap()
            .iter()
            .map(|(x, _, _)| *x)
            .collect();
        assert_eq!(x_values, (0..SIZE_FOR_TEST).collect::<Vec<_>>());

        let yz_values: Vec<(usize, usize)> = grid
            .get_edge_x(2, 2)
            .unwrap()
            .iter()
            .map(|t| (t.1, t.2))
            .collect();
        assert_eq!(yz_values.last(), Some(&(2, 2)));

        assert_eq!(grid.get_edge_x(SIZE_FOR_TEST, SIZE_FOR_TEST), None);
    }
    #[test]
    fn cell() {
        let grid = Grid::new(dummy_grid(), SIZE_FOR_TEST);

        let neighbors = grid.get_cell(1, 1, 1).unwrap();

        // Manually compute expected neighbors
        let expected = [
            &(1, 1, 1),
            &(2, 1, 1),
            &(1, 2, 1),
            &(2, 2, 1),
            &(1, 1, 2),
            &(2, 1, 2),
            &(1, 2, 2),
            &(2, 2, 2),
        ];

        assert_eq!(neighbors, expected);

        assert_eq!(
            grid.get_cell(SIZE_FOR_TEST, SIZE_FOR_TEST, SIZE_FOR_TEST),
            None
        );
    }
}
