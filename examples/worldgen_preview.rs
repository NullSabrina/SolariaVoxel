//! Preview offline del generador de mundo (auditoria de worldgen, FASE 9 PARCIAL).
//!
//! Genera mapas de un area grande sin arrancar el juego ni tocar la GPU, para
//! **equilibrar el generador con datos** en vez de a ojo. Exporta un PNG y una
//! tabla de metricas.
//!
//! Uso:
//! ```text
//! cargo run --release --example worldgen_preview -- [seed] [pixels] [blocks_per_pixel] [layer]
//! cargo run --release --example worldgen_preview -- 13371 512 4 biome
//! ```
//! `layer`: `biome` (por defecto, mapa logico de biomas), `height` o `continental`.
//! Salida en `screenshots/worldgen_preview_<seed>_<layer>.png`.

use std::collections::HashMap;
use std::io::BufWriter;

use solaria_voxel::world::TerrainGenerator;
use solaria_voxel::world::terrain::{Biome, SEA_LEVEL};
use solaria_voxel::world::worldgen::LandClass;

fn arg_or(args: &[String], i: usize, default: i64) -> i64 {
    args.get(i)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

fn biome_color(b: Biome) -> [u8; 3] {
    match b {
        Biome::Desert => [225, 205, 130],
        Biome::Savanna => [185, 195, 95],
        Biome::Plains => [110, 180, 90],
        Biome::Forest => [40, 130, 60],
        Biome::Swamp => [70, 105, 70],
        Biome::Taiga => [60, 110, 100],
        Biome::Tundra => [205, 215, 225],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg_or(&args, 1, 13_371) as u32;
    let pixels = arg_or(&args, 2, 512).clamp(16, 4096) as usize;
    let step = arg_or(&args, 3, 4).clamp(1, 64) as i32;
    let layer = args.get(4).cloned().unwrap_or_else(|| "biome".to_string());

    let span = pixels as i32 * step;
    let origin = -span / 2;
    let generator = TerrainGenerator::new(seed);

    let mut img = vec![0u8; pixels * pixels * 4];
    let (mut n_ocean, mut n_land, mut n_deep) = (0u64, 0u64, 0u64);
    let (mut h_min, mut h_max, mut h_sum) = (i32::MAX, i32::MIN, 0i64);
    let mut cells: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut biome_counts: HashMap<Biome, u64> = HashMap::new();

    for py in 0..pixels {
        for px in 0..pixels {
            let x = origin + px as i32 * step;
            let z = origin + py as i32 * step;
            let s = generator.sample(x, z);
            let h = generator.height(x, z);
            cells.insert(s.cell_id);
            match s.land {
                LandClass::DeepOcean => n_deep += 1,
                LandClass::Ocean | LandClass::Shelf => n_ocean += 1,
                _ => n_land += 1,
            }
            h_min = h_min.min(h as i32);
            h_max = h_max.max(h as i32);
            h_sum += h as i64;
            *biome_counts.entry(s.biome).or_insert(0) += 1;

            let color = if s.land.is_ocean() {
                let d = ((SEA_LEVEL as f32 - h as f32) / 64.0).clamp(0.0, 1.0);
                [
                    (30.0 + 20.0 * (1.0 - d)) as u8,
                    (70.0 + 60.0 * (1.0 - d)) as u8,
                    (150.0 + 90.0 * (1.0 - d)) as u8,
                ]
            } else {
                match layer.as_str() {
                    "height" => {
                        let t = ((h as f32 - SEA_LEVEL as f32) / 120.0).clamp(0.0, 1.0);
                        if t < 0.5 {
                            let u = t * 2.0;
                            [
                                (90.0 + 60.0 * u) as u8,
                                (160.0 - 40.0 * u) as u8,
                                (80.0 - 20.0 * u) as u8,
                            ]
                        } else {
                            let u = (t - 0.5) * 2.0;
                            [
                                (150.0 + 100.0 * u) as u8,
                                (120.0 + 130.0 * u) as u8,
                                (60.0 + 190.0 * u) as u8,
                            ]
                        }
                    }
                    "continental" => {
                        let c = s.continentalness;
                        if c < 0.0 {
                            let d = (-c).clamp(0.0, 1.0);
                            [(180.0 - 120.0 * d) as u8, (200.0 - 120.0 * d) as u8, 255]
                        } else {
                            let d = c.clamp(0.0, 1.0);
                            [255, (200.0 - 140.0 * d) as u8, (180.0 - 140.0 * d) as u8]
                        }
                    }
                    // "river": bioma sombreado + cauces y lagos en azul.
                    "river" => {
                        let base = biome_color(s.biome);
                        let shade =
                            0.8 + 0.4 * ((h as f32 - SEA_LEVEL as f32) / 120.0).clamp(0.0, 1.0);
                        let land = [
                            base[0] as f32 * shade,
                            base[1] as f32 * shade,
                            base[2] as f32 * shade,
                        ];
                        let water = s.surface_water > s.base_height + 0.5;
                        if water {
                            let t = s.river_proximity.clamp(0.0, 1.0).max(0.35);
                            [
                                (land[0] * (1.0 - t) + 40.0 * t) as u8,
                                (land[1] * (1.0 - t) + 110.0 * t) as u8,
                                (land[2] * (1.0 - t) + 220.0 * t) as u8,
                            ]
                        } else {
                            [
                                land[0].min(255.0) as u8,
                                land[1].min(255.0) as u8,
                                land[2].min(255.0) as u8,
                            ]
                        }
                    }
                    // "biome": color del bioma sombreado por altura.
                    _ => {
                        let base = biome_color(s.biome);
                        let shade =
                            0.8 + 0.4 * ((h as f32 - SEA_LEVEL as f32) / 120.0).clamp(0.0, 1.0);
                        [
                            (base[0] as f32 * shade).min(255.0) as u8,
                            (base[1] as f32 * shade).min(255.0) as u8,
                            (base[2] as f32 * shade).min(255.0) as u8,
                        ]
                    }
                }
            };
            let i = (py * pixels + px) * 4;
            img[i] = color[0];
            img[i + 1] = color[1];
            img[i + 2] = color[2];
            img[i + 3] = 255;
        }
    }

    let total = (pixels * pixels) as f64;
    let avg = h_sum as f64 / total;
    println!(
        "[preview] seed={seed} area={span}x{span} bloques ({pixels}x{pixels} px, step {step}, layer {layer})"
    );
    println!(
        "[preview] oceano {:.1}% (abisal {:.1}%) | tierra {:.1}%",
        100.0 * (n_ocean + n_deep) as f64 / total,
        100.0 * n_deep as f64 / total,
        100.0 * n_land as f64 / total
    );
    println!("[preview] altura min/avg/max = {h_min}/{avg:.1}/{h_max} (nivel del mar {SEA_LEVEL})");
    println!("[preview] celdas de bioma distintas: {}", cells.len());
    let mut bioc: Vec<(&Biome, &u64)> = biome_counts.iter().collect();
    bioc.sort_by(|a, b| b.1.cmp(a.1));
    print!("[preview] biomas:");
    for (b, n) in bioc {
        print!(" {b:?} {:.1}%", 100.0 * *n as f64 / total);
    }
    println!();

    let path = format!("screenshots/worldgen_preview_{seed}_{layer}.png");
    std::fs::create_dir_all("screenshots").ok();
    let file = std::fs::File::create(&path).expect("crear PNG");
    let mut enc = png::Encoder::new(BufWriter::new(file), pixels as u32, pixels as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("cabecera PNG");
    writer.write_image_data(&img).expect("datos PNG");
    println!("[preview] escrito {path}");
}
