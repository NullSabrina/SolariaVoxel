//! **Seed gallery** (FASE 9): mosaico de mapas de bioma de varias semillas.
//!
//! Util para comparar semillas de un vistazo (variedad de continentes, biomas y
//! landforms). Sin GPU: solo worldgen + PNG.
//!
//! Uso:
//! ```text
//! cargo run --release --example seed_gallery
//! ```
//! Salida: `screenshots/seed_gallery.png` (4 columnas x 2 filas).

use std::io::BufWriter;

use solaria_voxel::world::TerrainGenerator;
use solaria_voxel::world::terrain::{Biome, SEA_LEVEL};

const SEEDS: [u32; 8] = [13_371, 7, 42, 2_024, 99, 1, 555, 31_337];
const COLS: usize = 4;
const TILE: usize = 256;
const STEP: i32 = 8;

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
    let rows = SEEDS.len().div_ceil(COLS);
    let (w, h) = (COLS * TILE, rows * TILE);
    let mut img = vec![0u8; w * h * 4];

    for (i, &seed) in SEEDS.iter().enumerate() {
        let g = TerrainGenerator::new(seed);
        let span = TILE as i32 * STEP;
        let origin = -span / 2;
        let (ox, oy) = ((i % COLS) * TILE, (i / COLS) * TILE);
        let mut ocean = 0u64;
        for py in 0..TILE {
            for px in 0..TILE {
                let x = origin + px as i32 * STEP;
                let z = origin + py as i32 * STEP;
                let s = g.sample(x, z);
                let hgt = g.height(x, z);
                let c = if s.land.is_ocean() {
                    ocean += 1;
                    let d = ((SEA_LEVEL as f32 - hgt as f32) / 64.0).clamp(0.0, 1.0);
                    [
                        (30.0 + 20.0 * (1.0 - d)) as u8,
                        (70.0 + 60.0 * (1.0 - d)) as u8,
                        (150.0 + 90.0 * (1.0 - d)) as u8,
                    ]
                } else {
                    let base = biome_color(s.biome);
                    let sh = 0.8 + 0.4 * ((hgt as f32 - SEA_LEVEL as f32) / 120.0).clamp(0.0, 1.0);
                    [
                        (base[0] as f32 * sh).min(255.0) as u8,
                        (base[1] as f32 * sh).min(255.0) as u8,
                        (base[2] as f32 * sh).min(255.0) as u8,
                    ]
                };
                let idx = ((oy + py) * w + (ox + px)) * 4;
                img[idx] = c[0];
                img[idx + 1] = c[1];
                img[idx + 2] = c[2];
                img[idx + 3] = 255;
            }
        }
        println!(
            "[gallery] seed {seed}: oceano {:.0}%",
            100.0 * ocean as f64 / (TILE * TILE) as f64
        );
    }

    std::fs::create_dir_all("screenshots").ok();
    let path = "screenshots/seed_gallery.png";
    let file = std::fs::File::create(path).expect("crear PNG");
    let mut enc = png::Encoder::new(BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("cabecera PNG");
    writer.write_image_data(&img).expect("datos PNG");
    println!("[gallery] escrito {path} ({w}x{h}, seeds {SEEDS:?})");
}
