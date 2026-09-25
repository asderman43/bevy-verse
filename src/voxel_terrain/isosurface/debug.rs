//! Demo scene for the extractor: queues the same field through both methods
//! and spawns them side by side, with gizmos showing the underlying samples.

use bevy::{
    color::palettes::css::{BLUE, GREEN, RED, SLATE_GRAY},
    math::vec3,
    prelude::*,
};

use super::{
    test_maps, ExtractSet, ExtractionJob, IsosurfaceQueue, IsosurfaceResults, Method, VoxelBuffer,
};
use crate::voxel_terrain::util::XYZ;

/// Which field the demo shows, and at what isolevel.
fn demo_field() -> (VoxelBuffer, i8) {
    test_maps::buffer(test_maps::solid_cube(4))
}

/// Spawns the demo scene. Expects [`super::IsosurfacePlugin`] to be added too.
pub struct IsosurfaceDebugPlugin;

impl Plugin for IsosurfaceDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (queue_demo_surfaces, draw_sample_gizmos))
            // Results land during ExtractSet, so collect after it to pick them
            // up in the same frame they were produced.
            .add_systems(Update, spawn_extracted_surfaces.after(ExtractSet));
    }
}

/// Queues the demo field through both methods, so they can be compared.
fn queue_demo_surfaces(mut queue: ResMut<IsosurfaceQueue>) {
    let (buffer, isolevel) = demo_field();

    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        queue.push(ExtractionJob {
            buffer: buffer.clone(),
            isolevel,
            method,
            interpolate: true,
        });
    }
}

/// Turns finished surfaces into renderable entities, one column per method.
fn spawn_extracted_surfaces(
    mut commands: Commands,
    mut results: ResMut<IsosurfaceResults>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if results.is_empty() {
        return;
    }

    let spacing = demo_field().0.size() as f32 + 2.0;

    for result in results.drain() {
        let (offset, colour) = match result.method {
            Method::FlyingEdges => (0.0, Color::srgb(1.0, 0.0, 0.0)),
            Method::MarchingCubes => (spacing, Color::srgb(0.0, 0.4, 1.0)),
        };

        info!(
            "{:?}: {} triangles, {} vertices (interpolate: {})",
            result.method,
            result.surface.triangle_count(),
            result.surface.vertex_count(),
            result.interpolate,
        );

        commands.spawn((
            Mesh3d(meshes.add(result.surface.into_mesh())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: colour,
                ..default()
            })),
            Transform::from_translation(Vec3::X * offset),
        ));
    }
}

/// Draws the sample grid: the field's bounds, the axes, and a sphere per
/// sample coloured by whether it is solid.
fn draw_sample_gizmos(mut gizmo_assets: ResMut<Assets<GizmoAsset>>, mut commands: Commands) {
    let (buffer, isolevel) = demo_field();
    let size = buffer.size();

    let mut gizmo = GizmoAsset::new();

    for (from, to) in bounding_box(size) {
        gizmo.line(from, to, SLATE_GRAY);
    }

    let origin = Vec3::splat(-0.1);
    gizmo.arrow(origin, origin + Vec3::X, RED);
    gizmo.arrow(origin, origin + Vec3::Y, GREEN);
    gizmo.arrow(origin, origin + Vec3::Z, BLUE);

    for (x, y, z) in XYZ::new(size) {
        // `> isolevel` is solid; see the module docs.
        let solid = buffer.get(x, y, z) > isolevel;
        let colour = if solid { GREEN } else { RED };
        gizmo.sphere(vec3(x as f32, y as f32, z as f32), 0.1, colour);
    }

    commands.spawn((
        Gizmo {
            handle: gizmo_assets.add(gizmo),
            line_config: GizmoLineConfig {
                width: 2.0,
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
    ));
}

/// The 12 wireframe edges of the box spanning the origin to `size - 1`.
fn bounding_box(size: usize) -> Vec<(Vec3, Vec3)> {
    let n = size.saturating_sub(1) as f32;

    let corners = [
        vec3(0.0, 0.0, 0.0),
        vec3(n, 0.0, 0.0),
        vec3(n, n, 0.0),
        vec3(0.0, n, 0.0),
        vec3(0.0, 0.0, n),
        vec3(n, 0.0, n),
        vec3(n, n, n),
        vec3(0.0, n, n),
    ];
    let edges = [
        (0, 1), (1, 2), (2, 3), (3, 0), // near face
        (4, 5), (5, 6), (6, 7), (7, 4), // far face
        (0, 4), (1, 5), (2, 6), (3, 7), // connecting
    ];

    edges
        .iter()
        .map(|&(a, b)| (corners[a], corners[b]))
        .collect()
}
