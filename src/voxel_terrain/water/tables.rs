//! Lookup tables for the water mesher, all generated at compile time from the
//! terrain's own [`VERTEX_POSITIONS`] and [`EDGE_VERTEX_INDICES`] so they cannot
//! drift from it. See `docs/water-surface.md` §2 and §4.
//!
//! # Slots and faces
//!
//! The four vertical edges of a cell are **slots** `k = 0..4`, counter-clockwise
//! seen from above, so a polygon listed in slot order faces up. Face `f` is the
//! vertical face between slot `f` and slot `f + 1 (mod 4)`.
//!
//! Each face has four corners and four edges, numbered around it:
//!
//! ```text
//!   c3 ──e2── c2        c0 = slot f's lower corner      e0 = bottom edge
//!   │          │        c1 = slot f+1's lower corner    e1 = slot f+1's vertical
//!   e3         e1       c2 = slot f+1's upper corner    e2 = top edge
//!   │          │        c3 = slot f's upper corner      e3 = slot f's vertical
//!   c0 ──e0── c1
//! ```
//!
//! so corner `ci` sits between face edges `e(i-1)` and `ei`.

use bevy::math::Vec3;

use super::{W_ISO, W_MAX};
use crate::voxel_terrain::isosurface::tables::{EDGE_VERTEX_INDICES, VERTEX_POSITIONS};

/// (lower, upper) corner of each slot's vertical edge.
pub const VERT_EDGE: [(usize, usize); 4] = [(4, 6), (5, 7), (1, 3), (0, 2)];

/// The face-local edge index of slot `f`'s own vertical edge on face `f`, and
/// of slot `f + 1`'s.
pub const FACE_EDGE_OWN: usize = 3;
pub const FACE_EDGE_NEXT: usize = 1;

/// Each face's four corners, in the face-local order of the module docs.
pub const FACE_CORNERS: [[usize; 4]; 4] = {
    let mut table = [[0; 4]; 4];
    let mut f = 0;
    while f < 4 {
        let (lo_a, hi_a) = VERT_EDGE[f];
        let (lo_b, hi_b) = VERT_EDGE[(f + 1) % 4];
        table[f] = [lo_a, lo_b, hi_b, hi_a];
        f += 1;
    }
    table
};

/// Each face's four edges, as terrain table edges, in face-local order: edge
/// `ei` joins face corners `ci` and `c(i+1)`.
pub const FACE_EDGES: [[usize; 4]; 4] = {
    let mut table = [[0; 4]; 4];
    let mut f = 0;
    while f < 4 {
        let mut i = 0;
        while i < 4 {
            table[f][i] = table_edge(FACE_CORNERS[f][i], FACE_CORNERS[f][(i + 1) % 4]);
            i += 1;
        }
        f += 1;
    }
    table
};

/// The terrain table edge joining two corners, in either direction.
const fn table_edge(a: usize, b: usize) -> usize {
    let mut edge = 0;
    while edge < 12 {
        let (p, q) = EDGE_VERTEX_INDICES[edge];
        if (p == a && q == b) || (p == b && q == a) {
            return edge;
        }
        edge += 1;
    }
    panic!("corners are not joined by a cube edge");
}

/// Contour segments on one face, as pairs of face-local edges, for a 4-bit
/// inside mask over the face's corners (bit `i` for corner `ci`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceSegs {
    pub count: usize,
    pub segs: [(usize, usize); 2],
}

/// [`FaceSegs`] for every face mask.
///
/// An edge is cut when its two corners disagree. Two cut edges make one
/// segment. Four -- the diagonal masks `0101` and `1010` -- make two, and they
/// resolve the ambiguity the way the terrain's triangle table does: each
/// segment cuts off one **outside** corner, so the inside corners join across
/// the diagonal (`docs/transvoxel-face-ambiguity.md`). Anything else would pair
/// edges differently from the terrain mesh.
pub const FACE_SEGS: [FaceSegs; 16] = {
    let mut table = [FaceSegs {
        count: 0,
        segs: [(0, 0); 2],
    }; 16];

    let mut mask = 0;
    while mask < 16 {
        if mask == 0b0101 || mask == 0b1010 {
            // Corner `ci` sits between edges e(i-1) and ei.
            let mut n = 0;
            let mut i = 0;
            while i < 4 {
                if (mask >> i) & 1 == 0 {
                    table[mask].segs[n] = ((i + 3) % 4, i);
                    n += 1;
                }
                i += 1;
            }
            table[mask].count = 2;
        } else {
            let mut cut = [0; 2];
            let mut n = 0;
            let mut e = 0;
            while e < 4 {
                if (mask >> e) & 1 != (mask >> ((e + 1) % 4)) & 1 {
                    cut[n] = e;
                    n += 1;
                }
                e += 1;
            }
            if n == 2 {
                table[mask].segs[0] = (cut[0], cut[1]);
                table[mask].count = 1;
            }
        }

        mask += 1;
    }

    table
};

/// [`FACE_SEGS`] with the diagonal cases resolved the other way: each segment
/// cuts off an **inside** corner, so the two inside corners stay apart.
///
/// For the occupancy fallback (§3.3), where the inside corners on a diagonal
/// face are water on one side and rock on the other. Water joins water, not
/// rock, so its contour runs to its own outside neighbours rather than across
/// to the rock. Identical to [`FACE_SEGS`] everywhere but the two diagonals.
pub const FACE_SEGS_APART: [FaceSegs; 16] = {
    let mut table = FACE_SEGS;
    let mut mask = 0;
    while mask < 16 {
        if mask == 0b0101 || mask == 0b1010 {
            let mut n = 0;
            let mut i = 0;
            while i < 4 {
                if (mask >> i) & 1 == 1 {
                    table[mask].segs[n] = ((i + 3) % 4, i);
                    n += 1;
                }
                i += 1;
            }
        }
        mask += 1;
    }
    table
};

/// The face-local inside mask of face `f`, from a cell's 8-bit corner mask.
pub const fn face_bits(cell_mask: u8, f: usize) -> usize {
    let mut bits = 0;
    let mut i = 0;
    while i < 4 {
        bits |= (((cell_mask >> FACE_CORNERS[f][i]) & 1) as usize) << i;
        i += 1;
    }
    bits
}

/// Water height up a vertical edge from its lower corner, indexed by the two
/// corners' bottom fill `b_lo + b_hi` (§3.2): `(n − 7.5) / W_MAX`, the same
/// volume formula the terrain uses on vertical edges.
///
/// Entries below 8 are negative and never read; the top entries exceed 1 and
/// are only read by the ledge rule, which drops them.
pub const VERT_T: [f32; 2 * W_MAX as usize + 1] = {
    let mut table = [0.0; 2 * W_MAX as usize + 1];
    let mut n = 0;
    while n < table.len() {
        table[n] = (n as f32 - (W_ISO as f32 + 0.5)) / W_MAX as f32;
        n += 1;
    }
    table
};

/// `1 / d` for a difference of water values; entry 0 is unused. Only the
/// shoreline fallback divides.
pub const RECIP: [f32; W_MAX as usize + 1] = {
    let mut table = [0.0; W_MAX as usize + 1];
    let mut d = 1;
    while d < table.len() {
        table[d] = 1.0 / d as f32;
        d += 1;
    }
    table
};

/// For each solid mask `S`, which body of open space each corner is in: the
/// smallest corner index it connects to through cube edges with both ends not
/// solid, or 8 for a solid corner.
///
/// Edges only, never face or body diagonals, which is how the terrain's
/// triangle table resolves ambiguity: on a diagonal face the solid corners
/// join, so the open ones are apart. Water in two bodies can stand at two
/// levels in one cell, and each is meshed on its own.
pub const CORNER_BODY: [[u8; 8]; 256] = {
    let mut table = [[8u8; 8]; 256];
    let mut mask = 0;
    while mask < 256 {
        let mut body = [8u8; 8];
        let mut c = 0;
        while c < 8 {
            if (mask >> c) & 1 == 0 {
                body[c] = c as u8;
            }
            c += 1;
        }
        // Relax along open edges until nothing changes; eight rounds cover
        // the longest path through a cube.
        let mut round = 0;
        while round < 8 {
            let mut edge = 0;
            while edge < 12 {
                let (a, b) = EDGE_VERTEX_INDICES[edge];
                if body[a] < 8 && body[b] < 8 {
                    let m = if body[a] < body[b] { body[a] } else { body[b] };
                    body[a] = m;
                    body[b] = m;
                }
                edge += 1;
            }
            round += 1;
        }
        table[mask] = body;
        mask += 1;
    }
    table
};

/// A polygon corner: a surface vertex on slot `k` or a shoreline point on
/// face `f`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    V(usize),
    F(usize),
}

/// The triangles of one surface case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tris {
    pub count: usize,
    pub tris: [[Slot; 3]; 3],
}

/// Triangles for every 4-bit set `Q` of slots carrying the surface.
///
/// The boundary goes round the cell's square in slot order: `V_k` for each
/// surface slot, and `F_f` on each face with exactly one surface slot, where
/// the water meets the shore. Each run of consecutive surface slots is one
/// polygon, `F, V.., F` (or the four `V`s of open water), and is
/// fan-triangulated. Every corner lies on the square's boundary in boundary
/// order, so each polygon is convex in top view and the fan is valid.
///
/// The diagonal cases have two runs, so they give two corner triangles: water
/// does not bridge the cell centre.
pub const MS_TRIS: [Tris; 16] = {
    let empty = Tris {
        count: 0,
        tris: [[Slot::V(0); 3]; 3],
    };
    let mut table = [empty; 16];

    let mut q = 1;
    while q < 16 {
        let mut out = empty;
        if q == 0b1111 {
            out.tris[0] = [Slot::V(0), Slot::V(1), Slot::V(2)];
            out.tris[1] = [Slot::V(0), Slot::V(2), Slot::V(3)];
            out.count = 2;
        } else {
            // Each run starts at a surface slot whose predecessor is dry.
            let mut start = 0;
            while start < 4 {
                if (q >> start) & 1 == 1 && (q >> ((start + 3) % 4)) & 1 == 0 {
                    let mut poly = [Slot::V(0); 6];
                    let mut n = 0;
                    poly[n] = Slot::F((start + 3) % 4);
                    n += 1;
                    let mut k = start;
                    while (q >> (k % 4)) & 1 == 1 {
                        poly[n] = Slot::V(k % 4);
                        n += 1;
                        k += 1;
                    }
                    // Face (k - 1) lies between the run's last slot and the
                    // dry slot `k`.
                    poly[n] = Slot::F((k + 3) % 4);
                    n += 1;

                    let mut i = 1;
                    while i + 1 < n {
                        out.tris[out.count] = [poly[0], poly[i], poly[i + 1]];
                        out.count += 1;
                        i += 1;
                    }
                }
                start += 1;
            }
        }

        table[q] = out;
        q += 1;
    }

    table
};

/// Where slot `k`'s vertical edge sits in the cell, as its lower corner.
pub const fn slot_base(k: usize) -> Vec3 {
    VERTEX_POSITIONS[VERT_EDGE[k].0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel_terrain::isosurface::tables::TRIANGLE_TABLE;

    #[test]
    fn slots_are_vertical_and_faces_hold_their_slots() {
        for k in 0..4 {
            let (lo, hi) = VERT_EDGE[k];
            let (a, b) = (VERTEX_POSITIONS[lo], VERTEX_POSITIONS[hi]);
            assert_eq!(b - a, Vec3::Y, "slot {k} is not an upward vertical edge");
        }
        for f in 0..4 {
            let own = FACE_EDGES[f][FACE_EDGE_OWN];
            let next = FACE_EDGES[f][FACE_EDGE_NEXT];
            assert_eq!(own, table_edge(VERT_EDGE[f].0, VERT_EDGE[f].1));
            assert_eq!(next, table_edge(VERT_EDGE[(f + 1) % 4].0, VERT_EDGE[(f + 1) % 4].1));
            // Bottom and top edges are horizontal at the face's bottom and top.
            let y = |e: usize| {
                let (a, b) = EDGE_VERTEX_INDICES[e];
                (VERTEX_POSITIONS[a].y, VERTEX_POSITIONS[b].y)
            };
            assert_eq!(y(FACE_EDGES[f][0]), (0.0, 0.0), "face {f} bottom");
            assert_eq!(y(FACE_EDGES[f][2]), (1.0, 1.0), "face {f} top");
        }
    }

    /// Matches `docs/water-surface.md` §2.
    #[test]
    fn layout_matches_the_doc() {
        let slot_edges: Vec<usize> = (0..4).map(|f| FACE_EDGES[f][FACE_EDGE_OWN]).collect();
        assert_eq!(slot_edges, [7, 5, 1, 3]);
        let bottom_top: Vec<(usize, usize)> =
            (0..4).map(|f| (FACE_EDGES[f][0], FACE_EDGES[f][2])).collect();
        assert_eq!(bottom_top, [(4, 6), (9, 10), (0, 2), (8, 11)]);
    }

    /// For every cell case and vertical face, `FACE_SEGS` on the solid bits has
    /// to give exactly the segments the terrain mesh draws in that face: the
    /// triangle edges whose ends both lie on the face's edges, counted an odd
    /// number of times (an even count is interior to the in-face triangles).
    #[test]
    fn face_segs_match_the_terrain() {
        for solid in 0..=255u8 {
            // The terrain's case index sets a bit per *empty* corner.
            let tris = &TRIANGLE_TABLE[(!solid) as usize];
            let listed = tris.iter().take_while(|&&e| e != -1).count();

            for f in 0..4 {
                let on_face = |e: i8| FACE_EDGES[f].contains(&(e as usize));
                let mut seen: Vec<(usize, usize)> = Vec::new();
                for t in tris[..listed].chunks(3) {
                    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        if on_face(a) && on_face(b) {
                            let pair = ((a as usize).min(b as usize), (a as usize).max(b as usize));
                            if let Some(i) = seen.iter().position(|&p| p == pair) {
                                seen.remove(i);
                            } else {
                                seen.push(pair);
                            }
                        }
                    }
                }
                seen.sort();

                let segs = FACE_SEGS[face_bits(solid, f)];
                let mut ours: Vec<(usize, usize)> = segs.segs[..segs.count]
                    .iter()
                    .map(|&(a, b)| {
                        let (a, b) = (FACE_EDGES[f][a], FACE_EDGES[f][b]);
                        (a.min(b), a.max(b))
                    })
                    .collect();
                ours.sort();

                assert_eq!(ours, seen, "solid mask {solid:#010b}, face {f}");
            }
        }
    }

    #[test]
    fn vert_t_covers_the_surface_range() {
        // Lower corner inside, upper outside: the height lies strictly inside
        // the edge.
        for lo in W_ISO + 1..=W_MAX {
            for hi in 0..=W_ISO {
                let t = VERT_T[(lo + hi) as usize];
                assert!(t > 0.0 && t < 1.0, "b_lo {lo}, b_hi {hi}: t = {t}");
            }
        }
        // One unit is one step, exactly.
        for n in 1..VERT_T.len() {
            assert!(((VERT_T[n] - VERT_T[n - 1]) * W_MAX as f32 - 1.0).abs() < 1e-5);
        }
    }

    /// Each case's triangles are wound up (counter-clockwise from above) and
    /// convex in top view, wherever the shoreline points land on their faces.
    #[test]
    fn ms_tris_are_convex_and_face_up() {
        let mut state = 0x1234_5678u32;
        let mut random = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        };

        for q in 1..16 {
            let case = MS_TRIS[q];
            assert!(case.count > 0, "case {q:04b} has no triangles");
            for _ in 0..64 {
                // Top view: V at the slot's corner, F somewhere along its face.
                let at = |slot: Slot, r: &[f32; 4]| match slot {
                    Slot::V(k) => slot_base(k),
                    Slot::F(f) => slot_base(f).lerp(slot_base((f + 1) % 4), r[f]),
                };
                let r = [random(), random(), random(), random()];
                for tri in &case.tris[..case.count] {
                    let [a, b, c] = tri.map(|s| at(s, &r));
                    let n = (b - a).cross(c - a);
                    assert!(n.y >= 0.0, "case {q:04b}: triangle {tri:?} faces down");
                    assert!(n.x.abs() < 1e-6 && n.z.abs() < 1e-6, "flat in top view");
                }
            }
        }
    }

    /// A body is only ever open corners, joined along edges; the corner the
    /// terrain walls off on three sides is its own body.
    #[test]
    fn corner_bodies_follow_open_edges() {
        assert_eq!(CORNER_BODY[0], [0; 8]);
        assert_eq!(CORNER_BODY[255], [8; 8]);
        // Solid 0, 3, 4, 5: corner 1 is sealed off from 2, 6, 7.
        let solid = 0b0011_1001;
        assert_eq!(CORNER_BODY[solid], [8, 1, 2, 8, 8, 8, 2, 2]);
        // Only a face diagonal open (1 and 2 on z = 0, with 0 and 3 solid,
        // and the whole far face solid): two bodies, not one.
        let solid = 0b1111_1001;
        assert_eq!(CORNER_BODY[solid], [8, 1, 2, 8, 8, 8, 8, 8]);
    }

    #[test]
    fn face_segs_apart_differ_only_on_diagonals() {
        for mask in 0..16 {
            if mask == 0b0101 || mask == 0b1010 {
                assert_ne!(FACE_SEGS_APART[mask], FACE_SEGS[mask]);
                // Each segment cuts off an inside corner: corner i sits
                // between edges i - 1 and i.
                for &(a, b) in &FACE_SEGS_APART[mask].segs[..2] {
                    assert_eq!(a, (b + 3) % 4);
                    assert_eq!((mask >> b) & 1, 1, "mask {mask:04b}: cuts off outside corner {b}");
                }
            } else {
                assert_eq!(FACE_SEGS_APART[mask], FACE_SEGS[mask], "mask {mask:04b}");
            }
        }
    }

    #[test]
    fn diagonal_cases_do_not_bridge() {
        for q in [0b0101, 0b1010] {
            let case = MS_TRIS[q];
            assert_eq!(case.count, 2);
            for tri in &case.tris[..2] {
                let vs = tri.iter().filter(|s| matches!(s, Slot::V(_))).count();
                assert_eq!(vs, 1, "case {q:04b}: {tri:?} is not a corner triangle");
            }
        }
    }

    #[test]
    fn polygon_shapes_follow_the_doc() {
        // (case, triangle count): 1 slot -> triangle, 2 adjacent -> quad,
        // 3 -> pentagon, 4 -> quad.
        for (q, tris) in [(0b0001, 1), (0b0011, 2), (0b0111, 3), (0b1111, 2), (0b1001, 2)] {
            assert_eq!(MS_TRIS[q].count, tris, "case {q:04b}");
        }
    }
}
