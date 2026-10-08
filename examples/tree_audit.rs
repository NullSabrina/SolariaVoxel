//! Auditoria offline de **arboles y hojas** (MEGA PROMPT 1, Fase A).
//!
//! Genera un area de columnas reales (sin GPU) para varios biomas y semillas y
//! mide, con numeros, los problemas de la seccion 2 del prompt:
//!
//! * forma rigida: alturas de tronco distintas y formas de copa distintas;
//! * hojas dentro de solidos / dentro de agua;
//! * arboles en el borde del chunk (banda excluida `2..=13`);
//! * repeticion de patron por chunk (la altura solo depende de `(x, z)` locales).
//!
//! Uso:
//! ```text
//! cargo run --release --example tree_audit
//! cargo run --release --example tree_audit -- 4      # tam. de la rejilla en chunks
//! ```
//!
//! Salida: tabla por semilla + por bioma, y `screenshots/tree_audit_<seed>.png`
//! (vista cenital: madera en marron, hojas en verde; revela el enrejado de chunk).

use std::collections::HashMap;
use std::io::BufWriter;

use solaria_voxel::world::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};
use solaria_voxel::world::terrain::Biome;
use solaria_voxel::world::{Block, TerrainGenerator};

const SEEDS: [u32; 5] = [13_371, 7, 2_024, 99, 4_242];

/// Acumulador de una corrida (una semilla o un bioma).
#[derive(Default, Clone)]
struct Stats {
    columns: u64,
    trees: u64,
    trunk_sum: u64,
    leaves: u64,
    /// Hojas con un vecino (6 direcciones) solido que **no** es tronco.
    leaves_against_terrain: u64,
    /// Hojas con un vecino de agua.
    leaves_over_water: u64,
    /// Troncos en la banda de borde excluida (`0..=1` o `14..=15`).
    border_trees: u64,
    /// Troncos cuya copa (radio 2) se saldria del chunk.
    clipped_canopies: u64,
    /// Altura local que contradice una observacion previa (repeticion de patron).
    inconsistent_heights: u64,
    trunk_heights: HashMap<u8, u64>,
}

impl Stats {
    fn merge(&mut self, other: &Stats) {
        self.columns += other.columns;
        self.trees += other.trees;
        self.trunk_sum += other.trunk_sum;
        self.leaves += other.leaves;
        self.leaves_against_terrain += other.leaves_against_terrain;
        self.leaves_over_water += other.leaves_over_water;
        self.border_trees += other.border_trees;
        self.clipped_canopies += other.clipped_canopies;
        self.inconsistent_heights += other.inconsistent_heights;
        for (h, n) in &other.trunk_heights {
            *self.trunk_heights.entry(*h).or_insert(0) += n;
        }
    }

    fn trees_per_km2(&self) -> f64 {
        if self.columns == 0 {
            return 0.0;
        }
        // 1 columna ~ 1 m^2.
        self.trees as f64 * 1_000_000.0 / self.columns as f64
    }

    fn mean_trunk(&self) -> f64 {
        if self.trees == 0 {
            return 0.0;
        }
        self.trunk_sum as f64 / self.trees as f64
    }

    fn distinct_heights(&self) -> usize {
        self.trunk_heights.len()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grid: i32 = args
        .get(1)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(16)
        .clamp(2, 48);
    let side = grid * CHUNK_SIZE as i32;

    println!(
        "tree_audit: rejilla {grid}x{grid} chunks ({side}x{side} bloques = {:.4} km2) por semilla",
        (side as f64 * side as f64) / 1_000_000.0
    );

    let mut global = Stats::default();
    for &seed in &SEEDS {
        let (stats, per_biome, img) = audit_seed(seed, grid);
        print_block(&format!("SEMILLA {seed}"), &stats, &per_biome);
        let path = format!("screenshots/tree_audit_{seed}.png");
        std::fs::create_dir_all("screenshots").ok();
        write_png(&path, side as usize, &img);
        println!("  -> {path}");
        global.merge(&stats);
    }
    println!();
    print_block("GLOBAL (5 semillas)", &global, &HashMap::new());
}

/// Audita una semilla: recorre la rejilla de chunks, cuenta arboles/hojas y
/// construye la imagen cenital.
fn audit_seed(seed: u32, grid: i32) -> (Stats, HashMap<Biome, Stats>, Vec<u8>) {
    let g = TerrainGenerator::new(seed);
    let half = grid / 2;
    let side = (grid * CHUNK_SIZE as i32) as usize;
    let origin = -(half * CHUNK_SIZE as i32);

    let mut total = Stats::default();
    let mut per_biome: HashMap<Biome, Stats> = HashMap::new();
    // (x local, z local) -> altura de tronco observada. Si la altura ignora las
    // coordenadas globales, este mapa nunca contradice una observacion previa.
    let mut local_heights: HashMap<(u8, u8), u8> = HashMap::new();
    let mut img = vec![0u8; side * side * 4];

    for cz in 0..grid {
        for cx in 0..grid {
            let wx0 = origin + cx * CHUNK_SIZE as i32;
            let wz0 = origin + cz * CHUNK_SIZE as i32;
            let column = g.generate_column(wx0, wz0);

            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = wx0 + lx as i32;
                    let wz = wz0 + lz as i32;
                    let biome = g.sample(wx, wz).biome;

                    let mut col = Stats {
                        columns: 1,
                        ..Default::default()
                    };
                    scan_column(&column, lx, lz, &mut col, &mut local_heights);

                    total.merge(&col);
                    per_biome.entry(biome).or_default().merge(&col);

                    let (r, gg, b) = column_pixel(&column, lx, lz);
                    let px = (wx - origin) as usize;
                    let py = (wz - origin) as usize;
                    let i = (py * side + px) * 4;
                    img[i] = r;
                    img[i + 1] = gg;
                    img[i + 2] = b;
                    img[i + 3] = 255;
                }
            }
        }
    }

    (total, per_biome, img)
}

/// Escanea una columna vertical `(lx, lz)`.
fn scan_column(
    column: &Column,
    lx: usize,
    lz: usize,
    col: &mut Stats,
    local_heights: &mut HashMap<(u8, u8), u8>,
) {
    let mut y = 1usize;
    while y < WORLD_HEIGHT {
        match column.get(lx, y, lz) {
            Block::Wood => {
                // Base de tronco: madera sin madera justo debajo.
                if column.get_or_air(lx as i32, y as i32 - 1, lz as i32) != Block::Wood {
                    let mut h = 0usize;
                    while y + h < WORLD_HEIGHT && column.get(lx, y + h, lz) == Block::Wood {
                        h += 1;
                    }
                    col.trees += 1;
                    col.trunk_sum += h as u64;
                    *col.trunk_heights.entry(h.min(255) as u8).or_insert(0) += 1;

                    // Repeticion por chunk: la altura debe ser funcion de (lx, lz).
                    let key = (lx as u8, lz as u8);
                    match local_heights.get(&key) {
                        Some(prev) if *prev != h as u8 => col.inconsistent_heights += 1,
                        None => {
                            local_heights.insert(key, h as u8);
                        }
                        _ => {}
                    }

                    // Banda excluida por el generador (y copa de radio 2 que no cabe).
                    if !(2..=13).contains(&lx) || !(2..=13).contains(&lz) {
                        col.border_trees += 1;
                        col.clipped_canopies += 1;
                    }
                    y += h;
                    continue;
                }
            }
            Block::Leaves => {
                col.leaves += 1;
                let mut against_terrain = false;
                let mut over_water = false;
                for (dx, dy, dz) in NEIGHBORS {
                    let n = column.get_or_air(lx as i32 + dx, y as i32 + dy, lz as i32 + dz);
                    if n == Block::Water {
                        over_water = true;
                    }
                    if n.is_solid() && n != Block::Wood {
                        against_terrain = true;
                    }
                }
                if against_terrain {
                    col.leaves_against_terrain += 1;
                }
                if over_water {
                    col.leaves_over_water += 1;
                }
            }
            _ => {}
        }
        y += 1;
    }
}

const NEIGHBORS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Color cenital de una columna vertical: tronco > hoja > otro.
fn column_pixel(column: &Column, lx: usize, lz: usize) -> (u8, u8, u8) {
    let mut has_leaf = false;
    let mut has_wood = false;
    for y in 0..WORLD_HEIGHT {
        match column.get(lx, y, lz) {
            Block::Leaves => has_leaf = true,
            Block::Wood => has_wood = true,
            _ => {}
        }
    }
    if has_wood {
        (120, 80, 40)
    } else if has_leaf {
        (60, 160, 70)
    } else {
        (30, 40, 60)
    }
}

fn print_block(title: &str, s: &Stats, per_biome: &HashMap<Biome, Stats>) {
    println!("\n== {title} ==");
    println!(
        "  arboles/km2={:.0}  tronco medio={:.2}  alturas distintas={}  hojas={}",
        s.trees_per_km2(),
        s.mean_trunk(),
        s.distinct_heights(),
        s.leaves
    );
    println!(
        "  hojas contra terreno={}  hojas sobre agua={}  troncos en borde={}  copas recortadas={}  alturas inconsistentes={}",
        s.leaves_against_terrain,
        s.leaves_over_water,
        s.border_trees,
        s.clipped_canopies,
        s.inconsistent_heights
    );
    let mut hs: Vec<(&u8, &u64)> = s.trunk_heights.iter().collect();
    hs.sort_by_key(|(h, _)| **h);
    print!("  alturas de tronco:");
    for (h, n) in hs {
        print!(" {h}:{n}");
    }
    println!();

    if !per_biome.is_empty() {
        let mut rows: Vec<(&Biome, &Stats)> = per_biome.iter().collect();
        rows.sort_by(|a, b| {
            b.1.trees
                .cmp(&a.1.trees)
                .then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)))
        });
        println!(
            "  {:<9} {:>8} {:>10} {:>8} {:>7}",
            "bioma", "cols", "arb/km2", "tronco", "alto#"
        );
        for (b, st) in rows {
            println!(
                "  {:<9} {:>8} {:>10.0} {:>8.2} {:>7}",
                format!("{b:?}"),
                st.columns,
                st.trees_per_km2(),
                st.mean_trunk(),
                st.distinct_heights()
            );
        }
    }
}

fn write_png(path: &str, side: usize, img: &[u8]) {
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  no se pudo crear {path}: {e}");
            return;
        }
    };
    let mut enc = png::Encoder::new(BufWriter::new(file), side as u32, side as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = match enc.write_header() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("  PNG header: {e}");
            return;
        }
    };
    if let Err(e) = writer.write_image_data(img) {
        eprintln!("  PNG data: {e}");
    }
}
