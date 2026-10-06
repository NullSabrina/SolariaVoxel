//! Benchmarks reproducibles (FASE 13).
//!
//! Son `#[test]` que **imprimen** tiempos (ejecutar con `--nocapture`); no
//! afirman umbrales absolutos (dependen de la maquina), pero dan una linea base
//! estable para comparar antes/despues de una optimizacion. Los resultados se
//! recopilan en `docs/performance.md`.
//!
//! Uso:
//! ```text
//! cargo test --release -- --nocapture bench_
//! ```

use std::time::Instant;

use super::block::Block;
use super::chunk::{Column, SECTION_COUNT};
use super::save::{ChunkPos, ChunkRecord, WorldSave};
use super::store::World;
use super::terrain::TerrainGenerator;

const SEED: u32 = 13_371;

#[test]
fn bench_terrain_column() {
    let generator = TerrainGenerator::new(SEED);
    let n = 32;
    let t = Instant::now();
    for i in 0..n {
        let column: Box<Column> = Box::new(generator.generate_column(i * 16, 0));
        std::hint::black_box(&column);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!("[bench] terrain_generate_column: {ms:.3} ms/columna ({n} columnas)");
}

#[test]
fn bench_greedy_section() {
    let generator = TerrainGenerator::new(SEED);
    let column = generator.generate_column(0, 0);
    let mut sections = 0usize;
    let t = Instant::now();
    for section in 0..SECTION_COUNT {
        if column.section_is_empty(section) {
            continue;
        }
        let mesh = super::greedy::greedy_section(&column, section, [0.0; 3]);
        std::hint::black_box(&mesh);
        sections += 1;
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] mesh_greedy_section: {ms:.3} ms para {sections} secciones no vacias");
}

#[test]
fn bench_light_incremental() {
    let mut world = World::new(SEED, 2, vec![]);
    world.warm_streaming([8.0, 80.0, 8.0]);
    // Aseguramos un hueco de aire alrededor de (8, 80, 8).
    for d in -1..=1 {
        world.set_block([8 + d, 80, 8], Block::Air);
    }
    let n = 200;
    let t = Instant::now();
    for i in 0..n {
        // Alterna antorcha/aire: ejercita add + remove incrementales.
        let b = if i % 2 == 0 { Block::Torch } else { Block::Air };
        world.set_block([8, 80, 8], b);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!("[bench] lighting_incremental (antorcha add/remove): {ms:.4} ms/edicion");
}

#[test]
fn bench_fluid_tick() {
    let mut world = World::new(SEED, 0, vec![]);
    world.warm_streaming([8.0, 120.0, 8.0]);
    for z in 0..16 {
        for x in 0..16 {
            world.set_block([x, 100, z], Block::Stone);
        }
    }
    world.set_block([8, 101, 8], Block::Water);
    let n = 20;
    let t = Instant::now();
    for _ in 0..n {
        world.tick_water(100_000);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!("[bench] fluid_tick (charca 16x16): {ms:.3} ms/tick");
}

#[test]
fn bench_save_load_column() {
    let generator = TerrainGenerator::new(SEED);
    let column = generator.generate_column(0, 0);
    let record = ChunkRecord::from_column(&column);

    let n = 20;
    let t = Instant::now();
    for _ in 0..n {
        let r = ChunkRecord::from_column(&column);
        std::hint::black_box(&r);
    }
    let encode_ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;

    let path = std::env::temp_dir().join("solaria_bench_column.vf");
    let _ = std::fs::remove_file(&path);
    let mut save = WorldSave::new(SEED, 0);
    save.set_chunk(ChunkPos::new(0, 0), record);
    let t = Instant::now();
    save.save_to(&path).unwrap();
    let save_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let loaded = WorldSave::load_from(&path).unwrap();
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    std::hint::black_box(&loaded);
    let _ = std::fs::remove_file(&path);

    println!(
        "[bench] save_load_column: record={encode_ms:.3} ms | save_to={save_ms:.3} ms | load_from={load_ms:.3} ms"
    );
}
