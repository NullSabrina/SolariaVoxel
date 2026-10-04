//! El atlas de texturas: una sola imagen que contiene las caras de todos los
//! bloques, en una rejilla de tiles.
//!
//! Usar un atlas (en vez de una textura por bloque) permite dibujar todo el
//! mundo sin cambiar de textura: el mesher solo asigna a cada cara las
//! coordenadas UV del tile que le toca.
//!
//! Layout: `COLS` tiles por fila, cada uno de `TILE`x`TILE` pixels. Generamos
//! los pixels por codigo (arte procedural sencillo) para no depender de assets.

/// Lado de cada tile, en pixels.
pub const TILE: u32 = 16;

/// Numero de tiles en el atlas (0..TILES).
pub const TILES: u32 = 8;

/// Tiles por fila.
pub const COLS: u32 = 4;

/// Filas de tiles.
pub const ROWS: u32 = TILES.div_ceil(COLS);

/// Ancho del atlas en pixels.
pub const WIDTH: u32 = COLS * TILE;

/// Alto del atlas en pixels.
pub const HEIGHT: u32 = ROWS * TILE;

/// Rectangulo UV de un tile: `[u0, v0, u1, v1]`, con medio texel de margen para
/// que el filtrado no coja pixels del tile vecino.
pub fn tile_uv_rect(tile: u16) -> [f32; 4] {
    let t = tile as u32;
    let col = t % COLS;
    let row = t / COLS;

    // Medio texel de inset.
    let du = 0.5 / WIDTH as f32;
    let dv = 0.5 / HEIGHT as f32;

    [
        (col * TILE) as f32 / WIDTH as f32 + du,
        (row * TILE) as f32 / HEIGHT as f32 + dv,
        ((col + 1) * TILE) as f32 / WIDTH as f32 - du,
        ((row + 1) * TILE) as f32 / HEIGHT as f32 - dv,
    ]
}

/// Genera la imagen RGBA del atlas (en sRGB, que es como la interpreta la
/// textura `Rgba8UnormSrgb`).
pub fn build_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    for tile in 0..TILES {
        for y in 0..TILE {
            for x in 0..TILE {
                let noise = noise(x, y, tile);
                let rgb = tile_color(tile, x, y, noise);

                let col = tile % COLS;
                let row = tile / COLS;
                let px = col * TILE + x;
                let py = row * TILE + y;
                let i = ((py * WIDTH + px) * 4) as usize;
                pixels[i] = rgb[0];
                pixels[i + 1] = rgb[1];
                pixels[i + 2] = rgb[2];
                pixels[i + 3] = 255;
            }
        }
    }

    pixels
}

/// Color de un pixel de un tile concreto, con los patrones de cada material.
fn tile_color(tile: u32, x: u32, y: u32, noise: i32) -> [u8; 3] {
    match tile {
        // 0: hierba (arriba)
        0 => tint([95, 159, 53], noise),
        // 1: lateral de hierba (franja verde arriba, tierra debajo)
        1 => {
            if y < 5 {
                tint([95, 159, 53], noise)
            } else {
                tint([134, 96, 67], noise)
            }
        }
        // 2: tierra
        2 => tint([134, 96, 67], noise),
        // 3: piedra
        3 => tint([128, 128, 128], noise),
        // 4: arena
        4 => tint([219, 207, 163], noise),
        // 5: corteza (lineas verticales)
        5 => {
            let streak = if x.is_multiple_of(4) { -22 } else { 0 };
            tint([102, 76, 46], noise + streak)
        }
        // 6: anillos de la madera
        6 => {
            let ring = (((x as i32 - 8).pow(2) + (y as i32 - 8).pow(2)) % 6 == 0) as i32 * -25;
            tint([166, 130, 80], noise + ring)
        }
        // 7: hojas (con huecos oscuros)
        7 => {
            let gap = if (x + y).is_multiple_of(5) { -35 } else { 0 };
            tint([60, 120, 40], noise + gap)
        }
        _ => [0, 0, 0],
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
        // Todos los texels son opacos.
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn las_uvs_quedan_dentro_de_su_tile() {
        for tile in 0..TILES {
            let [u0, v0, u1, v1] = tile_uv_rect(tile as u16);
            assert!(u0 < u1 && v0 < v1);
            assert!(u0 >= 0.0 && u1 <= 1.0 && v0 >= 0.0 && v1 <= 1.0);
        }
    }
}
