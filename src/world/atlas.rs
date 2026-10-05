//! El atlas de texturas: una sola imagen que contiene las caras de todos los
//! bloques, en una rejilla de tiles.
//!
//! Usar un atlas (en vez de una textura por bloque) permite dibujar todo el
//! mundo sin cambiar de textura: el mesher solo asigna a cada cara las
//! coordenadas UV del tile que le toca.
//!
//! Layout: `COLS` (4) tiles por fila de `TILE` (16)x`TILE` pixels.
//!
//! Desde v0.6.2 el atlas se **carga de `assets/atlas.png`** (pintado a mano en
//! LibreSprite). Si el archivo no existe, se cae al generador procedural de
//! [`build_pixels`], para que el juego siga arrancando en un clon sin assets.

/// Lado de cada tile, en pixels.
pub const TILE: u32 = 16;

/// Numero de tiles en el atlas (0..TILES).
pub const TILES: u32 = 12;

/// Tiles por fila.
pub const COLS: u32 = 4;

/// Filas de tiles.
pub const ROWS: u32 = TILES.div_ceil(COLS);

/// Ancho del atlas en pixels.
pub const WIDTH: u32 = COLS * TILE;

/// Alto del atlas en pixels.
pub const HEIGHT: u32 = ROWS * TILE;

/// Trocea el atlas en sus tiles: devuelve `TILES` bloques de `TILE`x`TILE`
/// pixels RGBA contiguos (fila a fila), listos para subir como capas de un
/// array de texturas.
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

/// Ruta del atlas en disco (relativa al directorio de trabajo).
pub const ATLAS_PATH: &str = "assets/atlas.png";

/// Carga el atlas de `assets/atlas.png` (RGBA8, sRGB). Si no existe o no se
/// puede decodificar, devuelve el atlas procedural de [`build_pixels`].
///
/// La imagen debe medir exactamente `WIDTH` x `HEIGHT` (64x48); si no, se
/// ignora y se usa el procedural.
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

/// Intenta decodificar `assets/atlas.png` a RGBA8. `None` si falla o el tamano
/// no coincide.
fn try_load_pixels() -> Option<Vec<u8>> {
    load_png_rgba(ATLAS_PATH, WIDTH, HEIGHT)
}

/// Decodifica un PNG a RGBA8, normalizando RGB/grises si hace falta. `None` si
/// no se puede abrir, no decodifica o el tamano no coincide con `width` x
/// `height`. Lo reutiliza la textura de interfaz (`render::gui`).
pub(crate) fn load_png_rgba(path: &str, width: u32, height: u32) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;

    // Rechazamos tamanos que no encajen con el layout esperado.
    if info.width != width || info.height != height {
        eprintln!(
            "[png] {path} mide {}x{} (esperado {width}x{height}); ignorado",
            info.width, info.height
        );
        return None;
    }

    // Normalizamos a RGBA8 (por si el PNG viniera en RGB o escala de grises).
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
        let (r, g, b, a) = match channels {
            3 => (buf[s], buf[s + 1], buf[s + 2], 255),
            1 => (buf[s], buf[s], buf[s], 255),
            2 => (buf[s], buf[s], buf[s], buf[s + 1]),
            _ => (0, 0, 0, 255),
        };
        rgba[i * 4] = r;
        rgba[i * 4 + 1] = g;
        rgba[i * 4 + 2] = b;
        rgba[i * 4 + 3] = a;
    }
    Some(rgba)
}

/// Genera la imagen RGBA del atlas (en sRGB, que es como la interpreta la
/// textura `Rgba8UnormSrgb`). Es el fallback sin assets.
pub fn build_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    for tile in 0..TILES {
        for y in 0..TILE {
            for x in 0..TILE {
                let noise = noise(x, y, tile);
                let rgba = tile_color(tile, x, y, noise);

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

/// Color RGBA de un pixel de un tile concreto, con los patrones de cada
/// material.
///
/// Paleta "Solaria Cartoon" (v0.8.1): tonos cercanos por tile, sin negro puro
/// ni motas de alto contraste; las manchas se agrupan en bloques de 4x4 en el
/// atlas pintado a mano. El fallback procedural usa los mismos tonos base para
/// que el juego se vea coherente con o sin assets.
fn tile_color(tile: u32, x: u32, y: u32, noise: i32) -> [u8; 4] {
    // Por defecto opaco; cada rama devuelve [r, g, b, a].
    let opaque = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    match tile {
        // 0: hierba (arriba)
        0 => opaque(tint([106, 190, 78], noise)),
        // 1: lateral de hierba (franja verde irregular arriba, tierra debajo)
        1 => {
            let depth = 2 + (noise.rem_euclid(4)) as u32;
            if y < depth {
                opaque(grass_shade(noise))
            } else {
                opaque(dirt_shade(noise))
            }
        }
        // 2: tierra (grano fino de bajo contraste)
        2 => opaque(dirt_shade(noise)),
        // 3: piedra (grises frios suaves)
        3 => opaque(tint([140, 140, 150], noise)),
        // 4: arena (amarillos calidos suaves)
        4 => opaque(tint([226, 205, 148], noise)),
        // 5: corteza del tronco (veta vertical, bajo contraste)
        5 => {
            let streak = if x.is_multiple_of(4) { -20 } else { 0 };
            let c = match noise + streak {
                i if i < -12 => [78, 51, 32],
                i if i < -2 => [96, 66, 41],
                i if i < 10 => [107, 74, 46],
                _ => [124, 89, 54],
            };
            opaque(c)
        }
        // 6: extremo del tronco (anillos concentricos)
        6 => {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let d = (dx * dx + dy * dy).sqrt();
            let c = if d > 7.0 {
                [138, 95, 46]
            } else if ((d as i32) / 2) % 2 == 0 {
                [196, 154, 90]
            } else {
                [224, 190, 130]
            };
            opaque(c)
        }
        // 7: hojas: verdes con **huecos transparentes** (alfa 0) para el cutout.
        7 => {
            if noise < -7 {
                [0, 0, 0, 0]
            } else {
                let c = match noise {
                    i if i < -2 => [44, 122, 56],
                    i if i < 8 => [62, 158, 79],
                    _ => [96, 186, 110],
                };
                opaque(c)
            }
        }
        // 8: antorcha: palo fino en la franja central, llama en la punta y
        // fondo TRANSPARENTE (para el cutout). Debe parecerse al atlas real.
        8 => {
            if (7..=8).contains(&x) && (6..=9).contains(&y) {
                // La llama: nucleo claro y bordes naranjas.
                if x == 8 && y <= 7 {
                    [255, 240, 180, 255]
                } else {
                    [240, 170, 60, 255]
                }
            } else if (7..=8).contains(&x) && y > 9 {
                opaque(tint([120, 80, 45], noise))
            } else {
                [0, 0, 0, 0] // fondo transparente
            }
        }
        // 9: nieve (blancos y azules claros), opaca.
        9 => {
            let c = if noise > 6 {
                [255, 255, 255]
            } else if noise > -3 {
                [240, 246, 252]
            } else if noise > -10 {
                [216, 230, 242]
            } else {
                [192, 212, 228]
            };
            opaque(c)
        }
        // 10: agua: azul translucida (alfa < 255) con una ondulacion suave. El
        // shader no la descarta (alfa > 0.5) y el pase de agua la mezcla.
        10 => {
            let a = 175u8;
            let c = if noise > 6 {
                [86, 150, 226]
            } else if noise > -5 {
                [54, 116, 200]
            } else {
                [40, 96, 178]
            };
            [c[0], c[1], c[2], a]
        }
        // 11: tablones (tablas horizontales con juntas).
        11 => {
            if y.is_multiple_of(4) {
                opaque([95, 63, 34])
            } else {
                let c = match noise {
                    i if i < -4 => [138, 95, 46],
                    i if i < 8 => [154, 107, 63],
                    _ => [176, 130, 78],
                };
                opaque(c)
            }
        }
        _ => [0, 0, 0, 0],
    }
}

/// Aplica una variacion de brillo `delta` a un color base, saturando a 0..255.
fn tint(base: [i32; 3], delta: i32) -> [u8; 3] {
    [
        (base[0] + delta).clamp(0, 255) as u8,
        (base[1] + delta).clamp(0, 255) as u8,
        (base[2] + delta).clamp(0, 255) as u8,
    ]
}

/// Tonos de tierra (paleta cartoon: marrones calidos cercanos).
fn dirt_shade(noise: i32) -> [u8; 3] {
    match noise {
        i if i < -12 => [107, 68, 35],
        i if i < -4 => [122, 79, 43],
        i if i < 6 => [138, 90, 51],
        i if i < 12 => [153, 103, 60],
        _ => [168, 117, 68],
    }
}

/// Tonos de hierba (verdes cartoon: base, oscuro, claro).
fn grass_shade(noise: i32) -> [u8; 3] {
    match noise {
        i if i < -4 => [78, 154, 60],
        i if i < 8 => [106, 190, 78],
        _ => [138, 217, 100],
    }
}

/// Ruido determinista en `-14..=14` a partir de la posicion y el tile.
fn noise(x: u32, y: u32, tile: u32) -> i32 {
    let mut h = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263))
        .wrapping_add(tile.wrapping_mul(2_246_822_519));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h % 29) as i32 - 14
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_atlas_tiene_el_tamano_esperado() {
        let pixels = build_pixels();
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        // Todos los tiles son opacos EXCEPTO el 8 (antorcha, cutout) y el 10
        // (agua, translucida), que necesitan alfa.
        for tile in 0..TILES {
            let col = tile % COLS;
            let row = tile / COLS;
            for y in 0..TILE {
                for x in 0..TILE {
                    let px = col * TILE + x;
                    let py = row * TILE + y;
                    let i = ((py * WIDTH + px) * 4) as usize;
                    let alpha = pixels[i + 3];
                    if tile == 8 {
                        // El fondo es transparente y la antorcha opaca.
                        assert!(alpha == 0 || alpha == 255);
                    } else if tile == 7 {
                        // Las hojas tienen huecos transparentes (cutout).
                        assert!(alpha == 0 || alpha == 255);
                    } else if tile == 10 {
                        // El agua es translucida.
                        assert!(alpha > 0 && alpha < 255, "tile 10 debe ser translucido");
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
        // El pixel (x,y) del tile t debe coincidir con el del atlas.
        for tile in 0..TILES {
            let (col, row) = (tile % COLS, tile / COLS);
            for (y, x) in [(0, 0), (5, 7), (15, 15)] {
                let src = (((row * TILE + y) * WIDTH + (col * TILE + x)) * 4) as usize;
                let dst = (((tile * TILE + y) * TILE + x) * 4) as usize;
                assert_eq!(
                    &tiles[dst..dst + 4],
                    &pixels[src..src + 4],
                    "tile {tile} pixel ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn el_tile_de_la_antorcha_tiene_transparencia() {
        // El cutout del shader (alfa < 0.5) depende de que el tile de la antorcha
        // (8) tenga fondo transparente. El atlas procedural lo genera asi.
        let pixels = build_pixels();
        let col = 8 % COLS;
        let row = 8 / COLS;
        let mut transparentes = 0;
        for y in 0..TILE {
            for x in 0..TILE {
                let px = col * TILE + x;
                let py = row * TILE + y;
                let alpha = pixels[((py * WIDTH + px) * 4 + 3) as usize];
                if alpha < 128 {
                    transparentes += 1;
                }
            }
        }
        assert!(
            transparentes > 0,
            "el tile 8 debe tener al menos un texel transparente"
        );
        // Y no puede ser todo transparente (habria desaparecido la antorcha).
        assert!(transparentes < (TILE * TILE) as usize);
    }

    #[test]
    fn las_capas_del_atlas_no_mezclan_tiles_vecinos() {
        // La capa de cada tile debe medir TILE x TILE exactos; un desajuste
        // haria que se vean texeles del tile de al lado (sangrado).
        let pixels = build_pixels();
        let tiles = split_tiles(&pixels);
        let layer = (TILE * TILE * 4) as usize;
        assert_eq!(tiles.len(), layer * TILES as usize);
        // El tile 0 (hierba) y el 1 (lateral) deben diferir en algun pixel.
        assert_ne!(&tiles[0..layer], &tiles[layer..2 * layer]);
    }
}
