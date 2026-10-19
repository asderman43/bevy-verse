//! Debugging views into the water mesher: what it sees in a cell, without
//! meshing it. Nothing here is on the meshing path; it is for demos, tests
//! and, later, the terrain's debug tooling.

use super::{
    mesher::{slot_height, Corners},
    tables::VERT_EDGE,
};
use crate::voxel_terrain::isosurface::{Interpolate, VoxelBuffer};

/// What the mesher sees in one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellInfo {
    /// Each corner's bottom fill (`docs/water-surface.md` §1.3).
    pub bottom_fill: [u8; 8],
    /// `S`, `O` and the wet mask, bit per corner.
    pub solid: u8,
    pub inside: u8,
    pub wet: u8,
    /// `Q`: which slots carry the surface, bit per slot, in [`VERT_EDGE`] order.
    pub surface_slots: u8,
}

/// Describes the cell whose lowest corner is `cell`.
pub fn describe_cell(
    solid: &VoxelBuffer,
    water: &VoxelBuffer,
    cell: (usize, usize, usize),
    interpolate: Interpolate,
) -> CellInfo {
    let c = Corners::load(solid, water, cell);
    let surface_slots = (0..VERT_EDGE.len())
        .filter(|&k| slot_height(&c, k, interpolate).is_some())
        .fold(0, |q, k| q | 1 << k);
    CellInfo {
        bottom_fill: c.b,
        solid: c.solid,
        inside: c.inside,
        wet: c.wet,
        surface_slots,
    }
}
