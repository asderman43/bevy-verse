//! The top-surface mesher, cell by cell (`docs/water-surface.md` §3).
//!
//! Unwelded, like `marching_cubes.rs`: three fresh vertices per triangle, all
//! carrying the triangle's face normal; zero-area triangles are dropped. Output goes into plain `Vec`s; the
//! surface is a thin band of cells, so the terrain's exact-count scan isn't
//! worth its machinery here yet.

use bevy::math::{vec3, Vec3};

use super::{
    tables::{
        face_bits, Slot, FACE_EDGES, FACE_EDGE_NEXT, FACE_EDGE_OWN, FACE_SEGS,
        MS_TRIS, RECIP, VERT_EDGE, VERT_T,
    },
    SHORE_OVERLAP, W_EPS, W_ISO, W_MAX,
};
use crate::voxel_terrain::isosurface::{
    face_normal,
    tables::{EDGE_AXIS_ORIGIN, EDGE_VERTEX_INDICES, VERTEX_POSITIONS},
    terrain_vertex, Interpolate, Surface, VoxelBuffer, ISO, S_MAX,
};

/// Meshes the top surface of `water` standing in `solid`, with the default
/// [`SHORE_OVERLAP`].
///
/// `interpolate` must match the terrain's, so the shoreline lands on the
/// terrain mesh as it is drawn.
///
/// # Panics
/// If the two buffers differ in size. In debug builds, also if a solid sample
/// exceeds `S_MAX` or a water sample exceeds [`W_MAX`].
pub fn extract(solid: &VoxelBuffer, water: &VoxelBuffer, interpolate: Interpolate) -> Surface {
    extract_with_overlap(solid, water, interpolate, SHORE_OVERLAP)
}

/// [`extract`] with an explicit shore overlap, so the shoreline can be checked
/// against the terrain with it switched off.
pub fn extract_with_overlap(
    solid: &VoxelBuffer,
    water: &VoxelBuffer,
    interpolate: Interpolate,
    overlap: f32,
) -> Surface {
    assert_eq!(
        solid.size(),
        water.size(),
        "the solid and water buffers must be the same size",
    );
    debug_assert!(solid.samples().iter().all(|&s| s <= S_MAX), "solid samples run 0..={S_MAX}");
    debug_assert!(water.samples().iter().all(|&w| w <= W_MAX), "water samples run 0..={W_MAX}");

    let mut surface = Surface::default();
    if solid.size() < 2 {
        return surface;
    }

    let mesher = Mesher {
        solid,
        interpolate,
        overlap,
    };
    let cells = solid.cells_per_axis();
    for z in 0..cells {
        for y in 0..cells {
            for x in 0..cells {
                let corners = Corners::load(solid, water, (x, y, z));
                if corners.wet != 0 {
                    mesher.cell(&mut surface, (x, y, z), &corners);
                }
            }
        }
    }

    surface
}

/// What the mesher sees in one cell, for debugging and the demo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellInfo {
    /// Each corner's bottom fill.
    pub bottom_fill: [u8; 8],
    /// `S`, `O` and the wet mask, bit per corner.
    pub solid: u8,
    pub inside: u8,
    pub wet: u8,
    /// `Q`: which slots carry the surface, bit per slot.
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
    let surface_slots = (0..4)
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

/// One cell's corner values and masks (§3.1).
pub(crate) struct Corners {
    pub s: [u8; 8],
    pub w: [u8; 8],
    /// Bottom fill: how much of the corner's slab is filled from its bottom,
    /// by rock and water together (§1.2). The inside test and the height
    /// formula both read it.
    pub b: [u8; 8],
    /// `S`: corners with `s > ISO`.
    pub solid: u8,
    /// `O`: corners with `b > W_ISO`.
    pub inside: u8,
    /// Corners whose water lies on this cell's vertical edges from below: all
    /// of them except a ledge, whose water is under its rock, in the edge below.
    pub wet: u8,
}

impl Corners {
    /// # Panics
    /// In debug builds, if a corner holds more than `S_MAX` of rock and water.
    pub fn load(solid: &VoxelBuffer, water: &VoxelBuffer, (x, y, z): (usize, usize, usize)) -> Self {
        let s = solid.cell_corners(x, y, z);
        let mut w = water.cell_corners(x, y, z);

        // What each corner rests on. The upper corners rest on the lower ones;
        // the lower ones on the row below the cell, or, at the bottom of the
        // buffer, on ground assumed to be there.
        let mut below_solid = [true; 8];
        for c in 0..8 {
            below_solid[c] = if c & 2 != 0 {
                s[c & !2] > ISO
            } else if y == 0 {
                true
            } else {
                solid.get(x + (c & 1), y - 1, z + (c >> 2 & 1)) > ISO
            };
        }

        let mut b = [0; 8];
        let (mut solid_mask, mut inside, mut wet) = (0, 0, 0);
        for c in 0..8 {
            debug_assert!(
                s[c] as u16 + w[c] as u16 <= S_MAX as u16,
                "a sample holds at most {S_MAX} of rock and water, got s {} + w {}",
                s[c],
                w[c],
            );
            if w[c] < W_EPS {
                w[c] = 0;
            }
            let is_solid = s[c] > ISO;
            let ledge = is_solid && !below_solid[c];
            b[c] = match (below_solid[c], ledge) {
                // Rock resting on rock, water on top of it.
                (true, _) => s[c] + w[c],
                // A ledge: rock hangs at the top, its water is underneath.
                (false, true) => s[c],
                // Rock held from above or the side doesn't carry water.
                (false, false) => w[c],
            };
            solid_mask |= (is_solid as u8) << c;
            inside |= ((b[c] > W_ISO) as u8) << c;
            wet |= ((w[c] > 0 && !ledge) as u8) << c;
        }

        debug_assert_eq!(solid_mask & !inside, 0, "a solid corner is always inside");
        Corners {
            s,
            w,
            b,
            solid: solid_mask,
            inside,
            wet,
        }
    }
}

fn bit(mask: u8, i: usize) -> bool {
    (mask >> i) & 1 == 1
}

/// The surface height up slot `k`'s vertical edge, from its lower corner, or
/// `None` if the edge doesn't carry the surface (§3.2).
pub(crate) fn slot_height(c: &Corners, k: usize, interpolate: Interpolate) -> Option<f32> {
    let (lo, hi) = VERT_EDGE[k];

    // Open water, or water resting on the terrain: the bottom fill crosses
    // the isolevel on this edge. With no water this is exactly where the
    // terrain draws the rock (`edge_t`), so the water rests on it.
    let crosses = bit(c.inside, lo) && !bit(c.inside, hi);
    if crosses && (bit(c.wet, lo) || bit(c.wet, hi)) {
        return Some(VERT_T[(c.b[lo] + c.b[hi]) as usize]);
    }

    // Water under a ledge: upper corner solid, lower corner not solid but
    // inside. The ledge's water lies under its rock, at the bottom of its slab.
    let ledge = bit(c.solid, hi) && !bit(c.solid, lo) && bit(c.inside, lo);
    if ledge && (bit(c.wet, lo) || c.w[hi] > 0) {
        let n = c.b[lo] + c.w[hi];
        if below_ledge(n, c.s[lo], c.s[hi], interpolate) {
            return Some(VERT_T[n as usize]);
        }
    }

    None
}

/// Whether water filled to `n` from the edge's bottom, `(n − 7.5) / W_MAX` up
/// it, stays below the ledge the terrain draws there, decided exactly in
/// integers.
///
/// The terrain hangs the ledge `(s_hi + s_lo − 7.5) / S_MAX` down from the
/// upper corner (`edge_t`), so the water is below it exactly when
/// `n + s_hi + s_lo < 2·S_MAX`. Without interpolation the ledge is at the
/// midpoint.
pub(crate) fn below_ledge(n: u8, s_lo: u8, s_hi: u8, interpolate: Interpolate) -> bool {
    if interpolate {
        (n as u16 + s_hi as u16 + s_lo as u16) < 2 * S_MAX as u16
    } else {
        // (n − 7.5) / W_MAX < 1/2.
        n < W_MAX
    }
}

struct Mesher<'a> {
    solid: &'a VoxelBuffer,
    interpolate: Interpolate,
    overlap: f32,
}

impl Mesher<'_> {
    fn cell(&self, out: &mut Surface, cell: (usize, usize, usize), c: &Corners) {
        // 1. Vertical edges carrying the surface.
        let mut q = 0usize;
        let mut v = [Vec3::ZERO; 4];
        for k in 0..4 {
            if let Some(t) = slot_height(c, k, self.interpolate) {
                q |= 1 << k;
                // Integer corner plus `t`: every cell sharing the edge adds the
                // same two numbers, so they agree bit for bit.
                let base = corner(cell, VERT_EDGE[k].0);
                v[k] = vec3(base.x, base.y + t, base.z);
            }
        }
        if q == 0 {
            return;
        }

        // 2. A shoreline point on each face with exactly one surface edge.
        let mut f_points = [Vec3::ZERO; 4];
        for f in 0..4 {
            let next = (f + 1) % 4;
            let (a, b) = ((q >> f) & 1, (q >> next) & 1);
            if a != b {
                let k_wet = if a == 1 { f } else { next };
                f_points[f] = self.shore_point(cell, c, f, k_wet, &v);
            }
        }

        // 3. Triangles, each with its own face normal. Two shoreline points
        // can land on the same terrain vertex -- the level exactly at a dry
        // corner's drawn ground -- and the triangle between them has no area;
        // it is skipped.
        let case = MS_TRIS[q];
        for tri in &case.tris[..case.count] {
            let [p0, p1, p2] = tri.map(|slot| match slot {
                Slot::V(k) => v[k],
                Slot::F(f) => f_points[f],
            });
            let Some(normal) = face_normal(p0, p1, p2) else {
                continue;
            };
            let first = out.positions.len() as u32;
            out.positions.extend([p0, p1, p2]);
            out.normals.extend([normal; 3]);
            out.indices.extend([first, first + 1, first + 2]);
        }
    }

    /// Where the water at slot `k_wet`'s height meets the shore on face `f`
    /// (§3.3).
    fn shore_point(
        &self,
        cell: (usize, usize, usize),
        c: &Corners,
        f: usize,
        k_wet: usize,
        v: &[Vec3; 4],
    ) -> Vec3 {
        let k_dry = if k_wet == f { (f + 1) % 4 } else { f };
        let wet_base = corner(cell, VERT_EDGE[k_wet].0);
        let dry_base = corner(cell, VERT_EDGE[k_dry].0);
        let h = v[k_wet].y;

        match self.terrain_crossing(cell, c, f, h, wet_base) {
            Some(p) => {
                // Tuck the point into the solid along the face, toward the dry
                // edge, but never past it.
                let toward_dry = dry_base - wet_base;
                let room = (dry_base - p).dot(toward_dry).max(0.0);
                p + toward_dry * self.overlap.min(room)
            }
            None => occupancy_crossing(cell, c, f, k_wet).unwrap_or(v[k_wet]),
        }
    }

    /// The point at height `h` on the terrain's contour across face `f`: the
    /// segment that brackets `h`, nearest the wet edge if two do.
    fn terrain_crossing(
        &self,
        cell: (usize, usize, usize),
        c: &Corners,
        f: usize,
        h: f32,
        wet_base: Vec3,
    ) -> Option<Vec3> {
        let segs = FACE_SEGS[face_bits(c.solid, f)];
        let mut best: Option<(f32, Vec3)> = None;

        for &(ea, eb) in &segs.segs[..segs.count] {
            let mut a = self.terrain_vertex(cell, FACE_EDGES[f][ea]);
            let mut b = self.terrain_vertex(cell, FACE_EDGES[f][eb]);
            if (a.y - h) * (b.y - h) > 0.0 || a.y == b.y {
                continue;
            }

            // Interpolate in a fixed order, so the cell across the face
            // computes the same point.
            if b.to_array() < a.to_array() {
                std::mem::swap(&mut a, &mut b);
            }
            let p = if h == a.y {
                a
            } else if h == b.y {
                b
            } else {
                let p = a + (b - a) * ((h - a.y) / (b.y - a.y));
                vec3(p.x, h, p.z)
            };

            let distance = (p - wet_base).with_y(0.0).length();
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, p));
            }
        }

        best.map(|(_, p)| p)
    }

    /// The terrain vertex on one of this cell's table edges, evaluated the way
    /// flying edges evaluates it.
    fn terrain_vertex(&self, (x, y, z): (usize, usize, usize), edge: usize) -> Vec3 {
        let (axis, ox, oy, oz) = EDGE_AXIS_ORIGIN[edge];
        terrain_vertex(self.solid, (x + ox, y + oy, z + oz), axis, self.interpolate)
    }
}

/// The fallback shoreline point (§3.3): where the occupancy contour that
/// leaves the wet edge crosses the face's next edge. It is placed the way the
/// terrain places its own vertices, on the bottom fill instead of the rock: by
/// the volume formula on a vertical edge, linearly to 7.5 on a horizontal one.
/// It depends only on that edge's two corners, so the cell across a level step
/// computes the same point.
///
/// `None` when the wet edge is not on the occupancy contour at all, which is
/// a surface edge kept by the ledge rule.
fn occupancy_crossing(cell: (usize, usize, usize), c: &Corners, f: usize, k_wet: usize) -> Option<Vec3> {
    let wet_edge = if k_wet == f { FACE_EDGE_OWN } else { FACE_EDGE_NEXT };
    let segs = FACE_SEGS[face_bits(c.inside, f)];
    let other = segs.segs[..segs.count].iter().find_map(|&(a, b)| match () {
        _ if a == wet_edge => Some(b),
        _ if b == wet_edge => Some(a),
        _ => None,
    })?;

    let (p, q) = EDGE_VERTEX_INDICES[FACE_EDGES[f][other]];
    let (i, u) = if bit(c.inside, p) { (p, q) } else { (q, p) };
    let (from, to) = (corner(cell, i), corner(cell, u));
    let t = if from.y != to.y {
        (c.b[i] as f32 + c.b[u] as f32 - (W_ISO as f32 + 0.5)) / W_MAX as f32
    } else {
        (c.b[i] as f32 - (W_ISO as f32 + 0.5)) * RECIP[(c.b[i] - c.b[u]) as usize]
    };
    Some(from + (to - from) * t)
}

/// A cell corner's absolute position. Integers, so exact.
fn corner((x, y, z): (usize, usize, usize), c: usize) -> Vec3 {
    vec3(x as f32, y as f32, z as f32) + VERTEX_POSITIONS[c]
}
