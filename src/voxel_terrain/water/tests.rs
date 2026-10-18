//! Tests for the water mesher, following `docs/water-surface.md` §10.

use std::collections::HashMap;

use bevy::math::{vec3, Vec3};

use super::{
    mesher::{below_ledge, extract_with_overlap, slot_height, Corners},
    presets,
    tables::{MS_TRIS, VERT_EDGE, VERT_T},
    W_ISO, W_MAX,
};
use crate::voxel_terrain::isosurface::{
    edge_t, flying_edges, terrain_vertex, Interpolate, Surface, VoxelBuffer, ISO, S_MAX,
};

/// Meshes with the shore overlap off, so shoreline points sit exactly on the
/// terrain.
fn mesh(solid: &VoxelBuffer, water: &VoxelBuffer, interpolate: Interpolate) -> Surface {
    extract_with_overlap(solid, water, interpolate, 0.0)
}

fn mesh_preset(name: &str) -> Surface {
    let preset = presets::all()
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no preset {name}"));
    let (solid, water) = preset.buffers();
    mesh(&solid, &water, true)
}

/// One cell's corners, straight from arrays.
fn corners(s: [u8; 8], w: [u8; 8]) -> Corners {
    let solid = VoxelBuffer::from_samples(s.to_vec(), 2);
    let water = VoxelBuffer::from_samples(w.to_vec(), 2);
    Corners::load(&solid, &water, (0, 0, 0))
}

/// A cube of samples from a function of position.
fn field(size: usize, mut f: impl FnMut(usize, usize, usize) -> u8) -> VoxelBuffer {
    let mut buffer = VoxelBuffer::new(size);
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                buffer.set(x, y, z, f(x, y, z));
            }
        }
    }
    buffer
}

/// How much of sample `y`'s slab a column filled to `units` covers. Heights
/// are in fill units from the bottom of sample 0's slab: `units` reaches
/// `−1/2 + units / S_MAX`.
fn fill(units: i32, y: usize) -> u8 {
    (units - y as i32 * S_MAX as i32).clamp(0, S_MAX as i32) as u8
}

/// The height a column filled to `units` reaches.
fn height(units: i32) -> f32 {
    (units as f32 - (ISO as f32 + 0.5)) / S_MAX as f32
}

/// A heightfield of rock filled to `ground(x, z)` units, with water settled to
/// `level(x, z)` units over it: every sample holds rock up to the ground and
/// water above that up to the level.
fn settled(
    size: usize,
    ground: impl Fn(usize, usize) -> i32,
    level: impl Fn(usize, usize) -> i32,
) -> (VoxelBuffer, VoxelBuffer) {
    let solid = field(size, |x, y, z| fill(ground(x, z), y));
    let water = field(size, |x, y, z| {
        fill(level(x, z).max(ground(x, z)), y) - fill(ground(x, z), y)
    });
    (solid, water)
}

/// The level `basin` fills to, in units.
const BASIN_LEVEL: i32 = 4 * S_MAX as i32 + 4;

/// A bowl holding a settled pool at [`BASIN_LEVEL`].
fn basin(size: usize) -> (VoxelBuffer, VoxelBuffer) {
    bowl(size, 0.18, BASIN_LEVEL)
}

/// A bowl with walls rising `steepness` cells per cell squared from the
/// centre, holding a settled pool at `level` units.
fn bowl(size: usize, steepness: f32, level: i32) -> (VoxelBuffer, VoxelBuffer) {
    let centre = (size as f32 - 1.0) / 2.0;
    let ground = move |x: usize, z: usize| {
        let d2 = (x as f32 - centre).powi(2) + (z as f32 - centre).powi(2);
        ((1.7 + steepness * d2) * S_MAX as f32) as i32
    };
    settled(size, ground, move |_, _| level)
}

/// Every level over two cell rows, in a gentle and a steep bowl: flat to the
/// shore, watertight between cells, and every shoreline point on the terrain.
#[test]
fn basins_at_every_level() {
    for steepness in [0.11, 0.4] {
        for level in 2 * S_MAX as i32 + 9..=4 * S_MAX as i32 + 8 {
            let (solid, water) = bowl(12, steepness, level);
            let surface = mesh(&solid, &water, true);
            let label = format!("steepness {steepness}, level {level}");
            assert!(!surface.is_empty(), "{label}: nothing drawn");

            let y = surface.positions[0].y;
            assert!((y - height(level)).abs() < 1e-5, "{label}: at {y}");
            assert!(surface.positions.iter().all(|p| p.y == y), "{label}: not flat");

            for (edge, count) in interior_face_edges(&surface, 12, Some(&water)) {
                assert_eq!(count, 2, "{label}: edge {edge:?} used by {count} triangles");
            }

            let terrain = flying_edges::extract(&solid, true);
            let segments: Vec<(Vec3, Vec3)> = terrain
                .indices
                .chunks(3)
                .flat_map(|t| {
                    let [a, b, c] = [0, 1, 2].map(|k| terrain.positions[t[k] as usize]);
                    [(a, b), (b, c), (c, a)]
                })
                .collect();
            for &p in &surface.positions {
                let wet = p.x.fract() == 0.0
                    && p.z.fract() == 0.0
                    && (0..12).any(|y| water.get(p.x as usize, y, p.z as usize) > 0);
                if wet {
                    continue;
                }
                let nearest = segments
                    .iter()
                    .map(|&(a, b)| {
                        let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                        (a + (b - a) * t - p).length()
                    })
                    .fold(f32::MAX, f32::min);
                assert!(nearest < 1e-5, "{label}: {p} is {nearest} off the terrain");
            }
        }
    }
}

fn bits(p: Vec3) -> [u32; 3] {
    p.to_array().map(f32::to_bits)
}

#[test]
fn every_preset_meshes() {
    for preset in presets::all() {
        let (solid, water) = preset.buffers();
        for interpolate in [true, false] {
            let surface = super::extract(&solid, &water, interpolate);
            assert!(
                surface.positions.iter().all(|p| p.is_finite()),
                "{}: non-finite position",
                preset.name
            );
            for n in &surface.normals {
                assert!((n.length() - 1.0).abs() < 1e-5 && n.y > 0.0, "{}: normal {n}", preset.name);
            }
        }
    }
}

#[test]
fn surface_cases_give_their_table_shapes() {
    let level = VERT_T[S_MAX as usize + 2];
    for q in 0..16 {
        let (solid, water) = presets::surface_case(q).buffers();
        let surface = mesh(&solid, &water, true);
        assert_eq!(surface.triangle_count(), MS_TRIS[q].count, "Q = {q:04b}");
        for p in &surface.positions {
            assert_eq!(p.y, level, "Q = {q:04b}: vertex off the pool's level: {p}");
        }
    }
}

#[test]
fn a_solid_corner_is_always_inside() {
    for s in 0..=S_MAX {
        for w in 0..=S_MAX - s {
            let c = corners([s; 8], [w; 8]);
            assert_eq!(c.solid & !c.inside, 0, "s {s}, w {w}");
        }
    }
}

/// The water rests on the terrain: water filling a column to some height is
/// drawn exactly where the terrain would draw rock filling it to the same
/// height. With no water at all it would sit on the drawn ground, and each
/// unit lifts it 1/15 from there.
#[test]
fn water_rests_on_the_terrain() {
    for rock in 0..=21 {
        for total in (rock + 1).max(ISO as i32 + 1)..=22 {
            let (solid, water) = settled(2, |_, _| rock, |_, _| total);
            let (all_rock, _) = settled(2, |_, _| total, |_, _| total);

            // Slot 2 is corners (1, 3), table edge 1, running up.
            let c = Corners::load(&solid, &water, (0, 0, 0));
            let drawn = edge_t(1, &all_rock.cell_corners(0, 0, 0), true);
            let t = slot_height(&c, 2, true).expect("the column carries the surface");
            assert!(
                (t - drawn).abs() < 1e-6,
                "rock {rock}, filled to {total}: water at {t}, rock filled as far drawn at {drawn}",
            );
        }
    }
}

/// Rock that doesn't rest on anything below it -- held up from the side -- is
/// not under the water, and doesn't move it.
#[test]
fn rock_not_resting_below_doesnt_move_the_water() {
    let size = 6;
    // Floor at y = 0, sample 1 full of water, sample 2 holding 5 units.
    let level = 2 * S_MAX as i32 + 5;
    let water = field(size, |_, y, _| if y == 0 { 0 } else { fill(level, y) });

    let mut state = 0xABCDu32;
    let mut rock = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((state >> 16) % (ISO as u32 + 1)) as u8
    };
    let bare = field(size, |_, y, _| if y == 0 { S_MAX } else { 0 });
    // Sample 1 is full of water, so rock above it rests on nothing. Sample 2
    // only gets rock up to the room its water leaves.
    let rocky = field(size, |_, y, _| match y {
        0 => S_MAX,
        1 => 0,
        2 => rock().min(S_MAX - 5),
        _ => rock(),
    });

    let a = mesh(&bare, &water, true);
    let b = mesh(&rocky, &water, true);
    assert!(!a.is_empty());
    assert_eq!(a.positions, b.positions);
}

/// Filling a column one unit at a time raises the surface exactly 1/15 per
/// unit, from the drawn ground up, with no jump where a sample turns inside --
/// whatever rock the column's first sample holds.
#[test]
fn column_fill_sweep() {
    let size = 4;
    for rock in [0, 4, ISO] {
        let ground = S_MAX as i32 + rock as i32;
        // Up to the top sample's last unit before it turns inside.
        let top = 3 * S_MAX as i32 + ISO as i32;
        for units in ground + 1..=top {
            let (solid, water) = settled(size, |_, _| ground, |_, _| units);
            let surface = mesh(&solid, &water, true);
            let expected = height(units);
            assert!(!surface.is_empty(), "rock {rock}, {units} units: nothing drawn");
            for p in &surface.positions {
                assert!(
                    (p.y - expected).abs() < 1e-5,
                    "rock {rock}, {units} units: surface at {}, expected {expected}",
                    p.y
                );
            }
        }
    }
}

/// The integer ledge test agrees with where the terrain actually draws the
/// ledge. On a tie the water touches the ledge, which counts as reaching it.
#[test]
fn ledge_test_matches_the_drawn_ledge() {
    for s_lo in 0..=ISO {
        for s_hi in ISO + 1..=S_MAX {
            // Table edge 1 runs up from corner 1 into a ledge at corner 3.
            let mut s = [0; 8];
            s[1] = s_lo;
            s[3] = s_hi;
            let ledge = edge_t(1, &s, true);
            for n in W_ISO + 1..=2 * W_MAX {
                let water = VERT_T[n as usize];
                let below = below_ledge(n, s_lo, s_hi, true);
                if n as u16 + s_lo as u16 + s_hi as u16 == 2 * S_MAX as u16 {
                    assert!(!below, "n {n}, s {s_lo}/{s_hi}: a tie touches the ledge");
                    assert!((water - ledge).abs() < 1e-6, "n {n}, s {s_lo}/{s_hi}: not a tie");
                } else {
                    assert_eq!(below, water < ledge, "n {n}, s {s_lo}/{s_hi}");
                }
                assert_eq!(below_ledge(n, s_lo, s_hi, false), water < 0.5, "n {n}, midpoint");
            }
        }
    }
}

/// Example F (§7).
#[test]
fn ledges() {
    let slot0 = |s6: u8, w6: u8| {
        let mut s = [0; 8];
        let mut w = [15, 15, 2, 2, 15, 15, 2, 2];
        s[6] = s6;
        w[6] = w6;
        slot_height(&corners(s, w), 0, true)
    };
    let pool = VERT_T[S_MAX as usize + 2];
    // Dips into the water, or just touches it: left out, the shore meets the
    // rock.
    assert_eq!(slot0(14, 1), None);
    assert_eq!(slot0(13, 2), None);
    assert_eq!(slot0(15, 0), None);
    // Above the water: flat with the pool.
    assert_eq!(slot0(10, 2), Some(pool));

    // Filling the pocket under a ledge rises one step per unit, until its
    // slab is full and the water reaches the drawn underside.
    let mut last: Option<f32> = None;
    for w6 in 0..=4 {
        let t = slot0(10, w6).expect("still below the ledge at 12.5/15");
        if let Some(last) = last {
            assert!((t - last - 1.0 / W_MAX as f32).abs() < 1e-6, "w6 {w6}");
        }
        last = Some(t);
    }
    assert_eq!(slot0(10, 5), None, "a full slab reaches the ledge");

    // The meshes: dipping gives the pentagon F0, V1, V2, V3, F3. Touching puts
    // F0 and F3 both on the ledge's own vertex on slot 0, at the pool's level,
    // so the pentagon closes up into a quad. Above, one flat quad.
    assert_eq!(mesh_preset("F: ledge dips in").triangle_count(), 3);
    let touches = mesh_preset("F: ledge touches");
    assert_eq!(touches.triangle_count(), 2);
    assert!(touches.positions.iter().any(|p| *p == vec3(0.0, pool, 1.0)));
    let above = mesh_preset("F: ledge above");
    assert_eq!(above.triangle_count(), 2);
    assert!(above.positions.iter().all(|p| p.y == pool));
}

#[test]
fn example_a_is_one_flat_quad() {
    let surface = mesh_preset("A: open water");
    assert_eq!(surface.triangle_count(), 2);
    assert!(surface.positions.iter().all(|p| p.y == VERT_T[20]));
    assert!((VERT_T[20] - 12.5 / 15.0).abs() < 1e-6);
}

#[test]
fn example_b_steps_into_the_next_cell() {
    let short = mesh_preset("B: one unit short");
    assert!(!short.is_empty());
    assert!(short.positions.iter().all(|p| p.y == VERT_T[22]));
    assert!(mesh_preset("B: just inside").is_empty());
}

#[test]
fn example_c_meets_the_bank_on_its_surface() {
    // Faces 1 (x = 1) and 3 (x = 0) join the water (z = 0) and bank (z = 1)
    // columns: the terrain runs from 0.5 across the bottom edge to 0.625
    // across the top, so the shoreline at 0.9 is at 0.5 + 0.125 * 0.9.
    let surface = mesh_preset("C: steep bank");
    let shore: Vec<Vec3> = surface.positions.iter().copied().filter(|p| p.z != 0.0).collect();
    assert!(!shore.is_empty());
    for p in shore {
        assert!((p.z - 0.6125).abs() < 1e-6, "{p}");
        assert!((p.y - 0.9).abs() < 1e-6, "{p}");
    }
}

#[test]
fn example_d_cuts_the_bank_corner() {
    let surface = mesh_preset("D: bank corner");
    assert_eq!(surface.triangle_count(), 3, "pentagon");
    // The bank stands on corner 4, at (0, 0, 1), drawn at 0.9 on its vertical
    // edge and halfway along the bottom edges; both shoreline points sit
    // 0.5 * (1 - level / 0.9) from it.
    let level = VERT_T[S_MAX as usize + 2];
    let expected = 0.5 * (1.0 - level / 0.9);
    let bank = vec3(0.0, level, 1.0);
    let mut shore: Vec<Vec3> = surface
        .positions
        .iter()
        .copied()
        .filter(|p| p.x.fract() != 0.0 || p.z.fract() != 0.0)
        .collect();
    shore.sort_by(|a, b| a.to_array().partial_cmp(&b.to_array()).unwrap());
    shore.dedup();
    assert_eq!(shore.len(), 2);
    for p in shore {
        assert!(((p - bank).length() - expected).abs() < 1e-5, "{p}");
    }
}

#[test]
fn example_e_climbs_onto_the_rock() {
    let level = VERT_T[S_MAX as usize + 2];
    let settled = mesh_preset("E: rock through the surface");
    assert_eq!(settled.triangle_count(), 3, "pentagon");
    assert!(settled.positions.iter().all(|p| p.y == level));

    // With corner 3 water instead, no terrain brackets the level, and the
    // fallback puts F1 on edge 10 and F2 on edge 2, both at the top.
    let unsettled = mesh_preset("E: unsettled");
    assert_eq!(unsettled.triangle_count(), 3);
    assert!(unsettled.positions.iter().any(|p| p.y == 1.0 && p.x == 1.0));
    assert!(unsettled.positions.iter().any(|p| p.y == 1.0 && p.z == 0.0));
}

#[test]
fn a_level_at_a_terrain_vertex_lands_on_it() {
    let preset = presets::all().into_iter().find(|p| p.name == "level at a terrain vertex").unwrap();
    let (solid, water) = preset.buffers();
    let surface = mesh(&solid, &water, true);
    // Slot 1's vertical edge, corners 5 (1, 0, 1) to 7: the bank, drawn at
    // exactly the water's level.
    let vertex = terrain_vertex(&solid, (1, 0, 1), 1, true);
    assert_eq!(vertex.y, VERT_T[18]);
    assert!(surface.positions.iter().any(|&p| bits(p) == bits(vertex)), "{vertex} not in the water");
}

#[test]
fn a_settled_basin_is_flat_to_the_shore() {
    let (solid, water) = basin(12);
    let surface = super::extract(&solid, &water, true);
    assert!(surface.triangle_count() > 50, "{} triangles", surface.triangle_count());
    let level = height(BASIN_LEVEL);
    for p in &surface.positions {
        assert_eq!(p.y, surface.positions[0].y, "vertex {p} off the level");
    }
    assert!((surface.positions[0].y - level).abs() < 1e-6);
    for n in &surface.normals {
        assert_eq!(*n, Vec3::Y);
    }
}

/// Every shoreline point in settled water lies on flying edges' own mesh:
/// none floats, and none comes from the fallback.
///
/// Only for interpolated terrain: without interpolation the terrain is drawn
/// at edge midpoints, which the water's fill heights don't follow.
#[test]
fn shoreline_points_lie_on_the_terrain() {
    for interpolate in [true] {
        let (solid, water) = basin(12);
        let terrain = flying_edges::extract(&solid, interpolate);
        let water_mesh = mesh(&solid, &water, interpolate);

        let segments: Vec<(Vec3, Vec3)> = terrain
            .indices
            .chunks(3)
            .flat_map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| terrain.positions[t[k] as usize]);
                [(a, b), (b, c), (c, a)]
            })
            .collect();
        let distance = |p: Vec3, (a, b): (Vec3, Vec3)| {
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            (a + (b - a) * t - p).length()
        };

        // The water's own vertices stand on columns holding water; anything
        // elsewhere is a shoreline point and must be on the terrain.
        let wet_column = |x: f32, z: f32| {
            x.fract() == 0.0
                && z.fract() == 0.0
                && (0..12).any(|y| water.get(x as usize, y, z as usize) > 0)
        };
        let mut checked = 0;
        for &p in &water_mesh.positions {
            if wet_column(p.x, p.z) {
                continue;
            }
            let nearest = segments.iter().map(|&s| distance(p, s)).fold(f32::MAX, f32::min);
            assert!(nearest < 1e-5, "interpolate {interpolate}: {p} is {nearest} off the terrain");
            checked += 1;
        }
        assert!(checked > 10, "only {checked} shoreline points");
    }
}

/// Triangle edges lying in a cell face inside the buffer, keyed bitwise. In a
/// watertight surface each one is used by exactly two triangles: one in each
/// cell sharing the face.
///
/// Except a shoreline chord that happens to run along a face -- the level
/// exactly at a bank's drawn ground on both its corners -- which is the
/// water's edge and has nothing beyond it. Given `water`, edges joining two
/// points that aren't on wet columns are left out for that reason.
fn interior_face_edges(
    surface: &Surface,
    size: usize,
    water: Option<&VoxelBuffer>,
) -> HashMap<([u32; 3], [u32; 3]), usize> {
    let interior = |v: f32| v.fract() == 0.0 && v >= 1.0 && v <= (size - 2) as f32;
    let on_wet_column = |p: Vec3| {
        water.is_none_or(|water| {
            p.x.fract() == 0.0
                && p.z.fract() == 0.0
                && (0..size).any(|y| water.get(p.x as usize, y, p.z as usize) > 0)
        })
    };
    let mut edges = HashMap::new();
    for t in surface.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| surface.positions[t[k] as usize]);
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let in_plane = (0..3).any(|axis| p[axis] == q[axis] && interior(p[axis]));
            if in_plane && p != q && (on_wet_column(p) || on_wet_column(q)) {
                let key = if bits(p) < bits(q) { (bits(p), bits(q)) } else { (bits(q), bits(p)) };
                *edges.entry(key).or_insert(0) += 1;
            }
        }
    }
    edges
}

#[test]
fn neighbouring_cells_agree_on_their_shared_faces() {
    let (solid, water) = basin(12);
    let surface = super::extract(&solid, &water, true);
    let edges = interior_face_edges(&surface, 12, Some(&water));
    assert!(!edges.is_empty());
    for (edge, count) in edges {
        assert_eq!(count, 2, "edge {edge:?} used by {count} triangles");
    }
}

/// Two columns whose surfaces sit in different cell rows, with no terrain
/// between them (unsettled): the fallback joins them with a sloped strip and
/// no crack.
#[test]
fn a_level_step_has_no_crack() {
    let size = 6;
    let (solid, water) = settled(
        size,
        |_, _| S_MAX as i32,
        |x, _| if x < 3 { 2 * S_MAX as i32 + 4 } else { 3 * S_MAX as i32 + 4 },
    );

    let surface = super::extract(&solid, &water, true);
    let edges = interior_face_edges(&surface, size, None);
    assert!(edges.len() > 4);
    for (edge, count) in edges {
        assert_eq!(count, 2, "edge {edge:?} used by {count} triangles");
    }

    // Sloped, but facing up.
    assert!(surface.normals.iter().any(|n| n.x != 0.0));
    for n in &surface.normals {
        assert!(n.y > 0.0 && (n.length() - 1.0).abs() < 1e-5, "{n}");
    }
}

/// Two chunks sharing a face put the same shoreline points on it. The terrain
/// itself evaluates a far-face vertex through the boundary cell's own table
/// edge, so the two chunks can disagree in the last bits there until chunks
/// get an apron; hence the tolerance.
#[test]
fn chunks_agree_on_their_shared_face() {
    let (solid, water) = basin(9);
    let sub = |buffer: &VoxelBuffer, ox: usize| field(5, |x, y, z| buffer.get(x + ox, y + 2, z + 2));
    let left = mesh(&sub(&solid, 0), &sub(&water, 0), true);
    let right = mesh(&sub(&solid, 4), &sub(&water, 4), true);

    let on_face = |surface: &Surface, x: f32| -> Vec<Vec3> {
        let mut points: Vec<Vec3> = surface.positions.iter().copied().filter(|p| p.x == x).collect();
        points.sort_by(|a, b| a.to_array().partial_cmp(&b.to_array()).unwrap());
        points.dedup();
        points
    };
    let a = on_face(&left, 4.0);
    let b: Vec<Vec3> = on_face(&right, 0.0).into_iter().map(|p| p + Vec3::X * 4.0).collect();
    assert!(!a.is_empty());
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(&b) {
        assert!((*p - *q).length() < 1e-5, "{p} vs {q}");
    }
}

#[test]
fn slots_follow_the_doc() {
    assert_eq!(VERT_EDGE, [(4, 6), (5, 7), (1, 3), (0, 2)]);
}


