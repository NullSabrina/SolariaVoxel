//! Mesher de **superficie fluida**: dibuja el agua como una lamina continua,
//! no como cubos apilados.
//!
//! Idea: en vez de que la cara superior de cada celda de agua sea plana en
//! `y + 1.0`, cada **esquina** del quad superior sube a `y + nivel/8`, tomando el
//! **maximo** del nivel propio y el de las celdas que comparten esa esquina. Asi
//! una celda de nivel 8 junto a una de nivel 4 forma una **rampa** (la esquina
//! compartida sube a 1.0 y la esquina opuesta de la celda baja se queda en 0.5),
//! en lugar de un escalon.
//!
//! Solo se emite la cara superior (si arriba no hay agua ni solido) y las caras
//! laterales **expuestas al aire** (no entre dos celdas de agua): el volumen
//! interior del agua no genera geometria.

use crate::render::mesh::Vertex;
use crate::world::block::{Block, Face};
use crate::world::chunk::{CHUNK_SIZE, MAX_LIGHT, WORLD_HEIGHT};
use crate::world::water::MAX_LEVEL;

/// Altura de la superficie (0..1) de una celda de nivel `level`.
///
/// No llena el bloque: deja 2/16 libres arriba, asi la superficie queda **por
/// debajo** del borde del bloque (como Minecraft) y se ve que es liquido, no un
/// cubo macizo.
#[inline]
pub fn surface_height(level: u8) -> f32 {
    (level.min(MAX_LEVEL) as f32 / MAX_LEVEL as f32) * (14.0 / 16.0)
}

/// Nivel maximo de las celdas de agua que comparten la esquina `(x+dx, z+dz)`,
/// con `dx`/`dz` en `0..=1`. Se toma el maximo para que la esquina se eleve
/// hacia la celda vecina mas alta y el borde quede inclinado.
fn corner_max_level(
    level: &dyn Fn(i32, i32, i32) -> u8,
    x: i32,
    y: i32,
    z: i32,
    dx: i32,
    dz: i32,
    own: u8,
) -> u8 {
    let mut m = own;
    for cx in [x + dx - 1, x + dx] {
        for cz in [z + dz - 1, z + dz] {
            let l = level(cx, y, cz);
            if l > m {
                m = l;
            }
        }
    }
    m
}

/// Empuja un quad (4 vertices + 2 triangulos).
fn push_quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    p: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    sky: f32,
    block: f32,
    tile: u32,
) {
    let base = vertices.len() as u32;
    for k in 0..4 {
        vertices.push(Vertex::with_light(p[k], uv[k], sky, block, tile));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Genera la malla de agua de una seccion.
///
/// * `level(x,y,z)` — nivel de agua en coordenadas **locales** (puede salirse a
///   -1 / 16 para mirar el chunk vecino); `0` = sin agua.
/// * `query(x,y,z)` — bloque (`is_solid` decide si una cara lateral queda oculta).
/// * `light(x,y,z)` — `(cielo, bloque)` 0..15 de la celda.
pub fn fluid_section(
    level: &dyn Fn(i32, i32, i32) -> u8,
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let (ox, oy, oz) = (origin[0], origin[1], origin[2]);
    let y_start = section * CHUNK_SIZE;
    let y_end = (y_start + CHUNK_SIZE).min(WORLD_HEIGHT);
    let tile = Block::Water.face_tile(Face::PosY) as u32;

    for y in y_start..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let own = level(x as i32, y as i32, z as i32);
                if own == 0 {
                    continue;
                }
                let (xi, yi, zi) = (x as i32, y as i32, z as i32);
                let (sky, block_light) = light(xi, yi, zi);
                let sky_f = sky as f32 / MAX_LIGHT as f32;
                let block_f = block_light as f32 / MAX_LIGHT as f32;
                let (xf, yf, zf) = (x as f32, y as f32, z as f32);

                // Esquinas del quad superior (nivel maximo en cada una).
                let h00 = corner_max_level(level, xi, yi, zi, 0, 0, own);
                let h10 = corner_max_level(level, xi, yi, zi, 1, 0, own);
                let h01 = corner_max_level(level, xi, yi, zi, 0, 1, own);
                let h11 = corner_max_level(level, xi, yi, zi, 1, 1, own);

                // Cara superior: solo si arriba no hay agua ni bloque solido.
                let above_water = level(xi, yi + 1, zi) > 0;
                let above_solid = query(xi, yi + 1, zi).is_solid();
                if !above_water && !above_solid {
                    let yh = |hl: u8| oy + yf + surface_height(hl);
                    let p = [
                        [ox + xf, yh(h00), oz + zf],
                        [ox + xf + 1.0, yh(h10), oz + zf],
                        [ox + xf + 1.0, yh(h11), oz + zf + 1.0],
                        [ox + xf, yh(h01), oz + zf + 1.0],
                    ];
                    // UV en coordenadas de mundo: el sampler repite y el shader
                    // las desplaza con el tiempo para simular la corriente.
                    let uv = [
                        [ox + xf, oz + zf],
                        [ox + xf + 1.0, oz + zf],
                        [ox + xf + 1.0, oz + zf + 1.0],
                        [ox + xf, oz + zf + 1.0],
                    ];
                    push_quad(&mut vertices, &mut indices, p, uv, sky_f, block_f, tile);
                }

                // Caras laterales expuestas (no contra agua ni contra solido).
                let base = oy + yf;
                let yh = |hl: u8| oy + yf + surface_height(hl);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (xi + dx, zi + dz);
                    if level(nx, yi, nz) > 0 || query(nx, yi, nz).is_solid() {
                        continue;
                    }
                    // Arista superior inclinada del lado expuesto + base plana,
                    // para que no queden huecos con la celda de abajo.
                    let p = match (dx, dz) {
                        (1, 0) => [
                            [ox + xf + 1.0, yh(h10), oz + zf],
                            [ox + xf + 1.0, yh(h11), oz + zf + 1.0],
                            [ox + xf + 1.0, base, oz + zf + 1.0],
                            [ox + xf + 1.0, base, oz + zf],
                        ],
                        (-1, 0) => [
                            [ox + xf, yh(h01), oz + zf + 1.0],
                            [ox + xf, yh(h00), oz + zf],
                            [ox + xf, base, oz + zf],
                            [ox + xf, base, oz + zf + 1.0],
                        ],
                        (0, 1) => [
                            [ox + xf + 1.0, yh(h11), oz + zf + 1.0],
                            [ox + xf, yh(h01), oz + zf + 1.0],
                            [ox + xf, base, oz + zf + 1.0],
                            [ox + xf + 1.0, base, oz + zf + 1.0],
                        ],
                        _ => [
                            [ox + xf, yh(h00), oz + zf],
                            [ox + xf + 1.0, yh(h10), oz + zf],
                            [ox + xf + 1.0, base, oz + zf],
                            [ox + xf, base, oz + zf],
                        ],
                    };
                    let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                    push_quad(&mut vertices, &mut indices, p, uv, sky_f, block_f, tile);
                }
            }
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn la_altura_de_superficie_es_nivel_entre_ocho_menos_dos_px() {
        assert!((surface_height(8) - 0.875).abs() < 1e-6);
        assert!((surface_height(4) - 0.4375).abs() < 1e-6);
        assert!((surface_height(1) - 0.109_375).abs() < 1e-6);
        // Niveles 8 y 4 -> 4/8 del alto de agua (0.875 - 0.4375 = 0.4375).
        assert!((surface_height(8) - surface_height(4) - 0.4375).abs() < 1e-6);
    }

    #[test]
    fn dos_celdas_adyacentes_8_y_4_forman_rampa() {
        let mut levels: HashMap<(i32, i32, i32), u8> = HashMap::new();
        levels.insert((0, 0, 0), 8);
        levels.insert((1, 0, 0), 4);
        let level = |x: i32, y: i32, z: i32| *levels.get(&(x, y, z)).unwrap_or(&0);
        let query = |_x: i32, _y: i32, _z: i32| Block::Air;
        let light = |_x: i32, _y: i32, _z: i32| (15u8, 0u8);

        let (vertices, indices) = fluid_section(&level, &query, &light, 0, [0.0; 3]);
        assert!(!vertices.is_empty() && indices.len() % 3 == 0);

        // Alturas presentes: 0.0 (base), 0.4375 (nivel 4) y 0.875 (nivel 8).
        let mut ys: Vec<f32> = vertices.iter().map(|v| v.position[1]).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        let h8 = ys
            .iter()
            .copied()
            .find(|y| (y - 0.875).abs() < 1e-4)
            .expect("falta la superficie de nivel 8");
        let h4 = ys
            .iter()
            .copied()
            .find(|y| (y - 0.4375).abs() < 1e-4)
            .expect("falta la superficie de nivel 4");
        assert!(((h8 - h4) - 0.4375).abs() < 1e-4, "diferencia {h8}-{h4}");
    }
}
