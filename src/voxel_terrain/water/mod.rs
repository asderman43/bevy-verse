//! Water surface meshing: the top surface of the water standing in the
//! terrain, as its own mesh. The design is `docs/water-surface.md`; section
//! numbers below refer to it.
//!
//! Each sample has a solid value `s` (the terrain's own [`VoxelBuffer`]) and a
//! water value `w`, held in a second buffer of the same size and layout.
//!
//! [`VoxelBuffer`]: crate::voxel_terrain::isosurface::VoxelBuffer

pub mod mesher;
pub mod presets;
pub mod tables;

#[cfg(test)]
mod tests;

pub use mesher::extract;

/// The largest water value: a non-solid sample holds `0..=W_MAX`, and one unit
/// raises a settled surface by `1 / W_MAX` of a cell. The same scale as the
/// terrain's [`S_MAX`](crate::voxel_terrain::isosurface::S_MAX).
pub const W_MAX: u8 = 15;

/// The water isolevel: a non-solid sample counts as inside the water when
/// `w > W_ISO`. Sets where a column's water starts, `(W_ISO + 1) / W_MAX` of a
/// cell below the sample (§1.3).
pub const W_ISO: u8 = 7;

/// Water below this many units is treated as none at all, so films thinner
/// than `W_EPS / W_MAX` of a cell aren't drawn. At this scale one unit is
/// already 1/15 of a cell, so nothing is dropped.
pub const W_EPS: u8 = 0;

/// How far a shoreline point is pushed into the solid, along its face toward
/// the dry edge, in cells. The water polygon's edge is a straight chord, while
/// the true waterline can bend inside the cell; tucking the chord under the
/// terrain lets the depth test draw the real waterline (§3.3).
pub const SHORE_OVERLAP: f32 = 0.02;
