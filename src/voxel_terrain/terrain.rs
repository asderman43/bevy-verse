use std::collections::HashMap;

use bevy::prelude::*;

use crate::voxel_terrain::voxels::grid::grid::Grid;

pub const SIZE_FOR_NOW: f32 = 5.0;

#[derive(Debug, Resource)]
pub struct Terrain {
    pub grids: HashMap<(i32, i32, i32), Grid<u8>>,
    pub size: usize,
}

// GlobalPosition()
// LocalPosition()
// ChunkId()
impl Terrain {
    pub fn new(size: usize) -> Self {
        let origin = Grid::new(vec![0; size.pow(3)], size);

        Terrain {
            grids: HashMap::from([((0, 0, 0), origin)]),
            size,
        }
    }

    pub fn get(&self, position: IVec3) -> Option<&u8> {
        let (chunk, local_pos) = pos_div_size(position, self.size as i32);

        let grid_opt = self.grids.get(&chunk);

        if let Some(grid) = grid_opt {
            return grid.get(local_pos.0, local_pos.1, local_pos.2);
        }

        None
    }
}

pub fn pos_div_size(pos: IVec3, size: i32) -> ((i32, i32, i32), (usize, usize, usize)) {
    fn minus_div(n: i32, s: i32) -> i32 {
        let c = if n < 0 { -1 } else { 0 };
        (n / s) + c
    }

    let chunk_x = minus_div(pos.x, size);
    let chunk_y = minus_div(pos.y, size);
    let chunk_z = minus_div(pos.z, size);

    let x = pos.x - (chunk_x * size);
    let y = pos.y - (chunk_y * size);
    let z = pos.z - (chunk_z * size);

    return (
        (chunk_x, chunk_y, chunk_z),
        (x as usize, y as usize, z as usize),
    );
}

#[cfg(test)]
pub mod tests {
    use bevy::math::IVec3;

    use crate::voxel_terrain::terrain::pos_div_size;

    #[test]
    fn global_to_local() {
        let size = 8;
        let origin = pos_div_size(IVec3::ZERO, size);

        assert_eq!(origin.0, (0, 0, 0));
        assert_eq!(origin.1, (0, 0, 0));

        let x_axis = pos_div_size(IVec3::new(12, 0, 0), size);
        assert_eq!(x_axis.0, (1, 0, 0));
        assert_eq!(x_axis.1, (4, 0, 0));

        let neg = pos_div_size(IVec3::new(-9, -9, -9), size);
        assert_eq!(neg.0, (-2, -2, -2));
        assert_eq!(neg.1, (7, 7, 7));
    }
}
