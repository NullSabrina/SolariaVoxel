//! Fuente **bitmap 5x7** para el overlay de diagnostico (F3).
//!
//! No hay rasterizador de fuentes: definimos los glifos a mano como rejillas de
//! `5x7` (`#` = encendido) y los empaquetamos en un atlas de una sola textura
//! (blanco sobre transparente). Cada caracter del texto es **un quad** que
//! muestrea su celda del atlas (la forma la da el alfa).
//!
//! El texto se dibuja en MAYUSCULAS (los glifos son de una sola caja), como los
//! rotulos de un HUD; el overlay convierte con `to_ascii_uppercase`.

use crate::render::UiQuad;

/// Ancho de un glifo, en pixels.
pub const GLYPH_W: u32 = 5;
/// Alto de un glifo, en pixels.
pub const GLYPH_H: u32 = 7;
/// Ancho de celda en el atlas (glifo + 1 px de separacion).
pub const CELL_W: u32 = 6;
/// Alto de celda en el atlas.
pub const CELL_H: u32 = 8;
/// Glifos por fila del atlas.
pub const COLS: u32 = 16;
/// Primer codigo representado (espacio).
const FIRST: u32 = 32;
/// Numero de glifos representados (ASCII 32..=126).
const COUNT: u32 = 95;
/// Filas del atlas.
const ROWS: u32 = COUNT.div_ceil(COLS);
/// Ancho del atlas de fuente.
pub const FONT_W: u32 = COLS * CELL_W;
/// Alto del atlas de fuente.
pub const FONT_H: u32 = ROWS * CELL_H;

/// Capa de UI que samples la textura de fuente (las demas: `-1` interfaz,
/// `>= 0` tile del atlas de bloques).
pub const FONT_LAYER: i32 = -2;

/// Glifos `5x7` (7 filas de 5, separadas por `/`). Solo mayusculas, digitos y
/// simbolos; el resto se dibuja como un hueco.
const FONT: &[(char, &str)] = &[
    (' ', "...../...../...../...../...../...../....."),
    ('A', ".###./#...#/#...#/#####/#...#/#...#/#...#"),
    ('B', "####./#...#/#...#/####./#...#/#...#/####."),
    ('C', ".###./#...#/#..../#..../#..../#...#/.###."),
    ('D', "####./#...#/#...#/#...#/#...#/#...#/####."),
    ('E', "#####/#..../#..../####./#..../#..../#####"),
    ('F', "#####/#..../#..../####./#..../#..../#...."),
    ('G', ".###./#...#/#..../#.###/#...#/#...#/.###."),
    ('H', "#...#/#...#/#...#/#####/#...#/#...#/#...#"),
    ('I', "#####/..#../..#../..#../..#../..#../#####"),
    ('J', "..###/...#./...#./...#./...#./#..#./.##.."),
    ('K', "#...#/#..#./#.#../##.../#.#../#..#./#...#"),
    ('L', "#..../#..../#..../#..../#..../#..../#####"),
    ('M', "#...#/##.##/#.#.#/#...#/#...#/#...#/#...#"),
    ('N', "#...#/##..#/#.#.#/#..##/#...#/#...#/#...#"),
    ('O', ".###./#...#/#...#/#...#/#...#/#...#/.###."),
    ('P', "####./#...#/#...#/####./#..../#..../#...."),
    ('Q', ".###./#...#/#...#/#...#/#.#.#/#..#./.##.#"),
    ('R', "####./#...#/#...#/####./#.#../#..#./#...#"),
    ('S', ".####/#..../#..../.###./....#/....#/####."),
    ('T', "#####/..#../..#../..#../..#../..#../..#.."),
    ('U', "#...#/#...#/#...#/#...#/#...#/#...#/.###."),
    ('V', "#...#/#...#/#...#/#...#/#...#/.#.#./..#.."),
    ('W', "#...#/#...#/#...#/#...#/#.#.#/##.##/#...#"),
    ('X', "#...#/#...#/.#.#./..#../.#.#./#...#/#...#"),
    ('Y', "#...#/#...#/.#.#./..#../..#../..#../..#.."),
    ('Z', "#####/....#/...#./..#../.#.../#..../#####"),
    ('0', ".###./#...#/#..##/#.#.#/##..#/#...#/.###."),
    ('1', "..#../.##../..#../..#../..#../..#../.###."),
    ('2', ".###./#...#/....#/...#./..#../.#.../#####"),
    ('3', "#####/...#./..#../...#./....#/#...#/.###."),
    ('4', "...#./..##./.#.#./#..#./#####/...#./...#."),
    ('5', "#####/#..../####./....#/....#/#...#/.###."),
    ('6', "..##./.#.../#..../####./#...#/#...#/.###."),
    ('7', "#####/....#/...#./..#../.#.../.#.../.#..."),
    ('8', ".###./#...#/#...#/.###./#...#/#...#/.###."),
    ('9', ".###./#...#/#...#/.####/....#/...#./.##.."),
    (':', "...../..#../..#../...../..#../..#../....."),
    ('.', "...../...../...../...../...../.##../.##.."),
    ('/', "....#/....#/...#./..#../.#.../#..../#...."),
    ('-', "...../...../...../.###./...../...../....."),
    ('%', "##..#/##.#./..#../.#.../#..##/#..##/....."),
    ('(', "...#./..#../.#.../.#.../.#.../..#../...#."),
    (')', ".#.../..#../...#./...#./...#./..#../.#..."),
    (',', "...../...../...../...../..#../..#../.#..."),
    ('+', "...../..#../..#../#####/..#../..#../....."),
    ('<', "...#./..#../.#.../#..../.#.../..#../...#."),
    ('>', ".#.../..#../...#./....#/...#./..#../.#..."),
    ('=', "...../...../#####/...../#####/...../....."),
    ('_', "...../...../...../...../...../...../#####"),
    ('?', ".###./#...#/....#/...#./..#../...../..#.."),
    ('#', ".#.#./.#.#./#####/.#.#./#####/.#.#./.#.#."),
    ('[', ".###./.#.../.#.../.#.../.#.../.#.../.###."),
    (']', ".###./...#./...#./...#./...#./...#./.###."),
    ('!', "..#../..#../..#../..#../..#../...../..#.."),
    ('\'', "..#../..#../...../...../...../...../....."),
    ('*', "...../#.#.#/.###./#####/.###./#.#.#/....."),
];

/// Patron de un caracter (`7` filas de `5`), o `None` si no hay glifo.
fn pattern(ch: char) -> Option<&'static str> {
    let up = ch.to_ascii_uppercase();
    FONT.iter().find(|(c, _)| *c == up).map(|(_, p)| *p)
}

/// Imagen RGBA del atlas de fuente: glifos **blancos** sobre transparente.
pub fn build_pixels() -> Vec<u8> {
    let mut px = vec![0u8; (FONT_W * FONT_H * 4) as usize];
    for ch in (FIRST..FIRST + COUNT).filter_map(char::from_u32) {
        let Some(pat) = pattern(ch) else { continue };
        let idx = ch as u32 - FIRST;
        let (col, row) = (idx % COLS, idx / COLS);
        for (ly, line) in pat.split('/').enumerate() {
            for (lx, c) in line.chars().enumerate() {
                if c != '#' {
                    continue;
                }
                let x = col * CELL_W + lx as u32;
                let y = row * CELL_H + ly as u32;
                if x >= FONT_W || y >= FONT_H {
                    continue;
                }
                let i = ((y * FONT_W + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    px
}

/// UV normalizada del glifo `ch`, o `None` si no se dibuja.
pub fn glyph_uv(ch: char) -> Option<[f32; 4]> {
    pattern(ch)?;
    let idx = (ch.to_ascii_uppercase() as u32).wrapping_sub(FIRST);
    if idx >= COUNT {
        return None;
    }
    let (col, row) = (idx % COLS, idx / COLS);
    let (x, y) = (col * CELL_W, row * CELL_H);
    Some([
        x as f32 / FONT_W as f32,
        y as f32 / FONT_H as f32,
        (x + GLYPH_W) as f32 / FONT_W as f32,
        (y + GLYPH_H) as f32 / FONT_H as f32,
    ])
}

/// Ancho en pixels de `text` a la escala `scale`.
pub fn text_width(text: &str, scale: f32) -> f32 {
    text.chars().count() as f32 * CELL_W as f32 * scale
}

/// Convierte `text` en quads (un glifo por caracter) con esquina superior
/// izquierda en `(x, y)`.
pub fn text_quads(text: &str, x: f32, y: f32, scale: f32) -> Vec<UiQuad> {
    let gw = GLYPH_W as f32 * scale;
    let gh = GLYPH_H as f32 * scale;
    let advance = CELL_W as f32 * scale;
    let mut quads = Vec::with_capacity(text.chars().count());
    let mut cx = x;
    for ch in text.chars() {
        // El espacio (y cualquier glifo sin pixel encendido) no gasta quad.
        if ch != ' '
            && let Some(uv) = glyph_uv(ch)
        {
            quads.push(UiQuad {
                rect: [cx, y, gw, gh],
                uv,
                layer: FONT_LAYER,
            });
        }
        cx += advance;
    }
    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_atlas_tiene_el_tamano_esperado() {
        let px = build_pixels();
        assert_eq!(px.len(), (FONT_W * FONT_H * 4) as usize);
    }

    #[test]
    fn hay_glifo_para_letras_y_digitos() {
        for ch in "ABCXYZ019:./-%".chars() {
            assert!(glyph_uv(ch).is_some(), "falta glifo para {ch}");
        }
        assert!(glyph_uv('a').is_some(), "deberia aceptar minusculas");
    }

    #[test]
    fn el_texto_genera_un_quad_por_caracter_visible() {
        let q = text_quads("AB C", 0.0, 0.0, 2.0);
        // El espacio no genera quad.
        assert_eq!(q.len(), 3);
        assert!(q.iter().all(|quad| quad.layer == FONT_LAYER));
        // C es el 3er caracter ("AB C"), a 3 celdas del origen.
        assert!((q[2].rect[0] - 3.0 * CELL_W as f32 * 2.0).abs() < 1e-3);
    }
}
