//! Preview offline del **grafo de densidad** (Parte C, C6).
//!
//! Sin GPU ni juego: compila el grafo por defecto y exporta un PNG con el mapa de
//! **altura** (izquierda) y un **corte vertical de densidad** (derecha), mas
//! metricas. Sirve para equilibrar el worldgen data-driven con datos.
//!
//! Uso:
//! ```text
//! cargo run --release --example graph_preview -- [seed] [px] [bloques_por_px]
//! ```
//! Salida en `screenshots/graph_preview_<seed>.png`.

use std::io::BufWriter;

use solaria_voxel::world::worldgen::graph::{default_density_graph, default_height_graph};

const WORLD_HEIGHT: usize = 384;

fn arg_or(args: &[String], i: usize, default: i64) -> i64 {
    args.get(i)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg_or(&args, 1, 13_371) as u64;
    let px = arg_or(&args, 2, 384).clamp(16, 2048) as usize;
    let bpp = arg_or(&args, 3, 4).clamp(1, 32) as f32;

    let hg = default_height_graph(seed);
    let hp = hg.compile().expect("grafo de altura valido");
    let dg = default_density_graph(seed);
    let dp = dg.compile().expect("grafo de densidad valido");

    let height = px;
    let width = px * 2;
    let mut img = vec![0u8; width * height * 4];

    let mut set = |x: usize, y: usize, c: [u8; 3]| {
        if x < width && y < height {
            let i = (y * width + x) * 4;
            img[i] = c[0];
            img[i + 1] = c[1];
            img[i + 2] = c[2];
            img[i + 3] = 255;
        }
    };

    // Mapa de altura (izquierda).
    let (mut hmin, mut hmax) = (f32::INFINITY, f32::NEG_INFINITY);
    for py in 0..px {
        for pxx in 0..px {
            let x = pxx as f32 * bpp;
            let z = py as f32 * bpp;
            let h = hp.eval(&hg, x, 0.0, z);
            hmin = hmin.min(h);
            hmax = hmax.max(h);
            // Azul (bajo) -> verde -> marron/blanco (alto).
            let t = ((h - 8.0) / 200.0).clamp(0.0, 1.0);
            let c = if t < 0.5 {
                let u = t * 2.0;
                [
                    (30.0 + 40.0 * u) as u8,
                    (80.0 + 120.0 * u) as u8,
                    (180.0 - 120.0 * u) as u8,
                ]
            } else {
                let u = (t - 0.5) * 2.0;
                [
                    (120.0 + 135.0 * u) as u8,
                    (150.0 - 20.0 * u) as u8,
                    (90.0 - 60.0 * u) as u8,
                ]
            };
            set(pxx, py, c);
        }
    }

    // Corte vertical de densidad (derecha): x en horizontal, y de arriba a abajo.
    let (mut solid, mut total) = (0u32, 0u32);
    for py in 0..px {
        // y alto arriba.
        let y = (WORLD_HEIGHT as f32) * (1.0 - py as f32 / px as f32);
        for pxx in 0..px {
            let x = pxx as f32 * bpp;
            let d = dp.eval(&dg, x, y, 0.0);
            total += 1;
            if d > 0.0 {
                solid += 1;
                set(px + pxx, py, [120, 110, 96]);
            } else {
                set(px + pxx, py, [18, 22, 34]);
            }
        }
    }

    std::fs::create_dir_all("screenshots").ok();
    let path = format!("screenshots/graph_preview_{seed}.png");
    let file = std::fs::File::create(&path).expect("no se pudo crear el PNG");
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("cabecera PNG");
    writer.write_image_data(&img).expect("datos PNG");

    println!(
        "graph_preview seed={seed}: altura {hmin:.0}..{hmax:.0} | densidad solida {:.1}% | {path}",
        100.0 * solid as f32 / total.max(1) as f32
    );
}
