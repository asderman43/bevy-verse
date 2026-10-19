//! One cell of terrain and water, for checking the water mesher case by case.
//!
//! ```text
//! cargo run --example water_cell
//! ```
//!
//! - `←` / `→`: previous / next preset (the 16 surface cases, then the worked
//!   examples of `docs/water-surface.md` §7)
//! - `1`–`8`: select a corner (corner index + 1)
//! - `W` / `S`: more / less rock at the selected corner
//! - `E` / `D`: more / less water at the selected corner
//! - `I`: toggle interpolation, `O`: toggle the shore overlap
//! - drag to orbit, scroll to zoom
//!
//! With `WATER_CELL_SHOTS=<dir>` set, it instead steps through every preset,
//! saves a screenshot of each into `<dir>`, and exits.
//!
//! The terrain is drawn see-through brown, the water blue. Spheres mark the
//! corners (green solid, grey empty, yellow selected) with a blue bar for their
//! water. White markers are the water's own vertices on vertical edges (`V`),
//! orange ones its shoreline points (`F`); cyan lines are the vertical edges
//! carrying the surface.

use bevy::{
    color::palettes::css::{AQUA, BLUE, GRAY, LIME, ORANGE, WHITE, YELLOW},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_verse::voxel_terrain::{
    isosurface::{flying_edges, tables::VERTEX_POSITIONS, Surface, VoxelBuffer, S_MAX},
    water::{
        debug::describe_cell,
        mesher::extract_with_overlap,
        presets::{self, Preset},
        tables::VERT_EDGE,
        SHORE_OVERLAP,
    },
};

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, PanOrbitCameraPlugin))
        .insert_resource(Demo::new())
        .add_systems(Startup, setup)
        .add_systems(Update, (input, rebuild, draw_gizmos, update_labels).chain());

    if let Ok(dir) = std::env::var("WATER_CELL_SHOTS") {
        std::fs::create_dir_all(&dir).expect("can create the screenshot directory");
        // Keep rendering at full rate while the window is in the background.
        app.insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .insert_resource(Shots { dir, frame: 0 })
            .add_systems(Update, take_shots.after(update_labels));
    }
    app.run();
}

/// Screenshot mode: one shot per preset, then exit.
#[derive(Resource)]
struct Shots {
    dir: String,
    frame: u32,
}

fn take_shots(
    mut commands: Commands,
    mut shots: ResMut<Shots>,
    mut demo: ResMut<Demo>,
    mut exit: EventWriter<AppExit>,
) {
    // Give each preset time to rebuild and render before the shot, and the
    // shot time to land before moving on.
    const SETTLE: u32 = 20;
    shots.frame += 1;
    if shots.frame == 1 {
        demo.load(0);
    }
    if shots.frame % SETTLE != 0 {
        return;
    }
    let shot = shots.frame / SETTLE;
    if shot as usize > demo.presets.len() {
        exit.write(AppExit::Success);
        return;
    }
    let name: String = demo
        .preset()
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let path = format!("{}/{:02}_{name}.png", shots.dir, demo.index + 1);
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    let next = demo.index + 1;
    if next < demo.presets.len() {
        demo.load(next);
    }
}

#[derive(Resource)]
struct Demo {
    presets: Vec<Preset>,
    index: usize,
    s: [u8; 8],
    w: [u8; 8],
    selected: usize,
    interpolate: bool,
    overlap: bool,
    dirty: bool,
    /// The last water mesh, for the markers.
    water: Surface,
}

impl Demo {
    fn new() -> Self {
        let presets = presets::all();
        let mut demo = Demo {
            s: presets[0].s,
            w: presets[0].w,
            presets,
            index: 0,
            selected: 0,
            interpolate: true,
            overlap: true,
            dirty: true,
            water: Surface::default(),
        };
        demo.load(15);
        demo
    }

    fn load(&mut self, index: usize) {
        self.index = index % self.presets.len();
        self.s = self.presets[self.index].s;
        self.w = self.presets[self.index].w;
        self.dirty = true;
    }

    fn preset(&self) -> &Preset {
        &self.presets[self.index]
    }

    /// Whether the corners still match the preset, or have been edited.
    fn edited(&self) -> bool {
        self.s != self.preset().s || self.w != self.preset().w
    }

    fn buffers(&self) -> (VoxelBuffer, VoxelBuffer) {
        Preset {
            name: String::new(),
            note: "",
            s: self.s,
            w: self.w,
        }
        .buffers()
    }
}

#[derive(Resource)]
struct Handles {
    terrain: Entity,
    water: Entity,
}

#[derive(Component)]
struct Panel;

#[derive(Component)]
struct CornerLabel(usize);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Transform::from_xyz(2.4, 2.0, 2.8),
        PanOrbitCamera {
            focus: Vec3::splat(0.5),
            ..default()
        },
    ));
    commands.insert_resource(AmbientLight {
        brightness: 600.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 4000.0,
            ..default()
        },
        Transform::from_xyz(1.0, 3.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let terrain = commands
        .spawn((
            Mesh3d(meshes.add(Mesh::from(Cuboid::default()))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.4, 0.25, 0.55),
                alpha_mode: AlphaMode::Opaque,
                cull_mode: None,
                double_sided: true,
                ..default()
            })),
        ))
        .id();
    let water = commands
        .spawn((
            Mesh3d(meshes.add(Mesh::from(Cuboid::default()))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.15, 0.45, 0.95, 0.7),
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            })),
        ))
        .id();
    commands.insert_resource(Handles { terrain, water });

    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        Panel,
    ));
    for c in 0..8 {
        commands.spawn((
            Text::new(""),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            CornerLabel(c),
        ));
    }
}

fn input(keys: Res<ButtonInput<KeyCode>>, mut demo: ResMut<Demo>) {
    if keys.just_pressed(KeyCode::ArrowRight) {
        let next = demo.index + 1;
        demo.load(next);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        let previous = demo.index + demo.presets.len() - 1;
        demo.load(previous);
    }

    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
    ];
    for (c, key) in digits.into_iter().enumerate() {
        if keys.just_pressed(key) {
            demo.selected = c;
        }
    }

    // Rock and water share the slab: s + w <= S_MAX.
    let c = demo.selected;
    let (s, w) = (demo.s[c], demo.w[c]);
    let edit = if keys.just_pressed(KeyCode::KeyW) && s + w < S_MAX {
        Some((s + 1, w))
    } else if keys.just_pressed(KeyCode::KeyS) && s > 0 {
        Some((s - 1, w))
    } else if keys.just_pressed(KeyCode::KeyE) && s + w < S_MAX {
        Some((s, w + 1))
    } else if keys.just_pressed(KeyCode::KeyD) && w > 0 {
        Some((s, w - 1))
    } else {
        None
    };
    if let Some((s, w)) = edit {
        demo.s[c] = s;
        demo.w[c] = w;
        demo.dirty = true;
    }

    if keys.just_pressed(KeyCode::KeyI) {
        demo.interpolate = !demo.interpolate;
        demo.dirty = true;
    }
    if keys.just_pressed(KeyCode::KeyO) {
        demo.overlap = !demo.overlap;
        demo.dirty = true;
    }
}

fn rebuild(
    mut demo: ResMut<Demo>,
    handles: Res<Handles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(&Mesh3d, &mut Visibility)>,
) {
    if !demo.dirty {
        return;
    }
    demo.dirty = false;

    let (solid, water) = demo.buffers();
    let terrain = flying_edges::extract(&solid, demo.interpolate);
    let overlap = if demo.overlap { SHORE_OVERLAP } else { 0.0 };
    let water_surface = extract_with_overlap(&solid, &water, demo.interpolate, overlap);
    demo.water = water_surface.clone();

    for (entity, surface) in [(handles.terrain, terrain), (handles.water, water_surface)] {
        let Ok((mesh, mut visibility)) = query.get_mut(entity) else {
            continue;
        };
        if surface.is_empty() {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Inherited;
        if let Some(target) = meshes.get_mut(&mesh.0) {
            *target = surface.into_mesh();
        }
    }
}

fn draw_gizmos(demo: Res<Demo>, mut gizmos: Gizmos) {
    gizmos.cuboid(Transform::from_translation(Vec3::splat(0.5)), GRAY);

    let (solid, water) = demo.buffers();
    let info = describe_cell(&solid, &water, (0, 0, 0), demo.interpolate);

    for c in 0..8 {
        let p = VERTEX_POSITIONS[c];
        let colour = if c == demo.selected {
            YELLOW
        } else if info.solid >> c & 1 == 1 {
            LIME
        } else {
            GRAY
        };
        gizmos.sphere(p, if c == demo.selected { 0.05 } else { 0.035 }, colour);
        if demo.w[c] > 0 {
            let bar = Vec3::new(0.06, 0.0, 0.06);
            let top = Vec3::Y * (demo.w[c] as f32 / S_MAX as f32) * 0.45;
            gizmos.line(p + bar, p + bar + top, BLUE);
        }
    }

    for k in 0..4 {
        if info.surface_slots >> k & 1 == 1 {
            let (lo, hi) = VERT_EDGE[k];
            gizmos.line(VERTEX_POSITIONS[lo], VERTEX_POSITIONS[hi], AQUA);
        }
    }

    for &p in &demo.water.positions {
        let on_vertical_edge = p.x.fract() == 0.0 && p.z.fract() == 0.0;
        let colour = if on_vertical_edge { WHITE } else { ORANGE };
        gizmos.sphere(p, 0.02, colour);
    }
}

fn update_labels(
    demo: Res<Demo>,
    camera: Query<(&Camera, &GlobalTransform)>,
    mut panel: Query<&mut Text, (With<Panel>, Without<CornerLabel>)>,
    mut labels: Query<(&CornerLabel, &mut Text, &mut Node), Without<Panel>>,
) {
    let (solid, water) = demo.buffers();
    let info = describe_cell(&solid, &water, (0, 0, 0), demo.interpolate);
    let preset = demo.preset();

    if let Ok(mut text) = panel.single_mut() {
        let mask = |m: u8| format!("{m:08b}");
        text.0 = format!(
            "[{}/{}] {}{}\n{}\n\n\
             S (solid)  {}\n\
             O (inside) {}\n\
             M (wet)    {}\n\
             Q (slots)  {:04b}   triangles {}\n\
             interpolate {}   overlap {}\n\n\
             <- -> preset   1-8 corner   W/S rock   E/D water   I interpolate   O overlap",
            demo.index + 1,
            demo.presets.len(),
            preset.name,
            if demo.edited() { " (edited)" } else { "" },
            preset.note,
            mask(info.solid),
            mask(info.inside),
            mask(info.wet),
            info.surface_slots,
            demo.water.triangle_count(),
            demo.interpolate,
            demo.overlap,
        );
    }

    let Ok((camera, transform)) = camera.single() else {
        return;
    };
    for (label, mut text, mut node) in &mut labels {
        let c = label.0;
        let Ok(at) = camera.world_to_viewport(transform, VERTEX_POSITIONS[c]) else {
            continue;
        };
        node.left = Val::Px(at.x + 8.0);
        node.top = Val::Px(at.y - 8.0);
        text.0 = format!("{c}: s {} w {}  b {}", demo.s[c], demo.w[c], info.bottom_fill[c]);
    }
}
