//! Preview offline del generador de mundo (auditoria de worldgen, FASE 9 PARCIAL).
//!
//! Genera mapas de un area grande sin arrancar el juego ni tocar la GPU, para
//! **equilibrar el generador con datos** en vez de a ojo: exporta un PNG
//! (color por clase continental + gradiente de altura) y una tabla de metricas.
//!
//! Uso:
//! ```text
//! cargo run --release --example worldgen_preview -- [seed] [pixels] [blocks_per_pixel]
//! cargo run --release --example worldgen_preview -- 13371 512 4
//! ```
//! Salidas en `screenshots/worldgen_preview_<seed>.png`.

use std::io::BufWriter;

use solaria_voxel::world::TerrainGenerator;
use solaria_voxel::world::terrain::SEA_LEVEL;
use solaria_voxel::world::worldgen::LandClass;

fn arg_or(args: &[String], i: usize, default: i64) -> i64 {
    args.get(i)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg_or(&args, 1, 13_371) as u32;
    let pixels = arg_or(&args, 2, 512).clamp(16, 4096) as usize;
    let step = arg_or(&args, 3, 4).clamp(1, 64) as i32;

    let span = pixels as i32 * step;
    let origin = -span / 2;
    let generator = TerrainGenerator::new(seed);

    let mut img = vec![0u8; pixels * pixels * 4];
    let (mut n_ocean, mut n_land, mut n_deep) = (0u64, 0u64, 0u64);
    let (mut h_min, mut h_max, mut h_sum) = (i32::MAX, i32::MIN, 0i64);
    let mut cells: std::collections::HashSet<u64> = std::collections::HashSet::new();

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

            let color = if s.land.is_ocean() {
                // Azul mas oscuro a mas profundidad.
                let d = ((SEA_LEVEL as f32 - h as f32) / 64.0).clamp(0.0, 1.0);
                [
                    (30.0 + 20.0 * (1.0 - d)) as u8,
                    (70.0 + 60.0 * (1.0 - d)) as u8,
                    (150.0 + 90.0 * (1.0 - d)) as u8,
                ]
            } else {
                // Verde -> marron -> blanco segun altura.
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
        "[preview] seed={seed} area={span}x{span} bloques ({pixels}x{pixels} px, step {step})"
    );
    println!(
        "[preview] oceano {:.1}% (abisal {:.1}%) | tierra {:.1}%",
        100.0 * (n_ocean + n_deep) as f64 / total,
        100.0 * n_deep as f64 / total,
        100.0 * n_land as f64 / total
    );
    println!("[preview] altura min/avg/max = {h_min}/{avg:.1}/{h_max} (nivel del mar {SEA_LEVEL})");
    println!("[preview] celdas de bioma distintas: {}", cells.len());

    let path = format!("screenshots/worldgen_preview_{seed}.png");
    std::fs::create_dir_all("screenshots").ok();
    let file = std::fs::File::create(&path).expect("crear PNG");
    let mut enc = png::Encoder::new(BufWriter::new(file), pixels as u32, pixels as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("cabecera PNG");
    writer.write_image_data(&img).expect("datos PNG");
    println!("[preview] escrito {path}");
}
