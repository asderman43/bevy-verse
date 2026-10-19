//! The top-surface mesher (`docs/water-surface.md` §3).
//!
//! Three passes, after flying edges:
//!
//! 1. **Rows.** Every X row of vertical edges becomes a `u64`, bit `x` set when
//!    the edge at `x` carries the surface. Each edge is decided once, not by
//!    each of the four cells around it, and a row with no water at all is
//!    skipped in one test. The rock is packed the same way, a bit per solid
//!    sample.
//! 2. **Count.** A cell's surface case `Q` is four bits out of the rows around
//!    it, so finding the cells with any surface, and counting their triangles
//!    and shoreline points, is bit operations; no samples are read.
//! 3. **Emit.** Only those cells are visited, into buffers allocated at the
//!    counted size.
//!
//! **Welded.** Every vertex is written once and shared by every triangle that
//! uses it, in every cell:
//!
//! - a surface vertex per vertical edge carrying the surface, numbered by its
//!   bit's rank in the rows: a per-row prefix count plus a popcount;
//! - a shoreline point per vertical face and wet edge on it, numbered by its
//!   key's rank among the keys the count pass collected.
//!
//! Each vertex's normal is the area-weighted sum of its triangles' face
//! normals, exactly +Y on flat water. Zero-area triangles -- two shoreline
//! points on the same terrain vertex -- are kept: dropping one could leave a
//! vertex nothing references, and they add nothing to the normals.

use bevy::math::{vec3, Vec3};

use super::{
    tables::{
        face_bits, Slot, CORNER_BODY, FACE_EDGES, FACE_EDGE_NEXT, FACE_EDGE_OWN, FACE_SEGS,
        FACE_SEGS_APART,
        MS_TRIS, RECIP, VERT_EDGE, VERT_T,
    },
    SHORE_OVERLAP, W_EPS, W_ISO, W_MAX,
};
use crate::voxel_terrain::isosurface::{
    tables::{EDGE_AXIS_ORIGIN, EDGE_VERTEX_INDICES, VERTEX_POSITIONS},
    terrain_vertex, Interpolate, Surface, VoxelBuffer, ISO, MAX_SIZE, S_MAX,
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

    let size = solid.size();
    if size < 2 {
        return Surface::default();
    }
    assert!(
        size <= MAX_SIZE,
        "a row of edges is packed into a u64, so a chunk can be at most {MAX_SIZE} samples per \
         axis, got {size}",
    );

    // Most chunks hold no water at all.
    if water.samples().iter().all(|&w| w == 0) {
        return Surface::default();
    }

    let rows = Rows::scan(solid, water, interpolate);
    let counts = rows.count();
    let vertices = rows.surface_vertices + counts.shore_keys.len();

    let mut out = Out {
        positions: vec![Vec3::NAN; vertices],
        normals: vec![Vec3::ZERO; vertices],
        indices: Vec::with_capacity(3 * counts.triangles),
        shore_written: vec![false; counts.shore_keys.len()],
    };
    let mesher = Mesher {
        solid,
        interpolate,
        overlap,
        rows: &rows,
        shore_keys: &counts.shore_keys,
    };
    let cells = size - 1;
    for z in 0..cells {
        for y in 0..cells {
            let mut row = rows.cells_with_surface(y, z);
            while row != 0 {
                let x = row.trailing_zeros() as usize;
                row &= row - 1;
                let corners = Corners::load(solid, water, (x, y, z));
                mesher.cell(&mut out, (x, y, z), &corners, rows.cell_q(x, y, z));
            }
        }
    }

    debug_assert_eq!(out.indices.len(), 3 * counts.triangles, "triangles built != counted");
    debug_assert!(
        out.positions.iter().all(|p| p.is_finite()),
        "a counted vertex was never written",
    );

    // Flat water is exactly up, rather than up to a rounding step.
    let normals = out
        .normals
        .into_iter()
        .map(|n| if n.x == 0.0 && n.z == 0.0 { Vec3::Y } else { n.normalize_or(Vec3::Y) })
        .collect();
    Surface {
        positions: out.positions,
        normals,
        indices: out.indices,
    }
}

/// What pass 2 finds.
pub(crate) struct Counts {
    /// Exactly the triangles emission builds.
    pub triangles: usize,
    /// Every shoreline point some cell needs, sorted and unique; a point's
    /// index among them is its vertex number after the surface vertices.
    pub shore_keys: Vec<u32>,
}

/// Passes 1 and 2: the rock and the surface edges as row bitmasks.
pub(crate) struct Rows {
    size: usize,
    /// Bit `x` of row `y + z·size`: sample `(x, y, z)` is solid.
    solid: Vec<u64>,
    /// Bit `x` of row `y + z·size`: the vertical edge from `(x, y, z)` up to
    /// `(x, y + 1, z)` carries the surface. The top row is unused.
    surface: Vec<u64>,
    /// The vertex number of each row's first surface vertex: the surface
    /// vertices of all rows before it.
    first_vertex: Vec<u32>,
    /// Surface vertices in all: one per set bit in `surface`.
    pub surface_vertices: usize,
}

impl Rows {
    pub fn scan(solid: &VoxelBuffer, water: &VoxelBuffer, interpolate: Interpolate) -> Self {
        let size = solid.size();
        let row = |y: usize, z: usize| y + z * size;

        let mut solid_rows = vec![0u64; size * size];
        for z in 0..size {
            for y in 0..size {
                solid_rows[row(y, z)] = solid
                    .row(y, z)
                    .iter()
                    .enumerate()
                    .fold(0, |m, (x, &s)| m | ((s > ISO) as u64) << x);
            }
        }

        let mut surface = vec![0u64; size * size];
        for z in 0..size {
            for y in 0..size - 1 {
                let (w_lo, w_hi) = (water.row(y, z), water.row(y + 1, z));
                // Every way an edge can carry the surface needs water at one
                // of its ends.
                if w_lo.iter().chain(w_hi).all(|&w| w == 0) {
                    continue;
                }
                let (s_lo, s_hi) = (solid.row(y, z), solid.row(y + 1, z));
                // The bottom row rests on ground assumed to be there.
                let rests = if y == 0 { u64::MAX } else { solid_rows[row(y - 1, z)] };

                let mut mask = 0;
                for x in 0..size {
                    let edge = Edge {
                        s_lo: s_lo[x],
                        w_lo: w_lo[x],
                        s_hi: s_hi[x],
                        w_hi: w_hi[x],
                        lo_rests: (rests >> x) & 1 == 1,
                    };
                    mask |= (edge.height(interpolate).is_some() as u64) << x;
                }
                surface[row(y, z)] = mask;
            }
        }

        let mut first_vertex = Vec::with_capacity(surface.len());
        let mut surface_vertices = 0;
        for &mask in &surface {
            first_vertex.push(surface_vertices as u32);
            surface_vertices += mask.count_ones() as usize;
        }

        Rows {
            size,
            solid: solid_rows,
            surface,
            first_vertex,
            surface_vertices,
        }
    }

    /// The vertex number of the surface vertex on the vertical edge up from
    /// sample `(x, y, z)`, which must carry the surface.
    fn surface_vertex(&self, x: usize, y: usize, z: usize) -> u32 {
        let row = y + z * self.size;
        let below = self.surface[row] & ((1u64 << x) - 1);
        self.first_vertex[row] + below.count_ones()
    }

    /// Cells of row `(y, z)` with any surface edge, bit per cell.
    fn cells_with_surface(&self, y: usize, z: usize) -> u64 {
        let (near, far) = (self.surface[y + z * self.size], self.surface[y + (z + 1) * self.size]);
        let cells = self.size - 1;
        let mask = if cells >= 64 { u64::MAX } else { (1 << cells) - 1 };
        (near | near >> 1 | far | far >> 1) & mask
    }

    /// The surface case `Q` of cell `(x, y, z)`, bit per slot in [`VERT_EDGE`]
    /// order: slots 0 and 1 stand at `z + 1`, slots 2 and 3 at `z`; slots 1
    /// and 2 at `x + 1`, slots 0 and 3 at `x`.
    fn cell_q(&self, x: usize, y: usize, z: usize) -> usize {
        let (near, far) = (self.surface[y + z * self.size], self.surface[y + (z + 1) * self.size]);
        let bit = |m: u64, x: usize| ((m >> x) & 1) as usize;
        bit(far, x) | bit(far, x + 1) << 1 | bit(near, x + 1) << 2 | bit(near, x) << 3
    }

    /// Cell `(x, y, z)`'s solid mask, bit per corner.
    fn cell_solid(&self, x: usize, y: usize, z: usize) -> u8 {
        (0..8).fold(0, |m, c| {
            let row = self.solid[(y + (c >> 1 & 1)) + (z + (c >> 2 & 1)) * self.size];
            m | (((row >> (x + (c & 1))) & 1) as u8) << c
        })
    }

    /// Pass 2: the triangles and shoreline points emission will build.
    pub fn count(&self) -> Counts {
        let cells = self.size - 1;
        let mut triangles = 0;
        let mut shore_keys = Vec::new();
        for z in 0..cells {
            for y in 0..cells {
                let mut row = self.cells_with_surface(y, z);
                while row != 0 {
                    let x = row.trailing_zeros() as usize;
                    row &= row - 1;
                    let q = self.cell_q(x, y, z);
                    let solid = if q.count_ones() == 1 { 0 } else { self.cell_solid(x, y, z) };
                    for group in bodies(q, solid) {
                        triangles += MS_TRIS[group].count;
                        for (f, k_wet) in shore_faces(group) {
                            shore_keys.push(shore_key(self.size, (x, y, z), f, k_wet));
                        }
                    }
                }
            }
        }
        // Two cells share each face; both asked for its point.
        shore_keys.sort_unstable();
        shore_keys.dedup();
        Counts {
            triangles,
            shore_keys,
        }
    }
}

/// The faces of a group of surface slots that get a shoreline point, with the
/// group's slot on each: the faces with exactly one of their two slots in it.
fn shore_faces(q: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..4).filter_map(move |f| {
        let next = (f + 1) % 4;
        match ((q >> f) & 1, (q >> next) & 1) {
            (1, 0) => Some((f, f)),
            (0, 1) => Some((f, next)),
            _ => None,
        }
    })
}

/// Which of a vertical face's two vertical edges is slot `k_wet` of face `f`:
/// 0 for the one at the face's lower x or z, 1 for the other. Face 0 holds
/// slots 0 and 1 along x, face 1 slots 2 and 1 along z, face 2 slots 3 and 2
/// along x, face 3 slots 3 and 0 along z.
const SIDE: [[u32; 2]; 4] = [[0, 1], [1, 0], [1, 0], [0, 1]];

/// The shoreline point of face `f` of `cell`, at the water level of its slot
/// `k_wet`, as a key both cells sharing the face compute alike: the face's
/// lowest sample, whether it spans x or z, and which of its vertical edges is
/// wet. Faces 0 and 2 are the cell's z + 1 and z faces, spanning x; faces 1
/// and 3 its x + 1 and x faces, spanning z.
fn shore_key(size: usize, (x, y, z): (usize, usize, usize), f: usize, k_wet: usize) -> u32 {
    let (base, spans_z) = match f {
        0 => ((x, z + 1), 0),
        1 => ((x + 1, z), 1),
        2 => ((x, z), 0),
        _ => ((x, z), 1),
    };
    let side = SIDE[f][(k_wet != f) as usize];
    let sample = base.0 + y * size + base.1 * size * size;
    ((sample * 2 + spans_z) * 2) as u32 + side
}

/// Splits the slots in `q` into groups that carry the same body of water
/// (§3.4). Water in separate bodies of open space -- a pool on top and a pocket
/// walled off under a rock -- stands at its own level, so each group is meshed
/// as its own polygon. A slot carries the water of its upper corner, or of its
/// lower one under a ledge.
fn bodies(q: usize, solid: u8) -> impl Iterator<Item = usize> {
    let corner_body = &CORNER_BODY[solid as usize];
    let body = move |k: usize| {
        let (lo, hi) = VERT_EDGE[k];
        if bit(solid, hi) {
            corner_body[lo]
        } else {
            corner_body[hi]
        }
    };
    let mut remaining = q;
    std::iter::from_fn(move || {
        if remaining == 0 {
            return None;
        }
        let first = remaining.trailing_zeros() as usize;
        let group = (0..4)
            .filter(|&k| (remaining >> k) & 1 == 1 && body(k) == body(first))
            .fold(0, |g, k| g | 1 << k);
        remaining &= !group;
        Some(group)
    })
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
    /// Corners resting on a solid sample below, or on the buffer's bottom.
    pub rests: u8,
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
        let (mut solid_mask, mut inside, mut wet, mut rests) = (0, 0, 0, 0);
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
            b[c] = bottom_fill(s[c], w[c], below_solid[c], ledge);
            solid_mask |= (is_solid as u8) << c;
            inside |= ((b[c] > W_ISO) as u8) << c;
            wet |= ((w[c] > 0 && !ledge) as u8) << c;
            rests |= (below_solid[c] as u8) << c;
        }

        debug_assert_eq!(solid_mask & !inside, 0, "a solid corner is always inside");
        Corners {
            s,
            w,
            b,
            solid: solid_mask,
            inside,
            wet,
            rests,
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
    Edge {
        s_lo: c.s[lo],
        w_lo: c.w[lo],
        s_hi: c.s[hi],
        w_hi: c.w[hi],
        lo_rests: bit(c.rests, lo),
    }
    .height(interpolate)
}

/// One vertical edge's two samples, and whether the lower one rests on solid
/// rock. Everything that decides whether the edge carries the surface; the
/// row pass and the cells both go through [`Edge::height`], so they agree.
pub(crate) struct Edge {
    pub s_lo: u8,
    pub w_lo: u8,
    pub s_hi: u8,
    pub w_hi: u8,
    pub lo_rests: bool,
}

impl Edge {
    /// The surface height up the edge, from its lower corner, or `None` if it
    /// doesn't carry the surface (§3.2).
    pub fn height(&self, interpolate: Interpolate) -> Option<f32> {
        let eps = |w: u8| if w < W_EPS { 0 } else { w };
        let (w_lo, w_hi) = (eps(self.w_lo), eps(self.w_hi));
        let (lo_solid, hi_solid) = (self.s_lo > ISO, self.s_hi > ISO);
        // The upper sample rests on the lower one.
        let (lo_ledge, hi_ledge) = (lo_solid && !self.lo_rests, hi_solid && !lo_solid);
        let b_lo = bottom_fill(self.s_lo, w_lo, self.lo_rests, lo_ledge);
        let b_hi = bottom_fill(self.s_hi, w_hi, lo_solid, hi_ledge);
        let (wet_lo, wet_hi) = (w_lo > 0 && !lo_ledge, w_hi > 0 && !hi_ledge);
        let (in_lo, in_hi) = (b_lo > W_ISO, b_hi > W_ISO);

        // Open water, or water resting on the terrain: the bottom fill crosses
        // the isolevel on this edge. With no water this is exactly where the
        // terrain draws the rock (`edge_t`), so the water rests on it.
        if in_lo && !in_hi && (wet_lo || wet_hi) {
            return Some(VERT_T[(b_lo + b_hi) as usize]);
        }

        // Water under a ledge: upper sample solid, lower one not solid but
        // inside. The ledge's water lies under its rock, at the bottom of its
        // slab.
        if hi_solid && !lo_solid && in_lo && (wet_lo || w_hi > 0) {
            let n = b_lo + w_hi;
            if below_ledge(n, self.s_lo, self.s_hi, interpolate) {
                return Some(VERT_T[n as usize]);
            }
        }

        None
    }
}

/// How much of a sample's slab is filled from its bottom (§1.3): rock
/// resting on solid below and the water on it; a ledge's rock alone, its
/// water being underneath; or the water alone, when the rock is held from the
/// side or above.
fn bottom_fill(s: u8, w: u8, rests: bool, ledge: bool) -> u8 {
    match (rests, ledge) {
        (true, _) => s + w,
        (false, true) => s,
        (false, false) => w,
    }
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

/// The output as it is built: positions written once at their counted
/// numbers, normals summed from the triangles around each vertex.
struct Out {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    indices: Vec<u32>,
    shore_written: Vec<bool>,
}

struct Mesher<'a> {
    solid: &'a VoxelBuffer,
    interpolate: Interpolate,
    overlap: f32,
    rows: &'a Rows,
    shore_keys: &'a [u32],
}

impl Mesher<'_> {
    /// Meshes one cell whose surface case, from the row pass, is `q`.
    fn cell(&self, out: &mut Out, cell: (usize, usize, usize), c: &Corners, q: usize) {
        let (x, y, z) = cell;

        // 1. The surface vertex on each slot in `q`. Every cell around the
        // edge writes the same bits to the same number.
        let mut v = [Vec3::ZERO; 4];
        let mut v_index = [0u32; 4];
        for k in 0..4 {
            if (q >> k) & 1 == 1 {
                let t = slot_height(c, k, self.interpolate);
                debug_assert!(t.is_some(), "the row pass and the cell disagree on slot {k}");
                // Integer corner plus `t`: every cell sharing the edge adds the
                // same two numbers, so they agree bit for bit.
                let base = corner(cell, VERT_EDGE[k].0);
                v[k] = vec3(base.x, base.y + t.unwrap_or(0.0), base.z);
                let at = VERTEX_POSITIONS[VERT_EDGE[k].0];
                v_index[k] = self.rows.surface_vertex(x + at.x as usize, y, z + at.z as usize);
                out.positions[v_index[k] as usize] = v[k];
            }
        }

        for group in bodies(q, c.solid) {
            self.polygon(out, cell, c, group, &v, &v_index);
        }
    }

    /// The surface over the slots in `q`, which all carry one body of water.
    fn polygon(
        &self,
        out: &mut Out,
        cell: (usize, usize, usize),
        c: &Corners,
        q: usize,
        v: &[Vec3; 4],
        v_index: &[u32; 4],
    ) {
        // 2. A shoreline point on each face with exactly one surface edge,
        // computed by the first cell to need it.
        let mut f_index = [0u32; 4];
        for (f, k_wet) in shore_faces(q) {
            let key = shore_key(self.rows.size, cell, f, k_wet);
            let rank = self
                .shore_keys
                .binary_search(&key)
                .expect("the count pass collected every shoreline point");
            if !out.shore_written[rank] {
                out.shore_written[rank] = true;
                let at = self.rows.surface_vertices + rank;
                out.positions[at] = self.shore_point(cell, c, f, k_wet, v);
            }
            f_index[f] = (self.rows.surface_vertices + rank) as u32;
        }

        // 3. Triangles, and their face normals onto their corners, weighted by
        // area: the cross product's length is twice it.
        let case = MS_TRIS[q];
        for tri in &case.tris[..case.count] {
            let corners = tri.map(|slot| match slot {
                Slot::V(k) => v_index[k],
                Slot::F(f) => f_index[f],
            });
            let [a, b, c] = corners.map(|i| out.positions[i as usize]);
            let normal = (b - a).cross(c - a);
            for i in corners {
                out.normals[i as usize] += normal;
            }
            out.indices.extend(corners);
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
/// leaves the wet edge crosses the face's next edge. On a diagonal face whose
/// inside corners are water and rock, the contour keeps them apart. It is placed the way the
/// terrain places its own vertices, on the bottom fill instead of the rock: by
/// the volume formula on a vertical edge, linearly to 7.5 on a horizontal one.
/// It depends only on that edge's two corners, so the cell across a level step
/// computes the same point.
///
/// `None` when the wet edge is not on the occupancy contour at all, which is
/// a surface edge kept by the ledge rule.
fn occupancy_crossing(cell: (usize, usize, usize), c: &Corners, f: usize, k_wet: usize) -> Option<Vec3> {
    let wet_edge = if k_wet == f { FACE_EDGE_OWN } else { FACE_EDGE_NEXT };
    // On a diagonal face whose two inside corners are water and rock, keep
    // them apart: the water's contour runs to its own neighbours, not across
    // to the rock. Decided from the face alone, so the cell across it agrees.
    let inside = face_bits(c.inside, f);
    let solid = face_bits(c.solid, f);
    let mixed = (inside == 0b0101 || inside == 0b1010) && solid != 0 && solid != inside;
    let segs = if mixed { FACE_SEGS_APART[inside] } else { FACE_SEGS[inside] };
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
