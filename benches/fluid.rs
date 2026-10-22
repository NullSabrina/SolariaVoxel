//! Benchmarks de la simulacion de agua y del pase de cascadas.
//!
//! `cargo bench --bench fluid -- --sample-size 10`
//!
//! * `fluid_tick_column` — coste de procesar **una** celda (`water::step_cell`).
//! * `fluid_tick_region_64` — coste de un tick completo sobre una region de
//!   64x64 columnas (4 096 celdas), el mismo presupuesto que
//!   `MAX_CELLS_PER_TICK` en `FluidBudget::default()`.
//! * `waterfall_pass` — coste de construir una `HydroWindow` (incluye el nuevo
//!   campo `waterfall` y su suavizado). No se puede aislar sin exponer la
//!   funcion interna, asi que se reporta el build completo.
//!
//! Los dos primeros usan una rejilla en memoria (sin GPU ni chunks) para medir
//! solo el automata; el tercero usa el generador real con semilla fija.

use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use solaria_voxel::world::water::{Fluid, FluidGrid, MAX_LEVEL, step_cell};
use solaria_voxel::world::worldgen::larion::{HydroWindow, LarionConfig, LarionGenerator};

/// Rejilla plana en memoria: suelo solido en `y <= 0`.
struct BenchGrid {
    size: i32,
    cells: Vec<Fluid>,
}

impl BenchGrid {
    fn new(size: i32) -> Self {
        Self {
            size,
            cells: vec![Fluid::None; (size * size) as usize],
        }
    }

    #[inline]
    fn index(&self, x: i32, z: i32) -> usize {
        (z * self.size + x) as usize
    }

    fn fill(&mut self, _y: i32, level: Fluid) {
        for z in 0..self.size {
            for x in 0..self.size {
                let i = self.index(x, z);
                self.cells[i] = level;
            }
        }
    }
}

impl FluidGrid for BenchGrid {
    fn in_bounds(&self, p: [i32; 3]) -> bool {
        (1..64).contains(&p[1]) && (0..self.size).contains(&p[0]) && (0..self.size).contains(&p[2])
    }

    fn is_solid(&self, p: [i32; 3]) -> bool {
        p[1] <= 0
    }

    fn fluid(&self, p: [i32; 3]) -> Fluid {
        if !self.in_bounds(p) || p[1] != 1 {
            return Fluid::None;
        }
        self.cells[self.index(p[0], p[2])]
    }

    fn set_fluid(&mut self, p: [i32; 3], f: Fluid) {
        if self.in_bounds(p) && p[1] == 1 {
            let i = self.index(p[0], p[2]);
            self.cells[i] = f;
        }
    }
}

fn bench_fluid(c: &mut Criterion) {
    let mut group = c.benchmark_group("fluid");

    group.bench_function("fluid_tick_column", |b| {
        let mut grid = BenchGrid::new(1);
        grid.fill(1, Fluid::Source);
        b.iter(|| {
            black_box(step_cell(&mut grid, [0, 1, 0]));
        });
    });

    let mut grid = BenchGrid::new(64);
    grid.fill(1, Fluid::Source);
    group.bench_function("fluid_tick_region_64", |b| {
        b.iter_batched(
            || {
                let mut fresh = BenchGrid::new(64);
                fresh.fill(1, Fluid::Flow(MAX_LEVEL));
                fresh
            },
            |mut fresh| {
                for z in 0..64 {
                    for x in 0..64 {
                        black_box(step_cell(&mut fresh, [x, 1, z]));
                    }
                }
            },
            BatchSize::SmallInput,
        );
    });

    let generator = LarionGenerator::with_version(13_371, LarionConfig::default(), 2);
    group.bench_function("waterfall_pass", |b| {
        b.iter(|| black_box(HydroWindow::build(&generator, (0, 0))));
    });

    group.finish();
}

criterion_group!(benches, bench_fluid);
criterion_main!(benches);
