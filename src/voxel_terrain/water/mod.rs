//! Water surface meshing: the top surface of the water standing in the
//! terrain, as its own mesh. The design is `docs/water-surface.md`; section
//! numbers below refer to it.
//!
//! Each sample has a solid value `s` (the terrain's own [`VoxelBuffer`]) and a
//! water value `w`, held in a second buffer of the same size and layout.
//!
//! [`VoxelBuffer`]: crate::voxel_terrain::isosurface::VoxelBuffer

use crate::voxel_terrain::isosurface::{ISO, S_MAX};

pub mod mesher;
pub mod presets;
pub mod tables;

#[cfg(test)]
mod tests;

pub use mesher::extract;

/// The largest water value. Rock and water share each sample's slab, so
/// `s + w <= W_MAX`, and one unit raises a settled surface by `1 / W_MAX` of a
/// cell.
///
/// Rock and water are measured in the same units -- the water rests on the
/// terrain by adding to the same fill -- so this must equal the terrain's
/// [`S_MAX`].
pub const W_MAX: u8 = S_MAX;

/// The water isolevel: a corner is inside the water when its bottom fill is
/// `> W_ISO`. Equal to the terrain's [`ISO`] for the same reason as
/// [`W_MAX`].
pub const W_ISO: u8 = ISO;

/// Water below this many units is treated as none at all, so films thinner
/// than `W_EPS / W_MAX` of a cell aren't drawn. At this scale one unit is
/// already 1/15 of a cell, so nothing is dropped.
pub const W_EPS: u8 = 0;

/// How far a shoreline point is pushed into the solid, along its face toward
/// the dry edge, in cells. The water polygon's edge is a straight chord, while
/// the true waterline can bend inside the cell; tucking the chord under the
/// terrain lets the depth test draw the real waterline (§3.3).
pub const SHORE_OVERLAP: f32 = 0.02;
