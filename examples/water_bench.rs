//! Timing harness for the water mesher, next to the terrain on the same rock.
//!
//! ```bash
//! cargo run --release --example water_bench
//! ```
//!
//! Each field is a heightfield of rock with water settled over it the way the
//! fill model stores it (`docs/water-surface.md` §1), except the noise field,
//! which is the worst case: water and rock everywhere, unsettled.
//!
//! Release only. The dev profile builds this crate at `opt-level = 1`, which
//! makes the numbers meaningless.

use std::time::{Duration, Instant};

/// The app's allocator, so the numbers match what the app pays. See `main.rs`.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use bevy_verse::voxel_terrain::{
    isosurface::{flying_edges, Surface, VoxelBuffer, S_MAX},
    water,
};

/// Meshings per measurement. Small fields get more, to stay out of the
/// timer's noise floor.
fn runs_for(size: usize) -> usize {
    match size {
        0..=16 => 400,
        17..=32 => 100,
        _ => 25,
    }
}

struct Timing {
    mean: Duration,
    best: Duration,
    surface: Surface,
}

/// Times `runs` calls, after one warm-up that is not counted.
fn time(runs: usize, mut f: impl FnMut() -> Surface) -> Timing {
    let surface = f();
    let mut total = Duration::ZERO;
    let mut best = Duration::MAX;
    for _ in 0..runs {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        std::hint::black_box(&result);
        total += elapsed;
        best = best.min(elapsed);
    }
    Timing {
        mean: total / runs as u32,
        best,
        surface,
    }
}

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

/// How much of sample `y`'s slab a column filled to `units` covers.
fn fill(units: i32, y: usize) -> u8 {
    (units - y as i32 * S_MAX as i32).clamp(0, S_MAX as i32) as u8
}

/// Rolling hills, in fill units: about a third of the way up the chunk, with
/// a couple of cells of relief.
fn hills(size: usize) -> impl Fn(usize, usize) -> i32 {
    let s = size as f32;
    move |x, z| {
        let (u, v) = (x as f32 / s * 6.0, z as f32 / s * 6.0);
        let h = s * 0.33 + 2.2 * (u.sin() + (v * 1.3).cos()) + 0.8 * ((u + v) * 2.1).sin();
        (h * S_MAX as f32) as i32
    }
}

/// Rock filled to `ground`, water settled over it to `level` units.
fn settled(size: usize, ground: impl Fn(usize, usize) -> i32, level: i32) -> (VoxelBuffer, VoxelBuffer) {
    let solid = field(size, |x, y, z| fill(ground(x, z), y));
    let water = field(size, |x, y, z| fill(level.max(ground(x, z)), y) - fill(ground(x, z), y));
    (solid, water)
}

/// Rock and water at random in every sample, `s + w <= S_MAX`.
fn noise(size: usize, seed: u32) -> (VoxelBuffer, VoxelBuffer) {
    let mut state = seed;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 16) as u8
    };
    let mut solid = VoxelBuffer::new(size);
    let mut water = VoxelBuffer::new(size);
    for i in 0..size.pow(3) {
        let s = next() % (S_MAX + 1);
        let w = next() % (S_MAX - s + 1);
        solid.samples_mut()[i] = s;
        water.samples_mut()[i] = w;
    }
    (solid, water)
}

fn bench(name: &str, (solid, water): (VoxelBuffer, VoxelBuffer)) {
    let size = solid.size();
    let runs = runs_for(size);
    let cells = (size - 1).pow(3);
    let wet = water.samples().iter().filter(|&&w| w > 0).count();

    let w = time(runs, || water::extract(&solid, &water, true));
    let t = time(runs, || flying_edges::extract(&solid, true));

    println!(
        "\n{name}  ({size}^3, {cells} cells, {:.1}% of samples wet, {runs} runs)",
        100.0 * wet as f64 / size.pow(3) as f64,
    );
    println!("  {:<22} {:>10} {:>10} {:>12} {:>10}", "", "mean", "best", "triangles", "vertices");
    for (label, timing) in [("water", &w), ("terrain (flying edges)", &t)] {
        println!(
            "  {:<22} {:>10} {:>10} {:>12} {:>10}",
            label,
            format!("{:.3}ms", timing.mean.as_secs_f64() * 1000.0),
            format!("{:.3}ms", timing.best.as_secs_f64() * 1000.0),
            timing.surface.triangle_count(),
            timing.surface.vertex_count(),
        );
    }
    println!(
        "  {:<22} {:.0} ns per cell, water/terrain {:.2}x",
        "",
        w.mean.as_secs_f64() * 1e9 / cells as f64,
        w.mean.as_secs_f64() / t.mean.as_secs_f64(),
    );
}

fn main() {
    println!("water meshing, {}", std::env::consts::ARCH);

    let size = 32;
    let ground = hills(size);
    let lowest = (0..size).flat_map(|x| (0..size).map(move |z| (x, z))).map(|(x, z)| ground(x, z)).min().unwrap();

    // No water: what every dry chunk costs, which is most of them.
    bench("dry hills", settled(size, hills(size), 0));
    // A lake in the valleys, about a cell and a half deep: the common wet case,
    // with plenty of shoreline.
    bench("lake", settled(size, hills(size), lowest + 22));
    // Water over everything: open sea, one quad per cell, no shore.
    bench("flooded", settled(size, hills(size), 20 * S_MAX as i32));
    // Worst case: unsettled rock and water in every sample.
    bench("noise", noise(size, 0x1234_5678));
    // A full-size chunk with a lake.
    let big = 64;
    let ground = hills(big);
    let lowest = (0..big).flat_map(|x| (0..big).map(move |z| (x, z))).map(|(x, z)| ground(x, z)).min().unwrap();
    bench("lake", settled(big, hills(big), lowest + 22));
}
