//! **Preview y metricas del generador Larion** (secciones 8 y 9 del prompt).
//!
//! Genera mapas PNG y una tabla de percentiles/cobertura de biomas sin arrancar
//! el juego ni tocar la GPU. Calibrar con datos, no a ojo.
//!
//! Uso:
//! ```text
//! cargo run --release --example larion_preview -- [seed] [pixels] [step] [layer]
//! cargo run --release --example larion_preview -- 13371 512 6 height
//! ```
//!
//! `layer`:
//! * `height` (defecto), `biome`, `erosion`, `continental`, `slope`, `river`.
//! * `density` — corte vertical (X-Y) de la densidad 3D cerca de una montana.

use std::collections::HashMap;
use std::io::BufWriter;

use solaria_voxel::world::terrain::{Biome, SEA_LEVEL};
use solaria_voxel::world::worldgen::larion::{LarionConfig, LarionGenerator};

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

fn put(img: &mut [u8], pixels: usize, px: usize, py: usize, c: [u8; 3]) {
    let i = (py * pixels + px) * 4;
    img[i] = c[0];
    img[i + 1] = c[1];
    img[i + 2] = c[2];
    img[i + 3] = 255;
}

fn percentile(sorted: &[i32], p: f64) -> i32 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (((sorted.len() - 1) as f64 * p).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg_or(&args, 1, 13_371) as u32;
    let pixels = arg_or(&args, 2, 512).clamp(16, 4096) as usize;
    let step = arg_or(&args, 3, 6).clamp(1, 128) as i32;
    let layer = args.get(4).cloned().unwrap_or_else(|| "height".to_string());

    let g = LarionGenerator::new(seed);
    let cfg = *g.config();
    println!(
        "[larion] seed={seed} pixels={pixels} step={step} layer={layer} (LARION_CONFIG_VERSION {})",
        solaria_voxel::world::worldgen::larion::LARION_CONFIG_VERSION
    );

    let origin = -(pixels as i32 * step) / 2;
    let mut img = vec![0u8; pixels * pixels * 4];

    match layer.as_str() {
        "density" => render_density(&g, &mut img, pixels, step, origin),
        _ => render_map(&g, &cfg, &mut img, pixels, step, origin, &layer),
    }

    let path = format!("screenshots/larion_{seed}_{layer}.png");
    std::fs::create_dir_all("screenshots").ok();
    let file = std::fs::File::create(&path).expect("crear PNG");
    let mut enc = png::Encoder::new(BufWriter::new(file), pixels as u32, pixels as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("cabecera PNG");
    writer.write_image_data(&img).expect("datos PNG");
    println!("[larion] escrito {path}");
}

#[allow(clippy::too_many_arguments)]
fn render_map(
    g: &LarionGenerator,
    _cfg: &LarionConfig,
    img: &mut [u8],
    pixels: usize,
    step: i32,
    origin: i32,
    layer: &str,
) {
    let mut heights: Vec<i32> = Vec::with_capacity(pixels * pixels);
    let mut n_ocean = 0u64;
    let mut n_river = 0u64;
    let mut biome_counts: HashMap<Biome, u64> = HashMap::new();
    let mut land = 0u64;
    let mut f_erosion = Vec::new();
    let mut f_cont = Vec::new();
    let mut f_peaks: Vec<i32> = Vec::new();
    let mut f_temp: Vec<i32> = Vec::new();
    let mut f_hum: Vec<i32> = Vec::new();
    let mut top: Vec<(i32, f32, f32, f32)> = Vec::new();

    for py in 0..pixels {
        for px in 0..pixels {
            let x = origin + px as i32 * step;
            let z = origin + py as i32 * step;
            let s = g.sample(x as f64, z as f64);
            let h = s.height.round() as i32;
            heights.push(h);
            if s.ocean {
                n_ocean += 1;
            } else {
                land += 1;
                *biome_counts.entry(s.biome).or_insert(0) += 1;
            }
            if s.surface_water > s.height + 0.5 {
                n_river += 1;
            }
            f_erosion.push((s.erosion * 255.0) as u8);
            f_cont.push((s.continentalness * 255.0) as u8);
            f_peaks.push((s.peaks * 1000.0) as i32);
            f_temp.push((s.temperature * 1000.0) as i32);
            f_hum.push((s.humidity * 1000.0) as i32);
            top.push((h, s.erosion, s.peaks, s.continental_raw));

            let color = match layer {
                "biome" => {
                    if s.ocean {
                        ocean_color(h)
                    } else {
                        shade(biome_color(s.biome), h)
                    }
                }
                "erosion" => gray(s.erosion),
                "continental" => color_ramp(s.continentalness),
                "slope" => slope_color(g, x, z, h),
                "river" => {
                    if s.surface_water > s.height + 0.5 {
                        [40, 110, 220]
                    } else if s.ocean {
                        ocean_color(h)
                    } else {
                        shade(biome_color(s.biome), h)
                    }
                }
                _ => height_color(h),
            };
            put(img, pixels, px, py, color);
        }
    }

    let total = (pixels * pixels) as f64;
    heights.sort_unstable();
    println!(
        "[larion] oceano {:.1}% | tierra {:.1}% | agua superficial {:.1}%",
        100.0 * n_ocean as f64 / total,
        100.0 * land as f64 / total,
        100.0 * n_river as f64 / total
    );
    println!(
        "[larion] altura min/p01/p50/p95/p99/max = {}/{}/{}/{}/{}/{} (mar {SEA_LEVEL})",
        heights[0],
        percentile(&heights, 0.01),
        percentile(&heights, 0.50),
        percentile(&heights, 0.95),
        percentile(&heights, 0.99),
        heights[heights.len() - 1]
    );
    println!(
        "[larion] rango p99-p01 = {} (objetivo >= 150)",
        percentile(&heights, 0.99) - percentile(&heights, 0.01)
    );
    println!(
        "[larion] erosion media {:.2} | continental media {:.2}",
        cfg_avg(&f_erosion),
        cfg_avg(&f_cont)
    );
    f_peaks.sort_unstable();
    println!(
        "[larion] peaks p50/p95/p99/max = {:.2}/{:.2}/{:.2}/{:.2}",
        percentile(&f_peaks, 0.50) as f64 / 1000.0,
        percentile(&f_peaks, 0.95) as f64 / 1000.0,
        percentile(&f_peaks, 0.99) as f64 / 1000.0,
        f_peaks[f_peaks.len() - 1] as f64 / 1000.0
    );
    f_temp.sort_unstable();
    f_hum.sort_unstable();
    println!(
        "[larion] temperatura p01/p50/p99 = {:.2}/{:.2}/{:.2} | humedad p01/p50/p99 = {:.2}/{:.2}/{:.2}",
        percentile(&f_temp, 0.01) as f64 / 1000.0,
        percentile(&f_temp, 0.50) as f64 / 1000.0,
        percentile(&f_temp, 0.99) as f64 / 1000.0,
        percentile(&f_hum, 0.01) as f64 / 1000.0,
        percentile(&f_hum, 0.50) as f64 / 1000.0,
        percentile(&f_hum, 0.99) as f64 / 1000.0
    );
    top.sort_by_key(|t| -t.0);
    for t in top.iter().take(5) {
        println!(
            "[larion]   top h={} erosion={:.2} peaks={:.2} continental={:.2}",
            t.0, t.1, t.2, t.3
        );
    }
    let mut bioc: Vec<(&Biome, &u64)> = biome_counts.iter().collect();
    bioc.sort_by(|a, b| b.1.cmp(a.1));
    print!("[larion] biomas (tierra):");
    for (b, n) in &bioc {
        print!(" {b:?} {:.1}%", 100.0 * **n as f64 / land.max(1) as f64);
    }
    println!("  [{} distintos]", bioc.len());
}

fn cfg_avg(v: &[u8]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|&b| b as f64).sum::<f64>() / v.len() as f64 / 255.0
}

fn ocean_color(h: i32) -> [u8; 3] {
    let d = ((SEA_LEVEL - h) as f32 / 64.0).clamp(0.0, 1.0);
    [
        (30.0 + 35.0 * (1.0 - d)) as u8,
        (70.0 + 80.0 * (1.0 - d)) as u8,
        (150.0 + 100.0 * (1.0 - d)) as u8,
    ]
}

fn height_color(h: i32) -> [u8; 3] {
    let t = ((h as f32 - 20.0) / (300.0 - 20.0)).clamp(0.0, 1.0);
    // Azul -> verde -> marron -> blanco (curva simple por tramos).
    if t < 0.35 {
        let u = t / 0.35;
        [
            (30.0 + 40.0 * u) as u8,
            (70.0 + 110.0 * u) as u8,
            (150.0 + 30.0 * (1.0 - u)) as u8,
        ]
    } else if t < 0.6 {
        let u = (t - 0.35) / 0.25;
        [
            (70.0 + 70.0 * u) as u8,
            (180.0 - 60.0 * u) as u8,
            (90.0 - 30.0 * u) as u8,
        ]
    } else if t < 0.85 {
        let u = (t - 0.6) / 0.25;
        [
            (140.0 + 40.0 * u) as u8,
            (120.0 - 30.0 * u) as u8,
            (60.0 + 30.0 * u) as u8,
        ]
    } else {
        let u = (t - 0.85) / 0.15;
        [
            (180.0 + 75.0 * u) as u8,
            (90.0 + 165.0 * u) as u8,
            (90.0 + 165.0 * u) as u8,
        ]
    }
}

fn gray(v: f32) -> [u8; 3] {
    let g = (v.clamp(0.0, 1.0) * 255.0) as u8;
    [g, g, g]
}

fn color_ramp(v: f32) -> [u8; 3] {
    if v < 0.5 {
        let u = v / 0.5;
        [(40.0 + 60.0 * u) as u8, (80.0 + 60.0 * u) as u8, 190]
    } else {
        let u = (v - 0.5) / 0.5;
        [
            (100.0 + 155.0 * u) as u8,
            (140.0 + 60.0 * u) as u8,
            (190.0 - 140.0 * u) as u8,
        ]
    }
}

fn shade(base: [u8; 3], h: i32) -> [u8; 3] {
    let s = 0.75 + 0.5 * ((h as f32 - SEA_LEVEL as f32) / 200.0).clamp(0.0, 1.0);
    [
        (base[0] as f32 * s).min(255.0) as u8,
        (base[1] as f32 * s).min(255.0) as u8,
        (base[2] as f32 * s).min(255.0) as u8,
    ]
}

fn slope_color(g: &LarionGenerator, x: i32, z: i32, h: i32) -> [u8; 3] {
    let dh = |dx: i32, dz: i32| {
        (g.sample((x + dx) as f64, (z + dz) as f64).height.round() as i32 - h)
            .abs()
    };
    let slope = dh(1, 0).max(dh(-1, 0)).max(dh(0, 1)).max(dh(0, -1));
    let t = (slope as f32 / 12.0).clamp(0.0, 1.0);
    [(60.0 + 195.0 * t) as u8, (120.0 - 90.0 * t) as u8, (90.0 - 60.0 * t) as u8]
}

/// Corte vertical X-Y de la densidad cerca de un tramo con relieve.
fn render_density(g: &LarionGenerator, img: &mut [u8], pixels: usize, step: i32, origin: i32) {
    let y_max = 300i32;
    let z = 0;
    for py in 0..pixels {
        let y = (pixels - 1 - py) as i32 * y_max / (pixels.max(2) - 1) as i32;
        for px in 0..pixels {
            let x = origin + px as i32 * step;
            let s = g.sample(x as f64, z as f64);
            let h = s.height.round() as i32;
            let color = if y as f32 > s.height + 24.0 {
                [200, 220, 240]
            } else if g.density(x, y, z, &s) > 0.0 {
                [90, 80, 70]
            } else if y <= SEA_LEVEL {
                [40, 110, 200]
            } else {
                [40, 170, 180]
            };
            let _ = h;
            put(img, pixels, px, py, color);
        }
    }
    println!("[larion] corte de densidad x-y en z={z} (roca marron, aire arriba, cueva cian, agua azul)");
}
