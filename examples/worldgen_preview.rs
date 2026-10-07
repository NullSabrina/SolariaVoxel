//! Preview offline del generador de mundo (auditoria de worldgen, FASE 9).
//!
//! Genera mapas/slices de un area grande sin arrancar el juego ni tocar la GPU,
//! para **equilibrar el generador con datos** en vez de a ojo. Exporta un PNG y
//! una tabla de metricas.
//!
//! Uso:
//! ```text
//! cargo run --release --example worldgen_preview -- [seed] [pixels] [blocks_per_pixel] [layer]
//! cargo run --release --example worldgen_preview -- 13371 512 4 biome
//! ```
//!
//! `layer`:
//! * `biome` (por defecto), `height`, `continental`, `river`, `landform` — mapa
//!   cenital del area.
//! * `cave` — **slice horizontal** de cuevas a `y=30` (vista cenital).
//! * `cave_yz` — **slice vertical** de cuevas en `x=0` (perfil Y-Z).
//!
//! Salida en `screenshots/worldgen_preview_<seed>_<layer>.png`.

use std::collections::HashMap;
use std::io::BufWriter;

use solaria_voxel::world::TerrainGenerator;
use solaria_voxel::world::terrain::{Biome, SEA_LEVEL};
use solaria_voxel::world::worldgen::{LandClass, LandformProfile};

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

fn landform_color(p: LandformProfile) -> [u8; 3] {
    match p {
        LandformProfile::Rolling => [110, 180, 90],
        LandformProfile::Plateau => [205, 150, 70],
        LandformProfile::Terraced => [235, 210, 90],
        LandformProfile::Cliffs => [185, 70, 95],
    }
}

fn shade(base: [u8; 3], h: usize) -> [u8; 3] {
    let s = 0.8 + 0.4 * ((h as f32 - SEA_LEVEL as f32) / 120.0).clamp(0.0, 1.0);
    [
        (base[0] as f32 * s).min(255.0) as u8,
        (base[1] as f32 * s).min(255.0) as u8,
        (base[2] as f32 * s).min(255.0) as u8,
    ]
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
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg_or(&args, 1, 13_371) as u32;
    let pixels = arg_or(&args, 2, 512).clamp(16, 4096) as usize;
    let step = arg_or(&args, 3, 4).clamp(1, 64) as i32;
    let layer = args.get(4).cloned().unwrap_or_else(|| "biome".to_string());

    let span = pixels as i32 * step;
    let origin = -span / 2;
    let g = TerrainGenerator::new(seed);
    let mut img = vec![0u8; pixels * pixels * 4];

    println!(
        "[preview] seed={seed} area={span}x{span} bloques ({pixels}x{pixels} px, step {step}, layer {layer})"
    );

    match layer.as_str() {
        "cave" => render_cave_horizontal(&g, &mut img, pixels, step, origin, 30),
        "cave_yz" => render_cave_vertical(&g, &mut img, pixels, step, origin),
        _ => render_map(&g, &mut img, pixels, step, origin, &layer),
    }

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

/// Mapa cenital del area (bioma/altura/continental/rio/landform) + metricas.
fn render_map(
    g: &TerrainGenerator,
    img: &mut [u8],
    pixels: usize,
    step: i32,
    origin: i32,
    layer: &str,
) {
    let (mut n_ocean, mut n_land, mut n_deep) = (0u64, 0u64, 0u64);
    let mut n_river = 0u64;
    let mut heights: Vec<i32> = Vec::with_capacity(pixels * pixels);
    let mut cells: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut biome_counts: HashMap<Biome, u64> = HashMap::new();
    let mut landform_counts: HashMap<u8, u64> = HashMap::new();

    for py in 0..pixels {
        for px in 0..pixels {
            let x = origin + px as i32 * step;
            let z = origin + py as i32 * step;
            let s = g.sample(x, z);
            let h = g.height(x, z);
            cells.insert(s.cell_id);
            match s.land {
                LandClass::DeepOcean => n_deep += 1,
                LandClass::Ocean | LandClass::Shelf => n_ocean += 1,
                _ => n_land += 1,
            }
            heights.push(h as i32);
            if s.surface_water > s.base_height + 0.5 {
                n_river += 1;
            }
            *biome_counts.entry(s.biome).or_insert(0) += 1;
            let lf_key = match s.landform {
                LandformProfile::Rolling => 0,
                LandformProfile::Plateau => 1,
                LandformProfile::Terraced => 2,
                LandformProfile::Cliffs => 3,
            };
            *landform_counts.entry(lf_key).or_insert(0) += 1;

            let color = if s.land.is_ocean() {
                let d = ((SEA_LEVEL as f32 - h as f32) / 64.0).clamp(0.0, 1.0);
                [
                    (30.0 + 20.0 * (1.0 - d)) as u8,
                    (70.0 + 60.0 * (1.0 - d)) as u8,
                    (150.0 + 90.0 * (1.0 - d)) as u8,
                ]
            } else {
                match layer {
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
                    "landform" => shade(landform_color(s.landform), h),
                    "river" => {
                        let base = shade(biome_color(s.biome), h);
                        if s.surface_water > s.base_height + 0.5 {
                            let t = s.river_proximity.clamp(0.0, 1.0).max(0.35);
                            [
                                (base[0] as f32 * (1.0 - t) + 40.0 * t) as u8,
                                (base[1] as f32 * (1.0 - t) + 110.0 * t) as u8,
                                (base[2] as f32 * (1.0 - t) + 220.0 * t) as u8,
                            ]
                        } else {
                            base
                        }
                    }
                    _ => shade(biome_color(s.biome), h),
                }
            };
            put(img, pixels, px, py, color);
        }
    }

    let total = (pixels * pixels) as f64;
    heights.sort_unstable();
    println!(
        "[preview] oceano {:.1}% (abisal {:.1}%) | tierra {:.1}% | agua superficial {:.1}%",
        100.0 * (n_ocean + n_deep) as f64 / total,
        100.0 * n_deep as f64 / total,
        100.0 * n_land as f64 / total,
        100.0 * n_river as f64 / total
    );
    println!(
        "[preview] altura min/p50/p95/p99/max = {}/{}/{}/{}/{} (nivel del mar {SEA_LEVEL})",
        heights[0],
        percentile(&heights, 0.50),
        percentile(&heights, 0.95),
        percentile(&heights, 0.99),
        heights[heights.len() - 1]
    );
    println!("[preview] celdas de bioma distintas: {}", cells.len());
    let mut bioc: Vec<(&Biome, &u64)> = biome_counts.iter().collect();
    bioc.sort_by(|a, b| b.1.cmp(a.1));
    print!("[preview] biomas:");
    for (b, n) in bioc {
        print!(" {b:?} {:.1}%", 100.0 * *n as f64 / total);
    }
    println!();
    let names = ["Rolling", "Plateau", "Terraced", "Cliffs"];
    print!("[preview] landforms:");
    for (k, name) in names.iter().enumerate() {
        let n = landform_counts.get(&(k as u8)).copied().unwrap_or(0);
        print!(" {name} {:.1}%", 100.0 * n as f64 / total);
    }
    println!();
}

/// Slice horizontal de cuevas a la altura `y` (vista cenital).
fn render_cave_horizontal(
    g: &TerrainGenerator,
    img: &mut [u8],
    pixels: usize,
    step: i32,
    origin: i32,
    y: i32,
) {
    let mut carved = 0u64;
    for py in 0..pixels {
        for px in 0..pixels {
            let x = origin + px as i32 * step;
            let z = origin + py as i32 * step;
            let surface = g.height(x, z) as i32;
            let color = if y > surface {
                [200, 220, 240] // por encima del terreno
            } else if g.cave_carve_at(x, y, z) {
                carved += 1;
                [40, 170, 180] // cueva
            } else {
                [90, 80, 70] // roca maciza
            };
            put(img, pixels, px, py, color);
        }
    }
    let total = (pixels * pixels) as f64;
    println!(
        "[preview] slice horizontal y={y}: cueva {:.2}% de las celdas del plano",
        100.0 * carved as f64 / total
    );
}

/// Slice vertical de cuevas en `x=0` (perfil Y-Z).
fn render_cave_vertical(
    g: &TerrainGenerator,
    img: &mut [u8],
    pixels: usize,
    step: i32,
    origin: i32,
) {
    let y_max = 200i32;
    let x = 0;
    let mut carved = 0u64;
    let mut underground = 0u64;
    for py in 0..pixels {
        let y = (pixels - 1 - py) as i32 * y_max / (pixels.max(2) - 1) as i32;
        for px in 0..pixels {
            let z = origin + px as i32 * step;
            let surface = g.height(x, z) as i32;
            let color = if y > surface {
                [200, 220, 240] // cielo
            } else if g.cave_carve_at(x, y, z) {
                carved += 1;
                [40, 170, 180] // cueva
            } else {
                [90, 80, 70] // roca
            };
            if y <= surface {
                underground += 1;
            }
            put(img, pixels, px, py, color);
        }
    }
    let frac = if underground == 0 {
        0.0
    } else {
        100.0 * carved as f64 / underground as f64
    };
    println!(
        "[preview] slice vertical x={x}: aire subterraneo (cuevas) {frac:.2}% de {underground} celdas"
    );
}
