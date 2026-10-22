//! Benchmarks de worldgen P0.0/P0.1.
//!
//! `cargo bench --bench worldgen -- --sample-size 10`
//!
//! `larion_column_v1` fija la linea base historica; `larion_column_v2` mide el
//! coste de la hidrologia. `hydro_window_build` es el coste amortizado de
//! construir una ventana y `hydro_sample` el de muestrearla.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use solaria_voxel::world::worldgen::larion::{HydroWindow, LarionConfig, LarionGenerator};
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
}

criterion_group!(benches, bench_worldgen);
criterion_main!(benches);
