//! Single-cell water cases, for the `water_cell` demo and the tests. Each is
//! one 2^3 cell: eight corners, each with a solid and a water value.
//!
//! The corner index is the terrain's (x bit 0, y bit 1, z bit 2), which is also
//! the sample index in a 2^3 buffer, so the arrays go straight into one.

use super::tables::VERT_EDGE;
use crate::voxel_terrain::isosurface::VoxelBuffer;

/// One cell's corner values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    /// What to look for.
    pub note: &'static str,
    pub s: [u8; 8],
    pub w: [u8; 8],
}

impl Preset {
    /// The cell as solid and water buffers.
    pub fn buffers(&self) -> (VoxelBuffer, VoxelBuffer) {
        (
            VoxelBuffer::from_samples(self.s.to_vec(), 2),
            VoxelBuffer::from_samples(self.w.to_vec(), 2),
        )
    }
}

/// Every preset, in demo order: the 16 surface cases first, then the worked
/// examples of `docs/water-surface.md` §7, then the special cases.
pub fn all() -> Vec<Preset> {
    let mut presets: Vec<Preset> = (0..16).map(surface_case).collect();
    presets.extend(examples());
    presets
}

/// Surface case `q`: the slots in `q` hold a pool at 0.6 on solid ground, and
/// the others are solid bank columns, so every shoreline lands on the bank.
pub fn surface_case(q: usize) -> Preset {
    let mut s = [15; 8];
    let mut w = [0; 8];
    for k in 0..4 {
        if q >> k & 1 == 1 {
            let (_, hi) = VERT_EDGE[k];
            s[hi] = 0;
            w[hi] = 2;
        }
    }
    let note = match q.count_ones() {
        0 => "no water: nothing drawn",
        1 => "triangle: V plus two shoreline points on the bank",
        2 if q == 0b0101 || q == 0b1010 => "diagonal: two separate corner triangles, no bridge",
        2 => "quad: two V, two shoreline points",
        3 => "pentagon: three V, two shoreline points",
        _ => "open water: one flat quad at 0.6",
    };
    Preset {
        name: format!("Q = {q:04b}"),
        note,
        s,
        w,
    }
}

fn examples() -> Vec<Preset> {
    let preset = |name: &str, note, s, w| Preset {
        name: name.to_string(),
        note,
        s,
        w,
    };
    // Corner order: 0 (0,0,0), 1 (1,0,0), 2 (0,1,0), 3 (1,1,0),
    //               4 (0,0,1), 5 (1,0,1), 6 (0,1,1), 7 (1,1,1).
    vec![
        preset(
            "A: open water",
            "walls s = 9 below, w = 5 above: flat quad at 12/15, terrain at 1/6",
            [9, 9, 0, 0, 9, 9, 0, 0],
            [0, 0, 5, 5, 0, 0, 5, 5],
        ),
        preset(
            "B: one unit short",
            "full below, w = 7 above: surface at 14/15, the top of this cell",
            [0; 8],
            [15, 15, 7, 7, 15, 15, 7, 7],
        ),
        preset(
            "B: just inside",
            "w = 8 above: the surface moves to the cell above, so nothing here",
            [0; 8],
            [15, 15, 8, 8, 15, 15, 8, 8],
        ),
        preset(
            "C: steep bank",
            "water at 13/15 meets a bank (s 15 / 12) 0.608 across the face",
            [0, 0, 0, 0, 15, 15, 12, 12],
            [15, 15, 6, 6, 0, 0, 0, 0],
        ),
        preset(
            "D: bank corner",
            "pentagon at 0.6; corner 4's column is a bank (s 15 / 6), shore 0.14 from it",
            [0, 0, 0, 0, 15, 0, 6, 0],
            [15, 15, 2, 2, 0, 15, 0, 2],
        ),
        preset(
            "E: rock through the surface",
            "pentagon at 0.6; the waterline climbs onto the rock at corner 3",
            [15, 15, 0, 15, 15, 15, 0, 0],
            [0, 0, 2, 0, 0, 0, 2, 2],
        ),
        preset(
            "E: unsettled",
            "corner 3 is water, not rock: no terrain brackets the level, fallback points",
            [15, 15, 0, 0, 15, 15, 0, 0],
            [0, 0, 2, 15, 0, 0, 2, 2],
        ),
        preset(
            "F: ledge dips in",
            "ledge s = 13 at corner 6 is drawn below the pool: its slot is left out",
            [0, 0, 0, 0, 0, 0, 13, 0],
            [15, 15, 2, 2, 15, 15, 2, 2],
        ),
        preset(
            "F: ledge above",
            "ledge s = 10 drawn at 0.75: flat quad at 0.6, continuing under it",
            [0, 0, 0, 0, 0, 0, 10, 0],
            [15, 15, 2, 2, 15, 15, 2, 2],
        ),
        preset(
            "F: thick ledge",
            "ledge s = 15 holds no water: the slot under it stays at 7/15",
            [0, 0, 0, 0, 0, 0, 15, 0],
            [15, 15, 2, 2, 15, 15, 0, 2],
        ),
        preset(
            "fallback: open air",
            "pool at 0.6 next to dry open ground: shore from occupancy, not terrain",
            [15, 15, 0, 0, 15, 15, 0, 0],
            [0, 0, 2, 2, 0, 0, 0, 0],
        ),
        // Slots 0 and 1 (corners 4, 5 below) hold w = 8 with nothing above:
        // surface at t = 0. Slots 2 and 3 (corners 1, 0 and above) are bank.
        preset(
            "level at a terrain vertex",
            "level exactly 0: shoreline points are the terrain's bottom-edge vertices",
            [15, 15, 15, 15, 0, 0, 0, 0],
            [0, 0, 0, 0, 8, 8, 0, 0],
        ),
    ]
}
