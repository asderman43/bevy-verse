use bevy::{
    dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin},
    prelude::*,
    text::FontSmoothing,
};
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_verse::voxel_terrain::{debug::VoxelTerrainDebugPlugin, VoxelTerrainPlugin};

mod prototype;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            prototype::PrototypePlugin,
            VoxelTerrainPlugin,
            FpsOverlayPlugin {
                config: FpsOverlayConfig {
                    text_config: TextFont {
                        // Here we define size of our overlay
                        font_size: 42.0,
                        // If we want, we can use a custom font
                        font: Default::default(),
                        // We could also disable font smoothing,
                        font_smoothing: FontSmoothing::default(),
                        ..Default::default()
                    },
                    // We can also change color of the overlay
                    text_color: Color::srgb(0.0, 1.0, 0.0),
                    enabled: true,
                    ..Default::default()
                },
            },
            PanOrbitCameraPlugin,
        ))
        .run();
}
