use bevy::{color::palettes::css::*, prelude::*};

use crate::voxel_terrain::{
    terrain::{Terrain, SIZE_FOR_NOW},
    voxels::grid::grid::Grid,
};

pub fn init(
    mut commands: Commands,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    terrain: Res<Terrain>,
) {
    let mut gizmo = GizmoAsset::new();

    let chunks = terrain.grids.keys();
    gizmo.cuboid(Transform::from_translation(vec3(0.,0.,0.)), WHITE);
    for chunk in chunks {
        let grid = terrain.grids.get(chunk).unwrap();

        for x in 0..terrain.size {
            for y in 0..terrain.size {
                for z in 0..terrain.size {
                    let value = grid.get(x, y, z).copied().unwrap_or(0);

                    let color = if value > 0 { GREEN } else { CRIMSON };

                    gizmo.sphere(
                        vec3(x as f32, y as f32, z as f32) * SIZE_FOR_NOW,
                        0.5,
                        color,
                    );
                }
            }
        }
    }

    let handle = gizmo_assets.add(gizmo);
    commands.spawn((
        Gizmo {
            handle: handle,
            line_config: GizmoLineConfig {
                width: 5.,
                ..default()
            },
            ..default()
        },
        Transform::from_xyz(4., 1., 0.),
    ));
}
