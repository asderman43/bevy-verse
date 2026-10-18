//! Tests for the water mesher, following `docs/water-surface.md` §10.

use std::collections::HashMap;

use bevy::math::{vec3, Vec3};

use super::{
    mesher::{below_ledge, extract_with_overlap, slot_height, Corners},
    presets,
    tables::{MS_TRIS, VERT_EDGE},
    W_ISO, W_MAX,
};
use crate::voxel_terrain::isosurface::{
    flying_edges, terrain_vertex, Interpolate, Surface, VoxelBuffer, ISO, S_MAX,
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

/// The settled water value of a non-solid sample at height `y` under a
/// surface at `level`: full below, partial in the top slab, empty above
/// (§1.3: `H = y + (o − W_ISO − 1) / W_MAX`).
fn settled(level_units: i32, y: usize) -> u8 {
    // `level_units` is the surface height in 1/W_MAX steps.
    let o = level_units - (y as i32) * W_MAX as i32 + W_ISO as i32 + 1;
    o.clamp(0, W_MAX as i32) as u8
}

/// A bowl of terrain holding a settled pool at height 3 + 4/15.
fn basin(size: usize) -> (VoxelBuffer, VoxelBuffer) {
    let centre = (size as f32 - 1.0) / 2.0;
    let ground = |x: usize, z: usize| {
        let d2 = (x as f32 - centre).powi(2) + (z as f32 - centre).powi(2);
        1.2 + 0.18 * d2
    };
    let solid = field(size, |x, y, z| {
        (7.5 + 3.0 * (ground(x, z) - y as f32)).clamp(0.0, S_MAX as f32) as u8
    });
    let level = 3 * W_MAX as i32 + 4;
    let water = field(size, |x, y, z| {
        if solid.get(x, y, z) > ISO {
            0
        } else {
            settled(level, y)
        }
    });
    (solid, water)
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
    for q in 0..16 {
        let (solid, water) = presets::surface_case(q).buffers();
        let surface = mesh(&solid, &water, true);
        assert_eq!(surface.triangle_count(), MS_TRIS[q].count, "Q = {q:04b}");
        for p in &surface.positions {
            assert_eq!(p.y, 0.6, "Q = {q:04b}: vertex off the pool's level: {p}");
        }
    }
}

#[test]
fn a_solid_corner_is_always_inside() {
    for s in 0..=S_MAX {
        for w in 0..=W_MAX {
            let c = corners([s; 8], [w; 8]);
            assert_eq!(c.solid & !c.inside, 0, "s {s}, w {w}");
        }
    }
}

/// Rock below the isolevel never moves the water.
#[test]
fn rock_independence() {
    let size = 6;
    let level = 2 * W_MAX as i32 + 12;
    let floor = |y: usize| y < 2;
    let water = field(size, |_, y, _| if floor(y) { 0 } else { settled(level, y) });

    let mut state = 0xABCDu32;
    let mut rock = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((state >> 16) % (ISO as u32 + 1)) as u8
    };
    let bare = field(size, |_, y, _| if floor(y) { S_MAX } else { 0 });
    let rocky = field(size, |_, y, _| if floor(y) { S_MAX } else { rock() });

    let a = mesh(&bare, &water, true);
    let b = mesh(&rocky, &water, true);
    assert!(!a.is_empty());
    assert_eq!(a.positions, b.positions);
}

/// Filling a column one unit at a time raises the surface exactly 1/W_MAX per
/// unit, with no jump where the top sample turns inside, whatever invisible
/// rock the column holds.
#[test]
fn column_fill_sweep() {
    let size = 4;
    for rock in [0, 4, ISO] {
        let solid = field(size, |_, y, _| if y == 0 { S_MAX } else { rock });
        // Up to the top sample's last unit before it turns inside.
        let top = (size - 2) as u8 as i32 * W_MAX as i32 + W_ISO as i32;
        for units in 1..=top {
            let water = field(size, |_, y, _| {
                if y == 0 {
                    0
                } else {
                    (units - (y as i32 - 1) * W_MAX as i32).clamp(0, W_MAX as i32) as u8
                }
            });
            let surface = mesh(&solid, &water, true);
            let expected = (W_ISO as f32 + units as f32) / W_MAX as f32;
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

/// The integer ledge test agrees with exact arithmetic everywhere it's used.
#[test]
fn ledge_test_is_exact() {
    for n in W_ISO + 1..=2 * W_MAX {
        for s_lo in 0..=ISO {
            for s_hi in ISO + 1..=S_MAX {
                let water = (n as f64 - W_ISO as f64 - 1.0) / W_MAX as f64;
                let terrain = (ISO as f64 + 0.5 - s_lo as f64) / (s_hi as f64 - s_lo as f64);
                assert_eq!(below_ledge(n, s_lo, s_hi, true), water < terrain, "n {n}, s {s_lo}/{s_hi}");
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
    // Dips into the water: left out, the shore meets the rock.
    assert_eq!(slot0(13, 2), None);
    // Above the water: flat with the pool.
    assert_eq!(slot0(10, 2), Some(0.6));
    // Thick: as high as water can stand under it.
    assert_eq!(slot0(15, 0), Some(7.0 / 15.0));

    // Filling the pocket under a ledge rises one step per unit.
    let mut last: Option<f32> = None;
    for w6 in 0..=4 {
        let t = slot0(10, w6).expect("still below the ledge at 0.75");
        if let Some(last) = last {
            assert!((t - last - 1.0 / W_MAX as f32).abs() < 1e-6, "w6 {w6}");
        }
        last = Some(t);
    }

    // The meshes: dips gives the pentagon F0, V1, V2, V3, F3; above, one quad.
    assert_eq!(mesh_preset("F: ledge dips in").triangle_count(), 3);
    let above = mesh_preset("F: ledge above");
    assert_eq!(above.triangle_count(), 2);
    assert!(above.positions.iter().all(|p| p.y == 0.6));
}

#[test]
fn example_a_is_one_flat_quad() {
    let surface = mesh_preset("A: open water");
    assert_eq!(surface.triangle_count(), 2);
    assert!(surface.positions.iter().all(|p| p.y == 0.8));
}

#[test]
fn example_b_steps_into_the_next_cell() {
    let short = mesh_preset("B: one unit short");
    assert!(short.positions.iter().all(|p| p.y == 14.0 / 15.0));
    assert!(mesh_preset("B: just inside").is_empty());
}

#[test]
fn example_c_meets_the_bank_on_its_surface() {
    // Faces 1 (x = 1) and 3 (x = 0) join the water (z = 0) and bank (z = 1)
    // columns: shoreline at z = 73/120.
    let surface = mesh_preset("C: steep bank");
    let shore: Vec<Vec3> = surface.positions.iter().copied().filter(|p| p.z != 0.0).collect();
    assert!(!shore.is_empty());
    for p in shore {
        assert!((p.z - 73.0 / 120.0).abs() < 1e-6, "{p}");
        assert!((p.y - 13.0 / 15.0).abs() < 1e-6, "{p}");
    }
}

#[test]
fn example_d_cuts_the_bank_corner() {
    let surface = mesh_preset("D: bank corner");
    assert_eq!(surface.triangle_count(), 3, "pentagon");
    // The bank stands on corner 4, at (0, 0, 1); both shoreline points sit
    // 0.14 from it along their faces.
    let bank = vec3(0.0, 0.6, 1.0);
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
        assert!(((p - bank).length() - 0.14).abs() < 1e-6, "{p}");
    }
}

#[test]
fn example_e_climbs_onto_the_rock() {
    let settled = mesh_preset("E: rock through the surface");
    assert_eq!(settled.triangle_count(), 3, "pentagon");
    assert!(settled.positions.iter().all(|p| p.y == 0.6));

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
    // Face 1's bottom edge runs along z from corner 1 (1, 0, 0) to 5 (1, 0, 1).
    let vertex = terrain_vertex(&solid, (1, 0, 0), 2, true);
    assert!(surface.positions.iter().any(|&p| bits(p) == bits(vertex)), "{vertex} not in the water");
}

#[test]
fn a_settled_basin_is_flat_to_the_shore() {
    let (solid, water) = basin(12);
    let surface = super::extract(&solid, &water, true);
    assert!(surface.triangle_count() > 50);
    let level = 3.0 + 4.0 / 15.0;
    for p in &surface.positions {
        assert_eq!(p.y, surface.positions[0].y, "vertex {p} off the level");
    }
    assert!((surface.positions[0].y - level).abs() < 1e-6);
    for n in &surface.normals {
        assert_eq!(*n, Vec3::Y);
    }
}

/// Every terrain-derived shoreline point lies on flying edges' own mesh.
#[test]
fn shoreline_points_lie_on_the_terrain() {
    for interpolate in [true, false] {
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

        let mut checked = 0;
        for &p in &water_mesh.positions {
            // Surface vertices stand on vertical edges; everything else is a
            // shoreline point.
            if p.x.fract() == 0.0 && p.z.fract() == 0.0 {
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
fn interior_face_edges(surface: &Surface, size: usize) -> HashMap<([u32; 3], [u32; 3]), usize> {
    let interior = |v: f32| v.fract() == 0.0 && v >= 1.0 && v <= (size - 2) as f32;
    let mut edges = HashMap::new();
    for t in surface.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| surface.positions[t[k] as usize]);
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let in_plane = (0..3).any(|axis| p[axis] == q[axis] && interior(p[axis]));
            if in_plane && p != q {
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
    let edges = interior_face_edges(&surface, 12);
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
    let solid = field(size, |_, y, _| if y == 0 { S_MAX } else { 0 });
    let low = W_MAX as i32 + 11; // H = 1 + 11/15, in the second row
    let high = 2 * W_MAX as i32 + 11; // H = 2 + 11/15, in the third
    let water = field(size, |x, y, _| {
        if y == 0 {
            0
        } else {
            settled(if x < 3 { low } else { high }, y)
        }
    });

    let surface = super::extract(&solid, &water, true);
    let edges = interior_face_edges(&surface, size);
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

