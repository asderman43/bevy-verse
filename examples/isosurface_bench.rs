//! Timing harness for the two extractors.
//!
//! ```bash
//! cargo run --release --example isosurface_bench
//! ```
//!
//! Each field is run four ways: every (method, pooled) pair. "cold" builds a
//! fresh [`ExtractionScratch`] per extraction, which is what allocating every
//! buffer per chunk costs; "pooled" reuses one, which is what the subsystem
//! actually does. The gap between them is the allocation bill.
//!
//! Release only. The dev profile builds this crate at `opt-level = 1`, which
//! makes the numbers meaningless.

use std::time::{Duration, Instant};

use bevy_verse::voxel_terrain::isosurface::{
    extract_with, test_maps, ExtractionJob, ExtractionScratch, Method, VoxelBuffer,
};

/// Extractions per measurement. Small fields get more, to stay out of the
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
    triangles: usize,
    vertices: usize,
}

/// Times `runs` extractions, after one warm-up that is not counted.
fn time(job: &ExtractionJob, pooled: bool, runs: usize) -> Timing {
    let mut scratch = ExtractionScratch::new();

    // Warm up: page in the field, and let a pooled run reach its high-water
    // mark so the measurement is of a steady-state pool, not of its first fill.
    let surface = extract_with(&mut scratch, job);

    let mut total = Duration::ZERO;
    let mut best = Duration::MAX;

    for _ in 0..runs {
        // A cold run pays for its buffers, including dropping the last set.
        let mut cold = ExtractionScratch::new();
        let scratch = if pooled { &mut scratch } else { &mut cold };

        let start = Instant::now();
        let surface = extract_with(scratch, job);
        let elapsed = start.elapsed();

        // Keep the result alive across the timer so the whole call cannot be
        // optimized out.
        std::hint::black_box(&surface);

        total += elapsed;
        best = best.min(elapsed);
    }

    Timing {
        mean: total / runs as u32,
        best,
        triangles: surface.triangle_count(),
        vertices: surface.vertex_count(),
    }
}

fn bench(name: &str, field: (Vec<i8>, i8, usize)) {
    let size = field.2;
    let runs = runs_for(size);
    let (buffer, isolevel) = test_maps::buffer(field);

    println!("\n{name}  ({size}^3, {} cells, {runs} runs)", cells(&buffer));
    println!(
        "  {:<16} {:>10} {:>10} {:>12} {:>10}",
        "", "mean", "best", "triangles", "vertices",
    );

    for method in [Method::FlyingEdges, Method::MarchingCubes] {
        let job = ExtractionJob {
            buffer: buffer.clone(),
            isolevel,
            method,
            interpolate: true,
        };

        let cold = time(&job, false, runs);
        let warm = time(&job, true, runs);

        for (label, t) in [("cold", &cold), ("pooled", &warm)] {
            println!(
                "  {:<16} {:>10} {:>10} {:>12} {:>10}",
                format!("{method:?}/{label}"),
                format!("{:.3}ms", t.mean.as_secs_f64() * 1000.0),
                format!("{:.3}ms", t.best.as_secs_f64() * 1000.0),
                t.triangles,
                t.vertices,
            );
        }

        let saved = cold.mean.as_secs_f64() / warm.mean.as_secs_f64();
        println!("  {:<16} pooling is {saved:.2}x", "");
    }
}

fn cells(buffer: &VoxelBuffer) -> usize {
    buffer.cells_per_axis().pow(3)
}

fn main() {
    println!("isosurface extraction, {} ", std::env::consts::ARCH);

    // Mostly-empty with a solid core: what the trim exists for, and the shape
    // real terrain chunks are closest to.
    bench("sphere", test_maps::sphere(64, 24.0));
    bench("sphere", test_maps::sphere(32, 12.0));
    // Dense noise: near-worst case, a surface through almost every cell.
    bench("pseudo_random", test_maps::pseudo_random(32, 0x1234_5678));
    // A slab, and a small chunk of the size the terrain system uses.
    bench("flat_slab", test_maps::flat_slab(32));
    bench("solid_cube", test_maps::solid_cube(16));
}
