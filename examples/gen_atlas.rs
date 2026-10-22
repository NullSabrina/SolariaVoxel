//! Genera el atlas **fallback** (matrices 16x16 escaladas) y `atlas.json`.
//!
//! El atlas canonico con texturas CC0 de Minetest se regenera con
//! `tools/gen_atlas.ps1` (Windows). Este ejemplo existe para clones sin ese
//! script y para que `cargo run --example gen_atlas` nunca deje un PNG corrupto.
//!
//! ```text
//! cargo run --release --example gen_atlas
//! ```

use std::io::BufWriter;

use serde_json::json;
use solaria_voxel::world::atlas::{
    COLS, HEIGHT, ROWS, TILE, TILE_NAMES, TILES, WIDTH, build_pixels,
};

fn main() {
    std::fs::create_dir_all("assets").expect("crear assets");
    let pixels = build_pixels();
    let file = std::fs::File::create("assets/atlas.png").expect("crear atlas.png");
    let mut enc = png::Encoder::new(BufWriter::new(file), WIDTH, HEIGHT);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("cabecera PNG");
    writer.write_image_data(&pixels).expect("datos PNG");

    let tiles: Vec<_> = (0..TILES)
        .map(|i| {
            let (x, y) = ((i % COLS) * TILE, (i / COLS) * TILE);
            json!({
                "index": i,
                "name": TILE_NAMES.get(i as usize).copied().unwrap_or("unknown"),
                "x": x, "y": y, "w": TILE, "h": TILE,
            })
        })
        .collect();
    let manifest = json!({
        "generator": "cargo run --release --example gen_atlas",
        "tile": TILE, "cols": COLS, "rows": ROWS,
        "width": WIDTH, "height": HEIGHT,
        "tiles": tiles,
    });
    std::fs::write(
        "assets/atlas.json",
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("escribir atlas.json");
    println!("[atlas] fallback {WIDTH}x{HEIGHT} + atlas.json");
}
