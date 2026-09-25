//! Cross-validation for the extractors.
//!
//! The load-bearing test is [`compare`]: it runs a field through a
//! deliberately naive marching cubes written out longhand below, then through
//! every (method, interpolate) combination, and requires the three to agree
//! triangle for triangle. That catches the whole class of bugs flying edges is
//! prone to -- a mis-counted vertex, a trim that clips real geometry, an ID
//! that drifts by one along a row -- which are invisible in a mesh that merely
//! looks plausible.

use bevy::math::{vec3, Vec3};

use super::{
    case_index, extract, extract_with, flying_edges, marching_cubes, place_vertex,
    tables::{EDGE_VERTEX_INDICES, TRIANGLE_COUNT, TRIANGLE_TABLE, VERTEX_POSITIONS},
    test_maps, ExtractionJob, ExtractionScratch, Interpolate, Method, Surface, VoxelBuffer,
    MAX_SIZE,
};

/// A triangle with quantised coordinates, so it compares exactly.
type Tri = [[i32; 3]; 3];

/// Snaps to a 1/1024 grid. Shared edges get interpolated from whichever side
/// reaches them first, which can differ by an ULP; the test fields never land
/// near a snap boundary.
fn quant(v: f32) -> i32 {
    (v * 1024.0).round() as i32
}

/// Rotates a triangle to start at its lowest vertex. Preserves winding, but
/// makes the comparison independent of which corner got listed first.
fn canonical(mut tri: Tri) -> Tri {
    let lowest = (0..3).min_by_key(|&i| tri[i]).unwrap();
    tri.rotate_left(lowest);
    tri
}

/// Marching cubes, written out longhand as the thing to be believed. No
/// welding, no trimming, no counting passes -- just every cell in turn.
fn reference(buffer: &VoxelBuffer, isolevel: i8, interpolate: Interpolate) -> Vec<Tri> {
    let cells = buffer.cells_per_axis();
    let mut out = Vec::new();

    for z in 0..cells {
        for y in 0..cells {
            for x in 0..cells {
                let corners = buffer.cell_corners(x, y, z);
                let case = case_index(&corners, isolevel);
                let cell = vec3(x as f32, y as f32, z as f32);

                let mut i = 0;
                while TRIANGLE_TABLE[case][i] != -1 {
                    let mut tri: Tri = [[0; 3]; 3];
                    for k in 0..3 {
                        let edge = TRIANGLE_TABLE[case][i + k] as usize;
                        let v = place_vertex(cell, edge, &corners, isolevel, interpolate);
                        tri[k] = [quant(v.x), quant(v.y), quant(v.z)];
                    }
                    out.push(canonical(tri));
                    i += 3;
                }
            }
        }
    }

    out.sort();
    out
}

/// Turns an extracted surface into comparable triangles, checking the buffer
/// invariants on the way.
fn triangles_of(surface: &Surface, label: &str) -> Vec<Tri> {
    assert_eq!(
        surface.indices.len() % 3,
        0,
        "{label}: index buffer is not a multiple of 3",
    );
    assert!(
        surface.positions.iter().all(|v| v.is_finite()),
        "{label}: vertex buffer holds non-finite positions",
    );

    // Flying edges sizes its vertex buffer by counting cut edges before it
    // writes any. If that count is right, every slot ends up referenced:
    // over-counting leaves holes, under-counting runs off the end.
    let mut used = vec![false; surface.positions.len()];
    for &i in &surface.indices {
        used[i as usize] = true;
    }
    let unused: Vec<usize> = used
        .iter()
        .enumerate()
        .filter(|(_, &u)| !u)
        .map(|(i, _)| i)
        .collect();
    assert!(
        unused.is_empty(),
        "{label}: {} of {} vertex slots were allocated but never referenced: {:?}",
        unused.len(),
        surface.positions.len(),
        &unused[..unused.len().min(16)],
    );

    let mut out: Vec<Tri> = surface
        .indices
        .chunks(3)
        .map(|t| {
            let mut tri: Tri = [[0; 3]; 3];
            for k in 0..3 {
                let v = surface.positions[t[k] as usize];
                tri[k] = [quant(v.x), quant(v.y), quant(v.z)];
            }
            canonical(tri)
        })
        .collect();
    out.sort();
    out
}

fn diff(name: &str, expected: &[Tri], actual: &[Tri]) {
    if expected == actual {
        return;
    }

    let missing: Vec<_> = expected.iter().filter(|t| !actual.contains(t)).collect();
    let spurious: Vec<_> = actual.iter().filter(|t| !expected.contains(t)).collect();

    panic!(
        "{name}: disagrees with the reference\n\
         expected {} triangles, got {}\n\
         {} missing (first 8): {:?}\n\
         {} spurious (first 8): {:?}",
        expected.len(),
        actual.len(),
        missing.len(),
        &missing[..missing.len().min(8)],
        spurious.len(),
        &spurious[..spurious.len().min(8)],
    );
}

/// Requires both methods, with and without interpolation, to reproduce the
/// reference exactly.
fn compare(name: &str, map: (Vec<i8>, i8, usize)) {
    let (buffer, isolevel) = test_maps::buffer(map);

    for interpolate in [true, false] {
        let expected = reference(&buffer, isolevel, interpolate);

        for method in [Method::FlyingEdges, Method::MarchingCubes] {
            let label = format!("{name} / {method:?} / interpolate={interpolate}");
            let surface = extract(&ExtractionJob {
                buffer: buffer.clone(),
                isolevel,
                method,
                interpolate,
            });
            diff(&label, &expected, &triangles_of(&surface, &label));
        }
    }
}

#[test]
fn solid_cube_4() {
    compare("solid_cube(4)", test_maps::solid_cube(4));
}

#[test]
fn solid_cube_8() {
    compare("solid_cube(8)", test_maps::solid_cube(8));
}

#[test]
fn single_voxel() {
    compare("single_voxel", test_maps::single_voxel());
}

#[test]
fn scattered_voxels() {
    compare("scattered", test_maps::scattered());
}

#[test]
fn terrain() {
    compare("terrain", test_maps::terrain());
}

#[test]
fn flat_slab() {
    // Cuts only Y edges, so a trim keyed on cut X edges loses all of it.
    compare("flat_slab", test_maps::flat_slab(6));
}

#[test]
fn sphere() {
    // Long runs of uniform interior, which is what the trim exists for.
    compare("sphere", test_maps::sphere(24, 8.0));
}

#[test]
fn pseudo_random() {
    compare("pseudo_random", test_maps::pseudo_random(10, 0x1234_5678));
}

#[test]
fn a_chunk_at_the_size_limit() {
    // 64 samples fill a row bitmask exactly, so every shift and mask in pass 1
    // is at its boundary here. One off and the top row of edges is lost.
    compare("sphere(MAX_SIZE)", test_maps::sphere(MAX_SIZE, 24.0));
}

/// The pool is what makes repeat extraction cheap, so it has to give the same
/// answer as a cold one. Leftovers from a previous, larger chunk -- stale rows,
/// stale weld entries -- are exactly the bug this guards, so the fields are
/// deliberately ordered large, small, large.
#[test]
fn a_reused_pool_matches_a_fresh_one() {
    let fields = [
        test_maps::sphere(16, 5.0),
        test_maps::terrain(),
        test_maps::solid_cube(4),
        test_maps::flat_slab(6),
        test_maps::single_voxel(),
        test_maps::sphere(16, 5.0),
    ];

    let mut scratch = ExtractionScratch::new();

    for field in fields {
        let (buffer, isolevel) = test_maps::buffer(field);

        for method in [Method::FlyingEdges, Method::MarchingCubes] {
            let job = ExtractionJob {
                buffer: buffer.clone(),
                isolevel,
                method,
                interpolate: true,
            };
            assert_eq!(
                extract_with(&mut scratch, &job),
                extract(&job),
                "{method:?}: pooled extraction diverged from a cold one",
            );
        }
    }
}

/// Once the pool has seen the biggest chunk it is going to see, extraction
/// must stop calling the allocator altogether.
#[test]
fn a_warm_pool_stops_growing() {
    let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(16, 5.0));
    let mut scratch = ExtractionScratch::new();

    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        let job = ExtractionJob {
            buffer: buffer.clone(),
            isolevel,
            method,
            interpolate: true,
        };

        extract_with(&mut scratch, &job);
        let warm = scratch.capacity_bytes();

        for _ in 0..8 {
            extract_with(&mut scratch, &job);
        }
        assert_eq!(
            scratch.capacity_bytes(),
            warm,
            "{method:?}: the pool reallocated after warming up",
        );
    }
}

#[test]
fn empty_field_yields_an_empty_surface() {
    let buffer = VoxelBuffer::new(8);
    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        let surface = extract(&ExtractionJob {
            buffer: buffer.clone(),
            isolevel: 2,
            method,
            interpolate: true,
        });
        assert!(surface.is_empty(), "{method:?} produced triangles");
        assert!(surface.positions.is_empty(), "{method:?} produced vertices");
    }
}

#[test]
fn a_buffer_too_small_to_hold_a_cell_is_handled() {
    for size in [0, 1] {
        for method in [Method::FlyingEdges, Method::MarchingCubes] {
            let surface = extract(&ExtractionJob {
                buffer: VoxelBuffer::new(size),
                isolevel: 2,
                method,
                interpolate: true,
            });
            assert!(surface.is_empty(), "{method:?} at size {size}");
        }
    }
}

#[test]
fn interpolation_moves_vertices_but_not_topology() {
    let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(16, 5.0));

    let job = |interpolate| ExtractionJob {
        buffer: buffer.clone(),
        isolevel,
        method: Method::FlyingEdges,
        interpolate,
    };

    let smooth = extract(&job(true));
    let blocky = extract(&job(false));

    assert_eq!(smooth.indices, blocky.indices, "topology changed");
    assert_eq!(smooth.positions.len(), blocky.positions.len());
    assert_ne!(
        smooth.positions, blocky.positions,
        "interpolation had no effect on vertex positions",
    );
}

#[test]
fn winding_faces_away_from_the_solid() {
    // TRIANGLE_TABLE winds each triangle so its normal points toward the
    // corners whose case bit is set, and the bit is set for `<= isolevel`. So
    // `<= isolevel` has to be empty space for normals to point out of the
    // solid, which is what wgpu's default Ccw / cull-Back and
    // StandardMaterial's lighting expect. Feed in the opposite sense and the
    // whole mesh is inside-out.
    let size = 24;
    let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(size, 8.0));
    let centre = Vec3::splat((size as f32 - 1.0) / 2.0);

    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        let surface = extract(&ExtractionJob {
            buffer: buffer.clone(),
            isolevel,
            method,
            interpolate: true,
        });

        let mut outward = 0;
        let mut inward = 0;
        let mut degenerate = 0;

        for t in surface.indices.chunks(3) {
            let (a, b, c) = (
                surface.positions[t[0] as usize],
                surface.positions[t[1] as usize],
                surface.positions[t[2] as usize],
            );
            let normal = (b - a).cross(c - a);
            if normal.length() < 1e-6 {
                degenerate += 1;
                continue;
            }
            // The field is a ball, so "away from the solid" is radially out.
            if normal.dot((a + b + c) / 3.0 - centre) > 0.0 {
                outward += 1;
            } else {
                inward += 1;
            }
        }

        assert!(outward > 0, "{method:?} produced no triangles");
        assert_eq!(inward, 0, "{method:?}: {inward} triangles wind inward");
        assert_eq!(degenerate, 0, "{method:?}: {degenerate} zero-area triangles");
    }
}

/// A test map as `test_maps` hands them out: samples, isolevel, size.
type Field = (Vec<i8>, i8, usize);

/// Every field the normal tests run over: tidy ones, dense noise full of
/// degenerate triangles, and a size-2 field that is all boundary.
fn normal_fields() -> Vec<(&'static str, Field)> {
    vec![
        ("sphere", test_maps::sphere(24, 8.0)),
        ("terrain", test_maps::terrain()),
        ("solid_cube", test_maps::solid_cube(8)),
        ("single_voxel", test_maps::single_voxel()),
        ("scattered", test_maps::scattered()),
        ("flat_slab", test_maps::flat_slab(6)),
        ("pseudo_random", test_maps::pseudo_random(12, 0xC0FFEE)),
        ("size_2", (vec![0, 5, 0, 0, 5, 5, 0, 5], 2, 2)),
        ("gradient_tie", gradient_tie()),
    ]
}

/// A field where the density gradient cancels at a vertex: two solid samples
/// either side of one sitting exactly on the isolevel, so both edges into it
/// put a vertex on that sample, where the gradient is zero.
fn gradient_tie() -> Field {
    let size = 5;
    let mut map = vec![0i8; size * size * size];
    let at = |x: usize, y: usize, z: usize| x + y * size + z * size * size;
    map[at(1, 2, 2)] = 5;
    map[at(3, 2, 2)] = 5;
    map[at(2, 2, 2)] = 2;
    (map, 2, size)
}

#[test]
fn normals_are_unit_and_finite() {
    for (name, field) in normal_fields() {
        let (buffer, isolevel) = test_maps::buffer(field);

        for interpolate in [true, false] {
            for method in [Method::FlyingEdges, Method::MarchingCubes] {
                let label = format!("{name} / {method:?} / interpolate={interpolate}");
                let surface = extract(&ExtractionJob {
                    buffer: buffer.clone(),
                    isolevel,
                    method,
                    interpolate,
                });

                assert_eq!(
                    surface.normals.len(),
                    surface.positions.len(),
                    "{label}: one normal per vertex",
                );
                for (i, n) in surface.normals.iter().enumerate() {
                    assert!(
                        n.is_finite() && (n.length() - 1.0).abs() < 1e-4,
                        "{label}: normal {i} is {n}",
                    );
                }
            }
        }
    }
}

#[test]
fn vertex_normals_point_away_from_the_solid() {
    let size = 24;
    let (buffer, isolevel) = test_maps::buffer(test_maps::sphere(size, 8.0));
    let centre = Vec3::splat((size as f32 - 1.0) / 2.0);

    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        let surface = extract(&ExtractionJob {
            buffer: buffer.clone(),
            isolevel,
            method,
            interpolate: true,
        });

        let inward = surface
            .positions
            .iter()
            .zip(&surface.normals)
            .filter(|(p, n)| n.dot(**p - centre) <= 0.0)
            .count();
        assert!(!surface.normals.is_empty(), "{method:?} produced nothing");
        assert_eq!(inward, 0, "{method:?}: {inward} normals point into the ball");
    }
}

/// The gradient is the smooth normal of the same surface the triangles
/// approximate, so at every corner it has to lean the same way as the face.
/// A sign error or a swapped corner would flip or scramble it.
#[test]
fn gradient_normals_agree_with_their_faces() {
    for (name, field) in [
        ("sphere(16)", test_maps::sphere(16, 5.0)),
        ("sphere(24)", test_maps::sphere(24, 8.0)),
        ("sphere(MAX_SIZE)", test_maps::sphere(MAX_SIZE, 24.0)),
        ("solid_cube", test_maps::solid_cube(8)),
        ("flat_slab", test_maps::flat_slab(6)),
    ] {
        let (buffer, isolevel) = test_maps::buffer(field);
        let surface = flying_edges::extract(&buffer, isolevel, true);

        let mut disagreeing = 0;
        for t in surface.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| surface.positions[t[k] as usize]);
            let Some(face) = super::face_normal(a, b, c) else {
                continue;
            };
            disagreeing += t
                .iter()
                .filter(|&&i| face.dot(surface.normals[i as usize]) <= 0.0)
                .count();
        }
        assert_eq!(disagreeing, 0, "{name}: {disagreeing} corners face against their triangle");
    }
}

/// On the buffer's faces the gradient has no sample beyond to read, and has
/// to fall back to a one-sided difference rather than read out of bounds or
/// lean sideways. A slab spans the whole buffer, so every vertex it has sits
/// on a column that reaches those faces -- and its normal is exactly up.
#[test]
fn a_slab_has_exactly_vertical_normals() {
    let (buffer, isolevel) = test_maps::buffer(test_maps::flat_slab(6));
    let surface = flying_edges::extract(&buffer, isolevel, true);

    assert!(!surface.normals.is_empty());
    for n in &surface.normals {
        assert_eq!(*n, Vec3::Y);
    }
}

/// Where the gradient cancels, the normal falls back to the edge direction,
/// from the solid end to the empty end -- never to NaN.
#[test]
fn a_cancelled_gradient_falls_back_to_the_edge() {
    let (buffer, isolevel) = test_maps::buffer(gradient_tie());
    let surface = flying_edges::extract(&buffer, isolevel, true);

    let mut at_tie: Vec<Vec3> = surface
        .positions
        .iter()
        .zip(&surface.normals)
        .filter(|(p, _)| **p == Vec3::splat(2.0))
        .map(|(_, n)| *n)
        .collect();
    at_tie.sort_by(|a, b| a.x.total_cmp(&b.x));

    // One vertex from the solid sample on each side.
    assert_eq!(at_tie, vec![-Vec3::X, Vec3::X]);
}

/// Marching cubes' three vertices per triangle all carry that triangle's own
/// face normal, which is exactly what makes it read as flat panels.
#[test]
fn marching_cubes_normals_are_face_normals() {
    for (name, field) in normal_fields() {
        let (buffer, isolevel) = test_maps::buffer(field);
        let surface = marching_cubes::extract(&buffer, isolevel, true);

        for (i, t) in surface.indices.chunks(3).enumerate() {
            let [a, b, c] = [0, 1, 2].map(|k| surface.positions[t[k] as usize]);
            let [na, nb, nc] = [0, 1, 2].map(|k| surface.normals[t[k] as usize]);

            assert!(na == nb && nb == nc, "{name}: triangle {i} has mixed normals");
            if let Some(face) = super::face_normal(a, b, c) {
                assert_eq!(na, face, "{name}: triangle {i} normal is not its face's");
            }
        }
    }
}

/// `place_vertex` is split into `edge_t` and `place_at` so flying edges can
/// reuse `t` for the gradient; the split must not move a single vertex.
#[test]
fn place_at_lands_where_the_longhand_formula_does() {
    let (buffer, isolevel) = test_maps::buffer(test_maps::pseudo_random(8, 0xABCD));
    let cells = buffer.cells_per_axis();

    for z in 0..cells {
        for y in 0..cells {
            for x in 0..cells {
                let corners = buffer.cell_corners(x, y, z);
                let cell = vec3(x as f32, y as f32, z as f32);

                for (edge, &(a, b)) in EDGE_VERTEX_INDICES.iter().enumerate() {
                    if (corners[a] <= isolevel) == (corners[b] <= isolevel) {
                        continue;
                    }
                    let (start, end) = (VERTEX_POSITIONS[a], VERTEX_POSITIONS[b]);
                    let (from, to) = (corners[a] as f32, corners[b] as f32);
                    let longhand = cell + (start + (isolevel as f32 - from) * (end - start) / (to - from));

                    assert_eq!(place_vertex(cell, edge, &corners, isolevel, true), longhand);
                    assert_eq!(
                        place_vertex(cell, edge, &corners, isolevel, false),
                        cell + (start + end) / 2.0,
                    );
                }
            }
        }
    }
}

#[test]
fn triangle_count_matches_the_triangle_table() {
    for (case, edges) in TRIANGLE_TABLE.iter().enumerate() {
        let listed = edges.iter().take_while(|&&e| e != -1).count();
        assert_eq!(listed % 3, 0, "case {case} lists a partial triangle");
        assert_eq!(
            TRIANGLE_COUNT[case] as usize,
            listed / 3,
            "TRIANGLE_COUNT disagrees with TRIANGLE_TABLE at case {case}",
        );
    }
}

#[test]
fn every_edge_in_the_triangle_table_is_cut_by_its_case() {
    // EDGE_INTERSECTION drives every vertex count and ID step in flying edges,
    // so it has to mark exactly the edges the case actually uses.
    for (case, edges) in TRIANGLE_TABLE.iter().enumerate() {
        let cut = super::tables::EDGE_INTERSECTION[case];
        for &edge in edges.iter().take_while(|&&e| e != -1) {
            let bit = super::tables::ORDER[edge as usize];
            assert!(
                cut >> bit & 1 == 1,
                "case {case} uses edge {edge} but EDGE_INTERSECTION does not mark bit {bit}",
            );
        }
    }
}

#[test]
fn edge_axis_origin_agrees_with_the_vertex_indices() {
    for edge in 0..12 {
        let (axis, ox, oy, oz) = super::tables::EDGE_AXIS_ORIGIN[edge];
        let (a, b) = EDGE_VERTEX_INDICES[edge];
        let (pa, pb) = (VERTEX_POSITIONS[a], VERTEX_POSITIONS[b]);

        let origin = vec3(ox as f32, oy as f32, oz as f32);
        assert_eq!(origin, pa.min(pb), "edge {edge} origin");

        let delta = (pb - pa).abs();
        assert_eq!(delta, Vec3::AXES[axis], "edge {edge} axis");
    }
}

/// The per-pass smoke tests, kept for eyeballing intermediate state while
/// working on flying edges.
mod passes {
    use super::*;

    #[test]
    fn passes_run_over_the_terrain_field() {
        let (buffer, isolevel) = test_maps::buffer(test_maps::terrain());
        let surface = flying_edges::extract(&buffer, isolevel, true);
        assert!(surface.triangle_count() > 0);
    }

    #[test]
    fn both_methods_agree_on_the_triangle_count() {
        let (buffer, isolevel) = test_maps::buffer(test_maps::terrain());
        let fe = flying_edges::extract(&buffer, isolevel, true);
        let mc = marching_cubes::extract(&buffer, isolevel, true);

        assert_eq!(fe.triangle_count(), mc.triangle_count(), "triangle count");
        // The vertex counts are *supposed* to differ: only flying edges welds.
        assert_eq!(
            mc.vertex_count(),
            mc.triangle_count() * 3,
            "marching cubes shared a vertex",
        );
    }

    /// What welding is worth, and whether flying edges gets it exactly right.
    ///
    /// Marching cubes emits every triangle corner separately, so the number of
    /// *distinct* positions it produces is, by definition, the number of cut
    /// edges the surface uses -- which is exactly how many vertices flying
    /// edges should allocate. Weld one edge too few and the counts diverge;
    /// weld one too many and two surfaces get stitched together.
    ///
    /// Without interpolation, so that distinct edges cannot land on the same
    /// point: a sample sitting exactly on the isolevel interpolates onto a grid
    /// corner, and two edges sharing that corner would then coincide.
    #[test]
    fn flying_edges_welds_exactly_the_shared_vertices() {
        for field in [
            test_maps::terrain(),
            test_maps::sphere(16, 5.0),
            test_maps::pseudo_random(8, 0x5EED),
        ] {
            let (buffer, isolevel) = test_maps::buffer(field);
            let fe = flying_edges::extract(&buffer, isolevel, false);
            let mc = marching_cubes::extract(&buffer, isolevel, false);

            let mut distinct: Vec<[i32; 3]> = mc
                .positions
                .iter()
                .map(|v| [quant(v.x), quant(v.y), quant(v.z)])
                .collect();
            distinct.sort();
            distinct.dedup();

            assert_eq!(
                fe.vertex_count(),
                distinct.len(),
                "flying edges welded {} vertices out of {} triangle corners, \
                 but only {} distinct positions exist",
                fe.vertex_count(),
                mc.vertex_count(),
                distinct.len(),
            );
        }
    }
}
