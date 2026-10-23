//! **Atlas de bloques 32x32** estilo "Solaria Cartoon".
//!
//! Guia visual (inspirada en la legibilidad de vanilla, arte original):
//! * paleta global de **32 colores** con bases reconocibles: hierba `#7CBD6B`,
//!   tierra `#A9704F`, piedra `#8B8B8B`, arena `#D2BC96`;
//! * patrones en celdas de 4x4/8x8, **sin ruido por pixel**, para que el bloque
//!   se lea limpio a distancia;
//! * detalles escasos: grietas, vetas, remaches y musgo;
//! * transparencia recortada en hojas y antorcha; agua translucida.
//!
//! El runtime sube el atlas como **texture array** (una capa por tile), asi que
//! no hay sangrado entre tiles y los mipmaps se generan por capa.

/// Lado de cada tile, en pixels.
pub const TILE: u32 = 32;

/// Numero de tiles (derivado del registro de bloques).
pub const TILES: u32 = super::registry::TILE_COUNT as u32;

/// Tiles por fila.
pub const COLS: u32 = 8;

/// Filas de tiles.
pub const ROWS: u32 = TILES.div_ceil(COLS);

/// Ancho del atlas en pixels.
pub const WIDTH: u32 = COLS * TILE;

/// Alto del atlas en pixels.
pub const HEIGHT: u32 = ROWS * TILE;

/// Nombres estables de los tiles, en orden de indice.
pub const TILE_NAMES: [&str; 19] = [
    "grass_top",
    "grass_side",
    "dirt",
    "stone",
    "sand",
    "wood_side",
    "wood_top",
    "leaves",
    "torch",
    "snow",
    "water",
    "planks",
    "crafting_side",
    "crafting_top",
    "coarse_dirt",
    "gravel",
    "podzol_top",
    "lava",
    "obsidian",
];

/// Paleta global de 32 colores. Familias ordenadas de oscuro a claro.
const PALETTE: [[u8; 3]; 32] = [
    [24, 22, 26],
    [42, 40, 46],
    [62, 60, 66],
    [82, 80, 84],
    [112, 110, 112],
    [139, 139, 139],
    [166, 166, 168],
    [58, 46, 74],
    [200, 208, 218],
    [224, 232, 240],
    [245, 249, 252],
    [74, 50, 32],
    [108, 72, 46],
    [139, 94, 62],
    [169, 112, 79],
    [192, 138, 100],
    [126, 98, 66],
    [150, 120, 84],
    [174, 144, 106],
    [210, 188, 150],
    [34, 72, 38],
    [52, 102, 50],
    [74, 135, 64],
    [96, 168, 78],
    [120, 196, 94],
    [146, 214, 112],
    [44, 84, 170],
    [70, 120, 220],
    [104, 156, 240],
    [214, 74, 34],
    [246, 140, 38],
    [255, 214, 92],
];

fn p(i: i32) -> [u8; 4] {
    let c = PALETTE[i.clamp(0, 31) as usize];
    [c[0], c[1], c[2], 255]
}

fn a(i: i32, alpha: u8) -> [u8; 4] {
    let mut c = p(i);
    c[3] = alpha;
    c
}

/// Hash determinista por pixel y tile.
fn h(x: u32, y: u32, tile: u32) -> u32 {
    let mut v = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263))
        .wrapping_add(tile.wrapping_mul(2_246_822_519));
    v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
    v ^ (v >> 16)
}

/// Elige un indice de paleta por celda de `size` px (patron limpio).
fn sel(x: u32, y: u32, size: u32, tile: u32, opts: &[i32]) -> i32 {
    opts[(h(x / size, y / size, tile) as usize) % opts.len()]
}

/// Variacion de +-1 en celdas de 2x2 (grano suave, no ruido por pixel).
fn grain(x: u32, y: u32, tile: u32) -> i32 {
    (h(x >> 1, y >> 1, tile) % 3) as i32 - 1
}

/// Trocea el atlas en `TILES` capas de `TILE`x`TILE` RGBA.
pub fn split_tiles(atlas: &[u8]) -> Vec<u8> {
    assert_eq!(atlas.len(), (WIDTH * HEIGHT * 4) as usize);
    let mut out = vec![0u8; (TILES * TILE * TILE * 4) as usize];
    for tile in 0..TILES {
        let (col, row) = (tile % COLS, tile / COLS);
        for y in 0..TILE {
            for x in 0..TILE {
                let src = (((row * TILE + y) * WIDTH + (col * TILE + x)) * 4) as usize;
                let dst = (((tile * TILE + y) * TILE + x) * 4) as usize;
                out[dst..dst + 4].copy_from_slice(&atlas[src..src + 4]);
            }
        }
    }
    out
}

/// Cadena de mipmaps del texture array: `levels[l]` contiene `TILES` capas de
/// `TILE >> l` pixels.
pub fn mip_chain(atlas: &[u8]) -> Vec<Vec<u8>> {
    let mut levels = vec![split_tiles(atlas)];
    let mut size = TILE;
    while size > 1 {
        let prev = levels.last().expect("nivel 0");
        let next_size = size / 2;
        let mut next = vec![0u8; (TILES * next_size * next_size * 4) as usize];
        for tile in 0..TILES {
            for y in 0..next_size {
                for x in 0..next_size {
                    let mut sum = [0u32; 4];
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = x * 2 + dx;
                            let sy = y * 2 + dy;
                            let si = (((tile * size + sy) * size + sx) * 4) as usize;
                            for c in 0..4 {
                                sum[c] += prev[si + c] as u32;
                            }
                        }
                    }
                    let di = (((tile * next_size + y) * next_size + x) * 4) as usize;
                    for c in 0..4 {
                        next[di + c] = (sum[c] / 4) as u8;
                    }
                }
            }
        }
        levels.push(next);
        size = next_size;
    }
    levels
}

/// Ruta del atlas en disco (relativa al directorio de trabajo).
pub const ATLAS_PATH: &str = "assets/atlas.png";

/// Carga el atlas de `assets/atlas.png`; si falta o no mide lo esperado, usa el
/// procedural.
pub fn load_pixels() -> Vec<u8> {
    match try_load_pixels() {
        Some(p) => {
            println!("[atlas] cargado {ATLAS_PATH} ({WIDTH}x{HEIGHT})");
            p
        }
        None => {
            println!("[atlas] sin {ATLAS_PATH}; uso el atlas procedural");
            build_pixels()
        }
    }
}

fn try_load_pixels() -> Option<Vec<u8>> {
    load_png_rgba(ATLAS_PATH, WIDTH, HEIGHT)
}

/// Resuelve un asset relativo probando, en orden: el directorio actual, la
/// carpeta del ejecutable, sus ancestros (hasta 3 niveles) y la raiz del repo en
/// desarrollo. Asi el binario funciona tanto desde la raiz como desde
/// `target/release`, incluso si se abre con doble clic.
fn resolve_asset_path(path: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }

    let mut bases: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        bases.push(dir.to_path_buf());
        let mut cur = dir;
        for _ in 0..3 {
            let Some(parent) = cur.parent() else { break };
            bases.push(parent.to_path_buf());
            cur = parent;
        }
    }
    bases.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    bases
        .into_iter()
        .map(|base| base.join(p))
        .find(|c| c.is_file())
}

/// Decodifica un PNG a RGBA8 (lo comparten GUI y cielo).
pub(crate) fn load_png_rgba(path: &str, width: u32, height: u32) -> Option<Vec<u8>> {
    let resolved = resolve_asset_path(path)?;
    let file = std::fs::File::open(&resolved).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.width != width || info.height != height {
        eprintln!(
            "[png] {} mide {}x{} (esperado {width}x{height}); ignorado",
            resolved.display(),
            info.width,
            info.height
        );
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => 1,
    };
    if channels == 4 {
        return Some(buf);
    }
    let count = (width * height) as usize;
    let mut rgba = vec![0u8; count * 4];
    for i in 0..count {
        let s = i * channels;
        let (r, g, b, al) = match channels {
            3 => (buf[s], buf[s + 1], buf[s + 2], 255),
            1 => (buf[s], buf[s], buf[s], 255),
            2 => (buf[s], buf[s], buf[s], buf[s + 1]),
            _ => (0, 0, 0, 255),
        };
        rgba[i * 4] = r;
        rgba[i * 4 + 1] = g;
        rgba[i * 4 + 2] = b;
        rgba[i * 4 + 3] = al;
    }
    Some(rgba)
}

/// Matrices 16x16 pintadas a mano. Se escalan x2 a 32x32 con nearest-neighbor:
/// asi el bloque conserva la lectura limpia de vanilla sin ruido por pixel.
fn matrix_for(tile: u32) -> Option<&'static [&'static str; 16]> {
    match tile {
        0 => Some(&GRASS_TOP),
        1 => Some(&GRASS_SIDE),
        2 => Some(&DIRT),
        3 => Some(&STONE),
        4 => Some(&SAND),
        5 => Some(&WOOD_SIDE),
        7 => Some(&LEAVES),
        8 => Some(&TORCH),
        _ => None,
    }
}

const GRASS_TOP: [&str; 16] = [
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmnnmmmmmmmmm",
    "mmmmnnnnmmmmmmmm",
    "mmmmnnnnmmmmmmmm",
    "mmmmmnnmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmnnmmm",
    "mmmmmmmmmmnnnnmm",
    "mmmmmmmmmmnnnnmm",
    "mmmmmmmmmmmnnmmm",
    "mmmmmmmmmmmmmmmm",
];

const GRASS_SIDE: [&str; 16] = [
    "mmmmmmmmmmmmmmmm",
    "mnnmmmmmmmmmmnnm",
    "mnnmmmmmmmmmmnnm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
    "dddddddddddddddd",
    "eeeeeeeeeeeeeeee",
    "eeebeeeeeeeeeeee",
    "eeeeeeeeeeebeeee",
    "eeeeefeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
    "ebeeeeeeeeeeebee",
    "eeeeeeeeefeeeeee",
    "eeeeeeeeeeeeeeee",
    "dddddddddddddddd",
];

const DIRT: [&str; 16] = [
    "eeeeeeeeeeeeeeee",
    "edeeeeeeeeeeedee",
    "eeeeeeeeeeeeeeee",
    "eeeeefeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
    "ebeeeeeeeeeeebee",
    "eeeeeeeeeeeeeeee",
    "eeeeeeeeefeeeeee",
    "eeeeeeeeeeeeeeee",
    "eeedeeeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
    "eeeeeeeeeeeeedee",
    "eeeeefeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
    "eeeeeeeeeeeeeeee",
];

const STONE: [&str; 16] = [
    "5555555555555555",
    "5555555555555555",
    "5555555555555555",
    "5555555555555555",
    "5544555535555555",
    "5544555553555555",
    "5555555555355555",
    "5555555555555555",
    "5555555555555555",
    "5555555555555555",
    "5555555555555555",
    "5555555555556655",
    "5555555555556655",
    "5555555555555555",
    "5555555555555555",
    "5555555555555555",
];

const SAND: [&str; 16] = [
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjiiijjjjjjjj",
    "jjjjjiiijjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjhjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjiiijjjjj",
    "jjjjjjjjiiijjjjj",
    "jjjjjjjjjjjjjjjj",
    "jjjjjjjjjjjjjjjj",
];

const WOOD_SIDE: [&str; 16] = [
    "iiiiiiiiiiiiiiii",
    "iiibbiiiiiiibbii",
    "iiibbiiiiiiibbii",
    "iiiiiiiiiiiiiiii",
    "iiiiiiiiiiiiiiii",
    "iiibbiiiiiiibbii",
    "iiibbiiiiiiibbii",
    "iiiiiiiiiiiiiiii",
    "iiiiiiiiiiiiiiii",
    "iiibbiiiiiiibbii",
    "iiibbiiiiiiibbii",
    "iiiiiiiiiiiiiiii",
    "iiiiiiiiiiiiiiii",
    "iiibbiiiiiiibbii",
    "iiibbiiiiiiibbii",
    "iiiiiiiiiiiiiiii",
];

const LEAVES: [&str; 16] = [
    "mmmm.mmmmmmm.mmm",
    "mmm...mmmm...mmm",
    "mmm...mmmm...mmm",
    "mmmm.mmmmmmm.mmm",
    "mmmmmmmmmmmmmmmm",
    "mm.mmmmmmmm.mmmm",
    "m...mmmmmm...mmm",
    "m...mmmmmm...mmm",
    "mm.mmmmmmmm.mmmm",
    "mmmmmmmmmmmmmmmm",
    "mmmm.mmmmmmm.mmm",
    "mmm...mmmm...mmm",
    "mmm...mmmm...mmm",
    "mmmm.mmmmmmm.mmm",
    "mmmmmmmmmmmmmmmm",
    "mmmmmmmmmmmmmmmm",
];

const TORCH: [&str; 16] = [
    "................",
    "................",
    ".......vv.......",
    "......vuuv......",
    "......vuuv......",
    ".......vv.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
    ".......gg.......",
];

fn matrix_color(ch: char) -> [u8; 4] {
    match ch {
        '.' => [0, 0, 0, 0],
        '0'..='9' => p(ch as i32 - '0' as i32),
        'a'..='v' => p(10 + (ch as i32 - 'a' as i32)),
        _ => [0, 0, 0, 0],
    }
}

/// Genera el atlas RGBA (fallback reproducible).
pub fn build_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for tile in 0..TILES {
        for y in 0..TILE {
            for x in 0..TILE {
                let rgba = match matrix_for(tile) {
                    Some(rows) => {
                        let ch = rows[(y / 2) as usize]
                            .chars()
                            .nth((x / 2) as usize)
                            .unwrap_or('.');
                        matrix_color(ch)
                    }
                    None => tile_color(tile, x, y),
                };
                let col = tile % COLS;
                let row = tile / COLS;
                let px = col * TILE + x;
                let py = row * TILE + y;
                let i = ((py * WIDTH + px) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&rgba);
            }
        }
    }
    pixels
}

/// Color de un pixel con patrones limpios por material.
fn tile_color(tile: u32, x: u32, y: u32) -> [u8; 4] {
    let g = grain(x, y, tile);
    match tile {
        // 0: hierba arriba (manchas 8x8 y algun brillo 2x2).
        0 => {
            let base = sel(x, y, 8, 0, &[22, 23, 23, 24, 22]);
            p((base + g).clamp(20, 25))
        }
        // 1: lateral de hierba; borde irregular de 6..9 px.
        1 => {
            let fringe = 6 + sel(x, 0, 4, 1, &[0, 1, 2, 3]) as u32;
            if y < fringe {
                p((sel(x, y, 4, 1, &[22, 23, 24, 23]) + g).clamp(20, 25))
            } else if y == fringe {
                p(20)
            } else {
                p((sel(x, y, 4, 1, &[13, 14, 15, 14]) + g).clamp(12, 15))
            }
        }
        // 2: tierra con guijarros escasos.
        2 => {
            if h(x, y, 2).is_multiple_of(47) {
                p(11)
            } else {
                p((sel(x, y, 4, 2, &[13, 14, 14, 15, 13]) + g).clamp(12, 15))
            }
        }
        // 3: piedra con grietas finas y brillo sutil.
        3 => {
            let crack = (x * 3 + y * 5 + h(x, y, 3) % 11) % 53 < 2;
            if crack {
                p(3)
            } else {
                p((sel(x, y, 8, 3, &[4, 5, 5, 6, 5]) + g).clamp(4, 6))
            }
        }
        // 4: arena.
        4 => p((sel(x, y, 4, 4, &[18, 19, 19, 19]) + g).clamp(18, 19)),
        // 5: corteza con vetas verticales y nudos.
        5 => {
            if x % 8 <= 1 {
                p((11 + g).clamp(11, 12))
            } else {
                p((sel(x, y, 4, 5, &[16, 17, 17, 18]) + g).clamp(16, 18))
            }
        }
        // 6: anillos del tronco.
        6 => {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > 15.0 {
                p(16)
            } else {
                p(if ((d / 3.2) as i32) % 2 == 0 { 18 } else { 16 })
            }
        }
        // 7: hojas con huecos grandes (cutout, ~18%).
        7 => {
            let hole = h(x >> 2, y >> 2, 7) % 100 < 14 || h(x >> 1, y >> 1, 7) % 100 < 6;
            if hole {
                [0, 0, 0, 0]
            } else {
                p((sel(x, y, 4, 7, &[21, 22, 22, 23, 24]) + g).clamp(20, 24))
            }
        }
        // 8: sprite de antorcha (el bloque usa un modelo 3D; esto es para UI).
        8 => {
            let flame = (x as i32 - 16).pow(2) + (y as i32 - 9).pow(2) * 2 < 30;
            if flame {
                if y < 8 { a(31, 255) } else { a(30, 255) }
            } else if (14..=17).contains(&x) && (14..32).contains(&y) {
                p(16)
            } else {
                [0, 0, 0, 0]
            }
        }
        // 9: nieve limpia.
        9 => p((sel(x, y, 8, 9, &[9, 9, 10, 10]) + (g / 2)).clamp(8, 10)),
        // 10: agua translucida con bandas horizontales.
        10 => {
            let band = y % 8;
            if band < 2 {
                a(28, 200)
            } else if band == 4 {
                a(26, 200)
            } else {
                a(27, 200)
            }
        }
        // 11: tablones horizontales con juntas.
        11 => {
            if y.is_multiple_of(8) {
                p(12)
            } else {
                p((sel(x, y, 4, 11, &[17, 17, 18, 16]) + g).clamp(16, 18))
            }
        }
        // 12: lateral de mesa.
        12 => {
            if !(2..=29).contains(&y) || !(2..=29).contains(&x) {
                p(12)
            } else {
                p((sel(x, y, 4, 12, &[17, 17, 18, 16]) + g).clamp(16, 18))
            }
        }
        // 13: tapa de mesa con rejilla.
        13 => {
            if !(2..=29).contains(&x)
                || !(2..=29).contains(&y)
                || x == 10
                || x == 21
                || y == 10
                || y == 21
            {
                p(12)
            } else {
                p((17 + g).clamp(16, 18))
            }
        }
        // 14: tierra gruesa.
        14 => p((sel(x, y, 4, 14, &[12, 12, 13, 14]) + g).clamp(12, 14)),
        // 15: grava (cantos con juntas oscuras).
        15 => {
            if x.is_multiple_of(4) || y.is_multiple_of(4) {
                p(1)
            } else {
                p(sel(x, y, 4, 15, &[3, 4, 4, 6, 12, 13]))
            }
        }
        // 16: podzol (tierra oscura con musgo).
        16 => {
            if h(x, y, 16).is_multiple_of(37) {
                p(21)
            } else {
                p((sel(x, y, 4, 16, &[13, 13, 14, 12]) + g).clamp(12, 14))
            }
        }
        // 17: lava (naranja con costra y puntos calientes).
        17 => {
            let base = sel(x, y, 8, 17, &[29, 30, 30, 31]);
            if h(x >> 2, y >> 2, 17).is_multiple_of(17) {
                p(12)
            } else {
                p((base + g).clamp(29, 31))
            }
        }
        // 18: obsidiana (purpura oscura con motas).
        18 => {
            if h(x, y, 18).is_multiple_of(41) {
                p(6)
            } else {
                p((sel(x, y, 8, 18, &[1, 2, 7, 7]) + (g / 2)).clamp(1, 7))
            }
        }
        _ => [0, 0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resuelve_los_assets_del_repo() {
        assert!(resolve_asset_path(ATLAS_PATH).is_some());
        assert!(resolve_asset_path("assets/no-existe.png").is_none());
    }

    #[test]
    fn las_matrices_16x16_estan_bien_formadas() {
        for tile in 0..TILES {
            if let Some(rows) = matrix_for(tile) {
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(row.chars().count(), 16, "tile {tile} fila {i}");
                }
            }
        }
    }

    #[test]
    fn el_atlas_tiene_el_tamano_esperado() {
        let pixels = build_pixels();
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        for tile in 0..TILES {
            let (col, row) = (tile % COLS, tile / COLS);
            for y in 0..TILE {
                for x in 0..TILE {
                    let i = (((row * TILE + y) * WIDTH + (col * TILE + x)) * 4) as usize;
                    let alpha = pixels[i + 3];
                    if tile == 8 || tile == 7 {
                        assert!(alpha == 0 || alpha == 255, "tile {tile} alfa {alpha}");
                    } else if tile == 10 {
                        assert!(alpha > 0 && alpha < 255, "agua debe ser translucida");
                    } else {
                        assert_eq!(alpha, 255, "tile {tile} pixel ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn trocear_conserva_los_pixeles_de_cada_tile() {
        let pixels = build_pixels();
        let tiles = split_tiles(&pixels);
        assert_eq!(tiles.len(), (TILES * TILE * TILE * 4) as usize);
        for tile in 0..TILES {
            let (col, row) = (tile % COLS, tile / COLS);
            for (y, x) in [(0, 0), (5, 7), (31, 31)] {
                let src = (((row * TILE + y) * WIDTH + (col * TILE + x)) * 4) as usize;
                let dst = (((tile * TILE + y) * TILE + x) * 4) as usize;
                assert_eq!(&tiles[dst..dst + 4], &pixels[src..src + 4]);
            }
        }
    }

    #[test]
    fn la_cadena_de_mips_termina_en_un_pixel_por_capa() {
        let levels = mip_chain(&build_pixels());
        assert_eq!(levels.len(), 6, "32 -> 16 -> 8 -> 4 -> 2 -> 1");
        assert_eq!(levels.last().unwrap().len(), (TILES * 4) as usize);
    }

    #[test]
    fn los_nombres_de_tile_cubren_todos_los_tiles() {
        assert_eq!(TILE_NAMES.len() as u32, TILES);
    }

    #[test]
    fn el_tile_de_la_antorcha_tiene_transparencia() {
        let pixels = build_pixels();
        let col = 8 % COLS;
        let row = 8 / COLS;
        let mut transparentes = 0;
        for y in 0..TILE {
            for x in 0..TILE {
                let px = col * TILE + x;
                let py = row * TILE + y;
                if pixels[((py * WIDTH + px) * 4 + 3) as usize] < 128 {
                    transparentes += 1;
                }
            }
        }
        assert!(transparentes > 0);
        assert!(transparentes < (TILE * TILE) as usize);
    }
}
