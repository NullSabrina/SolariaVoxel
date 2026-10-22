//! Benchmarks de worldgen P0.0/P0.1.
//!
//! `cargo bench --bench worldgen -- --sample-size 10`
//!
//! `larion_column_v1` fija la linea base historica; `larion_column_v2` mide el
//! coste de la hidrologia. `hydro_window_build` es el coste amortizado de
//! construir una ventana y `hydro_sample` el de muestrearla.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use solaria_voxel::world::worldgen::larion::{
    Fractal2D, HydroWindow, LarionConfig, LarionGenerator, ScalarField2D, SimdFractal2D,
};
use solaria_voxel::world::worldgen::{Species, TreePlacer};
use solaria_voxel::world::{GeneratorKind, TerrainGenerator};

fn bench_worldgen(c: &mut Criterion) {
    let v1 = TerrainGenerator::with_kind_and_config(13_371, GeneratorKind::Larion, 1);
    let v2 = TerrainGenerator::with_kind_and_config(13_371, GeneratorKind::Larion, 2);

    let mut columns = c.benchmark_group("larion_column");
    columns.sample_size(20);
    columns.bench_function("v1", |b| b.iter(|| black_box(v1.generate_column(0, 0))));
    columns.bench_function("v2", |b| b.iter(|| black_box(v2.generate_column(0, 0))));
    columns.finish();

    let generator = LarionGenerator::with_version(13_371, LarionConfig::default(), 2);
    let window = HydroWindow::build(&generator, (0, 0));

    let mut hydro = c.benchmark_group("hydro");
    hydro.sample_size(10);
    hydro.bench_function("window_build", |b| {
        b.iter(|| black_box(HydroWindow::build(&generator, (0, 0))))
    });
    hydro.bench_function("sample", |b| {
        b.iter(|| black_box(window.sample(123.0, -77.0)))
    });
    hydro.finish();

    // --- P0.3: colocacion y forma de arboles. ---
    let trees = TreePlacer::new(13_371);

    let mut poisson = c.benchmark_group("poisson_candidate");
    poisson.sample_size(50);
    poisson.bench_function("maybe_64x64", |b| {
        b.iter(|| {
            let mut hits = 0u32;
            for z in 0..64 {
                for x in 0..64 {
                    if trees.maybe_organic(x, z) {
                        hits += 1;
                    }
                }
            }
            black_box(hits)
        })
    });
    poisson.finish();

    let mut placement = c.benchmark_group("tree_placement");
    placement.sample_size(20);
    placement.bench_function("organic_48x48", |b| {
        b.iter(|| {
            let mut n = 0u32;
            for z in 0..48 {
                for x in 0..48 {
                    if trees
                        .plan_organic(x, z, solaria_voxel::world::Biome::Forest, 70, 0, 0.8)
                        .is_some()
                    {
                        n += 1;
                    }
                }
            }
            black_box(n)
        })
    });
    placement.finish();

    let mut sca = c.benchmark_group("space_colonization");
    sca.sample_size(50);
    sca.bench_function("oak", |b| {
        b.iter(|| black_box(trees.shape_for_bench(123, -45, Species::Oak, 0xDEAD_BEEF)))
    });
    sca.finish();

    // --- P1.1: lote de ruido escalar (legacy) vs SIMD (quick-noise). ---
    let n = 18 * 18;
    let xs: Vec<f64> = (0..n).map(|i| (i % 18) as f64 - 9.0).collect();
    let zs: Vec<f64> = (0..n).map(|i| (i / 18) as f64 - 9.0).collect();
    let legacy = Fractal2D::new(13371, 4, 1.0 / 1200.0);
    let simd = SimdFractal2D::new(13371, 4, 1.0 / 1200.0);
    let mut out = vec![0.0f32; n];

    let mut noise = c.benchmark_group("noise_sample_many");
    noise.sample_size(50);
    noise.bench_function("legacy_324", |b| {
        b.iter(|| {
            for i in 0..n {
                out[i] = legacy.sample(xs[i], zs[i]) as f32;
            }
            black_box(out.iter().copied().sum::<f32>())
        })
    });
    noise.bench_function("simd_324", |b| {
        b.iter(|| {
            simd.sample_many(&xs, &zs, &mut out);
            black_box(out.iter().copied().sum::<f32>())
        })
    });
    noise.finish();
}

criterion_group!(benches, bench_worldgen);
criterion_main!(benches);
