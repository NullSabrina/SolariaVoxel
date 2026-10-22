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
fn corner_max_level<L: Fn(i32, i32, i32) -> u8 + ?Sized>(
    level: &L,
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
pub fn fluid_section<L, Q, G>(
    level: &L,
    query: &Q,
    light: &G,
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>)
where
    L: Fn(i32, i32, i32) -> u8 + ?Sized,
    Q: Fn(i32, i32, i32) -> Block + ?Sized,
    G: Fn(i32, i32, i32) -> (u8, u8) + ?Sized,
{
    // P2-18: una seccion normal no deberia reasignar varias veces.
    let mut vertices = Vec::with_capacity(512);
    let mut indices = Vec::with_capacity(768);
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

                // Se decide **antes** si la celda aporta geometria: una celda
                // rodeada de agua/solido no calcula esquinas (P2-18).
                let above_water = level(xi, yi + 1, zi) > 0;
                let above_solid = query(xi, yi + 1, zi).is_solid();
                let emit_top = !above_water && !above_solid;
                let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                let mut exposed = [false; 4];
                let mut any_side = false;
                for (k, (dx, dz)) in dirs.iter().enumerate() {
                    let (nx, nz) = (xi + dx, zi + dz);
                    if level(nx, yi, nz) == 0 && !query(nx, yi, nz).is_solid() {
                        exposed[k] = true;
                        any_side = true;
                    }
                }
                if !emit_top && !any_side {
                    continue;
                }

                // Esquinas del quad superior (nivel maximo en cada una).
                let h00 = corner_max_level(level, xi, yi, zi, 0, 0, own);
                let h10 = corner_max_level(level, xi, yi, zi, 1, 0, own);
                let h01 = corner_max_level(level, xi, yi, zi, 0, 1, own);
                let h11 = corner_max_level(level, xi, yi, zi, 1, 1, own);

                // Cara superior: solo si arriba no hay agua ni bloque solido.
                if emit_top {
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
                // En una **columna de caida** (agua encima) la cara lateral sube
                // hasta el borde del bloque (`1.0`), no hasta la superficie
                // (`0.875`): si no, queda una rendija de 2/16 entre bloques y la
                // cascada se ve a escalones en vez de continua.
                let cap = if above_water { 1.0 } else { 0.0 };
                let yh = |hl: u8| oy + yf + surface_height(hl).max(cap);
                for (k, (dx, dz)) in dirs.iter().enumerate() {
                    if !exposed[k] {
                        continue;
                    }
                    let (dx, dz) = (*dx, *dz);
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
                    // P2-19: v=1 abajo del tile, pero **escalado** a la altura
                    // visible (nivel 1..7 mide menos de un bloque). Con v=1 fijo
                    // la textura se comprimia verticalmente.
                    // p[0..1] son los vertices superiores; p[2] esta bajo p[1] y
                    // p[3] bajo p[0].
                    let hv0 = (p[0][1] - base).clamp(0.0, 1.0);
                    let hv1 = (p[1][1] - base).clamp(0.0, 1.0);
                    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, hv1], [0.0, hv0]];
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

    #[test]
    fn las_caras_de_agua_no_salen_contra_solido_ni_igual_nivel() {
        // Contamos quads (4 vertices cada uno): 1 celda aislada = 1 superior + 4
        // laterales = 5. Un vecino solido o agua de igual nivel quita una lateral.
        let mut levels: HashMap<(i32, i32, i32), u8> = HashMap::new();
        levels.insert((0, 0, 0), 8);
        let light = |_x: i32, _y: i32, _z: i32| (15u8, 0u8);
        let air = |_x: i32, _y: i32, _z: i32| Block::Air;
        let level = |x: i32, y: i32, z: i32| *levels.get(&(x, y, z)).unwrap_or(&0);
        let nquads = |v: &[Vertex]| v.len() / 4;

        let (v, _) = fluid_section(&level, &air, &light, 0, [0.0; 3]);
        assert_eq!(nquads(&v), 5, "1 celda aislada = 5 caras");

        // Solido en +X: la cara +X no se emite (4 caras).
        let solid = |x: i32, y: i32, z: i32| {
            if (x, y, z) == (1, 0, 0) {
                Block::Stone
            } else {
                Block::Air
            }
        };
        let (v, _) = fluid_section(&level, &solid, &light, 0, [0.0; 3]);
        assert_eq!(nquads(&v), 4, "cara de agua contra solido");

        // Agua de igual nivel en +X: 2 celdas, cada una con 3 laterales (no hay
        // pared entre agua) + su superior = 8 caras.
        levels.insert((1, 0, 0), 8);
        let level2 = |x: i32, y: i32, z: i32| *levels.get(&(x, y, z)).unwrap_or(&0);
        let (v, _) = fluid_section(&level2, &air, &light, 0, [0.0; 3]);
        assert_eq!(nquads(&v), 8, "cara de agua contra agua de igual nivel");
    }

    #[test]
    fn las_caras_laterales_parciales_no_comprimen_la_textura() {
        // P2-19: una celda de nivel 4 mide 0.4375; el borde inferior de la cara
        // lateral debe usar v=0.4375 (no 1.0) para no aplastar la textura.
        let mut levels: HashMap<(i32, i32, i32), u8> = HashMap::new();
        levels.insert((0, 0, 0), 4);
        let level = |x: i32, y: i32, z: i32| *levels.get(&(x, y, z)).unwrap_or(&0);
        let query = |_x: i32, _y: i32, _z: i32| Block::Air;
        let light = |_x: i32, _y: i32, _z: i32| (15u8, 0u8);
        let (v, _) = fluid_section(&level, &query, &light, 0, [0.0; 3]);
        let bottom = v
            .iter()
            .find(|v| (v.position[0] - 1.0).abs() < 1e-4 && v.position[1].abs() < 1e-4)
            .expect("falta el vertice inferior de la cara +X");
        assert!(
            (bottom.uv[1] - 0.4375).abs() < 1e-3,
            "v inferior {} (se esperaba 0.4375)",
            bottom.uv[1]
        );
        assert!(
            v.iter().all(|v| v.uv[1] <= 1.0 + 1e-4),
            "las UV no deben salirse del tile"
        );
    }

    #[test]
    fn test_uv_en_mundo_es_continua_entre_chunks_adyacentes() {
        // Dos columnas de agua de 16x16 adyacentes en X. El mesher de cada
        // chunk usa su propio `origin`, pero la UV debe ser la MISMA en los
        // vertices del borde compartido (x = 16): si fuera local, A daria 16 y
        // B daria 0 y al animar el shader apareceria una costura vertical.
        let level_at = |offset: i32| {
            move |x: i32, y: i32, z: i32| -> u8 {
                let wx = x + offset;
                if y == 0 && (0..32).contains(&wx) && (0..4).contains(&z) {
                    8
                } else {
                    0
                }
            }
        };
        let query = |_x: i32, _y: i32, _z: i32| Block::Air;
        let light = |_x: i32, _y: i32, _z: i32| (15u8, 0u8);
        let level_a = level_at(0);
        let level_b = level_at(16);
        let (va, _) = fluid_section(&level_a, &query, &light, 0, [0.0, 0.0, 0.0]);
        let (vb, _) = fluid_section(&level_b, &query, &light, 0, [16.0, 0.0, 0.0]);

        // Vertices de la cara superior sobre el plano x = 16, como
        // `[y, z, u, v]` (la UV de mundo es exactamente x, z).
        let boundary = |verts: &[Vertex]| -> Vec<[f32; 4]> {
            verts
                .iter()
                .filter(|v| {
                    // Solo la cara superior: sus UV son de mundo (x,z). Las
                    // laterales usan 0..1 por bloque y no comparten convenio.
                    (v.position[0] - 16.0).abs() < 1e-4 && v.position[1] > 0.5 && v.uv[0] > 1.5
                })
                .map(|v| [v.position[1], v.position[2], v.uv[0], v.uv[1]])
                .collect()
        };
        let mut a = boundary(&va);
        let mut b = boundary(&vb);
        assert!(!a.is_empty(), "el borde de A no tiene vertices de agua");
        let cmp = |l: &[f32; 4], r: &[f32; 4]| {
            l.iter()
                .zip(r)
                .map(|(x, y)| x.total_cmp(y))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        a.sort_by(cmp);
        b.sort_by(cmp);
        assert_eq!(a.len(), b.len(), "bordes con distinto numero de vertices");
        for (l, r) in a.iter().zip(&b) {
            for k in 0..4 {
                assert!(
                    (l[k] - r[k]).abs() < 1e-6,
                    "vertice del borde discontinuo: A={l:?} B={r:?}"
                );
            }
            // La UV del borde es la coordenada de mundo, no 0/1 local.
            assert!(
                (l[2] - 16.0).abs() < 1e-6,
                "UV local en el borde: {} (se esperaba 16)",
                l[2]
            );
        }
    }

    #[test]
    fn una_cascada_no_deja_rendijas_entre_bloques() {
        // Columna de caida (0,0,0) y (0,1,0): la cara lateral del bloque de abajo
        // debe subir hasta y=1.0 (borde del bloque), sin la rendija de 0.875.
        let mut levels: HashMap<(i32, i32, i32), u8> = HashMap::new();
        levels.insert((0, 0, 0), 8);
        levels.insert((0, 1, 0), 8);
        let level = |x: i32, y: i32, z: i32| *levels.get(&(x, y, z)).unwrap_or(&0);
        let query = |_x: i32, _y: i32, _z: i32| Block::Air;
        let light = |_x: i32, _y: i32, _z: i32| (15u8, 0u8);
        let (v, _) = fluid_section(&level, &query, &light, 0, [0.0; 3]);
        assert!(
            !v.iter()
                .any(|v| (v.position[0] - 1.0).abs() < 1e-4 && (v.position[1] - 0.875).abs() < 1e-4),
            "rendija de 2/16 en la cara de la cascada"
        );
        // Y el borde superior del bloque de abajo llega a y=1.0.
        assert!(
            v.iter()
                .any(|v| (v.position[0] - 1.0).abs() < 1e-4 && (v.position[1] - 1.0).abs() < 1e-4),
            "la cara de la cascada no llega al borde del bloque"
        );
    }
}
