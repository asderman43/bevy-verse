use std::f64::consts::PI;

use bevy::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    color::palettes::css::{self, DARK_GRAY},
    math::{IVec3, UVec2},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use libnoise::prelude::*;

pub const STEP_EPSILON: f64 = 0.1;

pub fn get_noise(point: IVec3) -> i8 {
    let generator = Source::perlin(42)
        .scale([2., 2.])
        .lambda(|val| (val + 1.) / 2.);
    let (x, z) = (
        (point.x + 15) as f64 * STEP_EPSILON,
        (point.z + 15) as f64 * STEP_EPSILON,
    );

    //1. / (1. + (-y).exp())
    // 1 -> 5, 0 -> 0,
    // 0.8 -> 4, 0.6 -> 3, 0.4 -> 2, 0.2 -> 1

    let val = ((generator.sample([x, z]) + 1.).powi(4) as f32 - point.y as f32).clamp(0., 1.);

    let mapped_value = (val / 0.2) as i8;

    mapped_value
}
#[derive(Resource)]
pub struct NoiseImage(Handle<Image>);
pub const IMAGE_SIZE: u32 = 256;
pub fn noise_preview(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let generator = Source::perlin(42)
        .scale([200., 200.])
        .lambda(|val| (val + 1.) / 2.);
    let mut image = Image::new_fill(
        Extent3d {
            width: IMAGE_SIZE,
            height: IMAGE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &(css::WHITE.to_u8_array()),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    for x in 0..IMAGE_SIZE {
        for y in 0..IMAGE_SIZE {
            let per = ((x) as f64 * STEP_EPSILON, (y) as f64 * STEP_EPSILON);
            let value = generator.sample([per.0, per.1]) as f32;

            if let Err(e) = image.set_color_at(x, y, Color::srgb(value, value, value)) {
                dbg!(e);
            }
        }
    }
    let handle = images.add(image);

    // create a sprite entity using our image
    //commands.spawn((Sprite::from_image(handle.clone()), Transform::from_xyz(-500., 500., 0.)));

    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn(Node {
                    left: Val::Px(210.),
                    bottom: Val::Px(10.),
                    width: Val::Px(256.),
                    height: Val::Px(256.),
                    position_type: PositionType::Absolute,
                    ..Default::default()
                })
                .with_children(|parent| {
                    parent.spawn((
                        ImageNode::new(handle.clone()),
                        // Uses the transform to rotate the logo image by 45 degrees
                        Outline {
                            width: Val::Px(2.),
                            offset: Val::Px(4.),
                            color: DARK_GRAY.into(),
                        },
                    ));
                });
        });

    commands.insert_resource(NoiseImage(handle));
}

#[cfg(test)]
mod tests {
    use bevy::math::IVec3;

    use super::get_noise;
    #[test]
    fn test_noise() {
        for z in 0..3 {
            for x in 0..3 {
                print!("{} ", get_noise(IVec3::new(x, 3, z)));
            }
            println!();
        }
    }
}
