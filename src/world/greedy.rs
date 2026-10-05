//! Greedy meshing: fusiona caras contiguas del mismo material en rectangulos.
//!
//! El mesher "naive" emite una cara por cada cara de voxel que da al aire. Si
//! tienes una pared de 16x16 bloques de piedra, eso son 256 caras (512
//! triangulos). El greedy meshing recorre cada capa, agrupa celdas contiguas
//! **del mismo tipo de cara** (mismo bloque + misma orientacion) y emite **un
//! solo rectangulo** por grupo: la misma pared pasa a 1 cara (2 triangulos).
//!
//! Algoritmo (por cada uno de los 6 planos de la rejilla):
//! 1. Recorremos el plano capa a capa.
//! 2. Construimos una "mascara" 2D: por cada celda del plano, que tipo de cara
//!    asoma (o `None` si la cara esta oculta). Como cada eje del plano tiene 16
//!    posiciones, la mascara es 16x16.
//! 3. Sobre esa mascara, buscamos el rectangulo mas grande posible (crecer a lo
//!    ancho y luego a lo alto) y lo emitimos como 4 vertices + 2 triangulos.
//! 4. Marcamos esas celdas como consumidas y repetimos.
//!
//! Nota: dos caras del mismo bloque pero distinto `tile` (p.ej. la hierba tiene
//! cara lateral distinta a la de arriba) NO se fusionan.

use super::block::{Block, Face};
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};
use crate::render::mesh::Vertex;

/// Una cara candidata: identifica que asoma en una celda del plano.
///
/// Solo se fusionan celdas cuyo `(block_id, face)` coincide, por eso agrupamos
/// por ambos. (Como `face` ya determina el tile dentro de un mismo bloque, esto
/// basta para no mezclar texturas.)
#[derive(Clone, Copy, PartialEq, Eq)]
struct FaceKey {
    block: u8,
    face: Face,
    /// Luz de cielo de la celda de aire que hay delante de la cara (0..15).
    sky: u8,
    /// Luz de bloque (antorchas) de esa misma celda (0..15).
    block_light: u8,
}

/// Genera la malla de una columna entera con greedy meshing.
///
/// Devuelve `(vertices, indices)` en **coordenadas de mundo** (ya sumado
/// `origin`). `origin` desplaza la columna (0..16) a su sitio del mundo.
pub fn greedy_column(column: &Column, origin: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let query = |x: i32, y: i32, z: i32| column.get_or_air(x, y, z);
    let light = |x: i32, y: i32, z: i32| {
        (
            column.light_or_zero(x, y, z),
            column.block_light_or_zero(x, y, z),
        )
    };
    greedy_range(&query, &light, 0, WORLD_HEIGHT, origin)
}

/// Greedy meshing de una seccion concreta (16 capas).
pub fn greedy_section(
    column: &Column,
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>) {
    let query = |x: i32, y: i32, z: i32| column.get_or_air(x, y, z);
    let light = |x: i32, y: i32, z: i32| {
        (
            column.light_or_zero(x, y, z),
            column.block_light_or_zero(x, y, z),
        )
    };
    greedy_section_query(&query, &light, section, origin)
}

/// Greedy meshing de una seccion usando consultas de bloque y luz **externas**.
///
/// `query(x, y, z)` devuelve el bloque en coordenadas **locales** de la columna
/// (puede mirar fuera, 0..16, para el vecino: eso es lo que evita los muros
/// internos). `light(x, y, z)` devuelve `(cielo, bloque)` 0..15 de esa celda.
pub fn greedy_section_query(
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>) {
    let y_start = section * CHUNK_SIZE;
    let y_end = (y_start + CHUNK_SIZE).min(WORLD_HEIGHT);
    greedy_range(query, light, y_start, y_end, origin)
}

/// Nucleo del greedy meshing sobre el rango vertical `[y_start, y_end)`.
///
/// La mascara 2D siempre es 16x16, pero **que recorren `u` y `v` cambia** segun
/// la cara:
/// * +X/-X: `u` = z (0..16), `v` = y (limitado a `[y_start, y_end)`), `c` = x.
/// * +Z/-Z: `u` = x (0..16), `v` = y (limitado), `c` = z.
/// * +Y/-Y: `u` = x (0..16), `v` = z (0..16), `c` = y (limitado).
///
/// Por eso el rango vertical acota `v` en caras verticales y `c` en las
/// horizontales, y el "alto" del rectangulo nunca cruza el limite de seccion.
fn greedy_range(
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    y_start: usize,
    y_end: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for face in Face::ALL {
        let horizontal = matches!(face, Face::PosY | Face::NegY);
        // Numero de capas a lo largo del eje de la normal.
        let layers = if horizontal {
            (y_start, y_end) // eje Y acotado al rango
        } else {
            (0, CHUNK_SIZE) // eje X o Z completo
        };
        // Rango de `v` (el "alto" del plano).
        let v_range = if horizontal {
            (0, CHUNK_SIZE) // v = z
        } else {
            (y_start, y_end) // v = y
        };
        let v_hi = v_range.1 - v_range.0;

        for c in layers.0..layers.1 {
            // Mascara 2D: `u` en 0..16 y `v` en 0..v_hi (que para una columna
            // entera puede ser 384). La guardamos como `Vec<Vec<..>>` porque el
            // alto cambia; para el caso por secciones (16) es una 16x16 normal.
            let mut mask: Vec<Vec<Option<FaceKey>>> = vec![vec![None; v_hi]; CHUNK_SIZE];

            for (v, _) in (0..v_hi).enumerate() {
                let world_v = v_range.0 + v;
                for (u, u_mask_col) in mask.iter_mut().enumerate() {
                    u_mask_col[v] = mask_value(query, light, face, u, world_v, c);
                }
            }

            // Extraemos rectangulos de la mascara.
            let mut v = 0;
            while v < v_hi {
                let mut u = 0;
                while u < CHUNK_SIZE {
                    let Some(key) = mask[u][v] else {
                        u += 1;
                        continue;
                    };

                    // Ancho: cuanto se repite `key` hacia +u.
                    let mut width = 1;
                    while u + width < CHUNK_SIZE && mask[u + width][v] == Some(key) {
                        width += 1;
                    }
                    // Alto: cuantas filas completas se repiten.
                    let mut height = 1;
                    'outer: while v + height < v_hi {
                        for du in 0..width {
                            if mask[u + du][v + height] != Some(key) {
                                break 'outer;
                            }
                        }
                        height += 1;
                    }

                    // Emitimos el rectangulo. `v` local se convierte a coordenada
                    // real del plano.
                    emit_quad(
                        &mut vertices,
                        &mut indices,
                        face,
                        u,
                        v_range.0 + v,
                        c,
                        width,
                        height,
                        key,
                        origin,
                    );

                    for dv in 0..height {
                        for du in 0..width {
                            mask[u + du][v + dv] = None;
                        }
                    }

                    u += width;
                }
                v += 1;
            }
        }
    }

    // Antorchas: geometria propia (dos quads cruzados), no entra en el greedy
    // porque cada una es un objeto fino, no una cara de cubo. Las caras de los
    // bloques solidos vecinos ya se han emitido arriba: una antorcha no ocluye
    // (el vecino solo oculta si es solido).
    for y in y_start..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if query(x as i32, y as i32, z as i32) != Block::Torch {
                    continue;
                }
                let (sky, block_light) = light(x as i32, y as i32, z as i32);
                let sky_f = sky as f32 / super::chunk::MAX_LIGHT as f32;
                let block_f = block_light as f32 / super::chunk::MAX_LIGHT as f32;
                super::mesher::emit_torch_cross(
                    &mut vertices,
                    &mut indices,
                    origin,
                    x,
                    y,
                    z,
                    Block::Torch,
                    sky_f,
                    block_f,
                );
            }
        }
    }

    (vertices, indices)
}

/// Decide que asoma en la celda `(u, v)` del plano `face`, en la capa `c`.
///
/// * Para caras +X/-X: `u` recorre el eje Z, `v` el eje Y, `c` el eje X.
/// * Para caras +Z/-Z: `u` recorre el eje X, `v` el eje Y, `c` el eje Z.
/// * Para caras +Y/-Y: `u` recorre el eje X, `v` el eje Z, `c` el eje Y.
fn mask_value(
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    face: Face,
    u: usize,
    v: usize,
    c: usize,
) -> Option<FaceKey> {
    // Coordenadas del voxel segun la cara.
    let (x, y, z) = match face {
        Face::PosX | Face::NegX => (c, v, u),
        Face::PosZ | Face::NegZ => (u, v, c),
        Face::PosY | Face::NegY => (u, c, v),
    };

    let block = query(x as i32, y as i32, z as i32);
    // Solo los bloques solidos aportan caras de cubo. La antorcha (visible pero
    // no solida) se emite aparte, como una cruz de dos planos (ver el bucle de
    // antorchas en `greedy_range`).
    if !block.is_solid() {
        return None;
    }
    let (ox, oy, oz) = face.offset();
    // El vecino puede estar fuera de la columna (otro chunk): la query decide.
    let (nx, ny, nz) = (x as i32 + ox, y as i32 + oy, z as i32 + oz);
    let neighbor = query(nx, ny, nz);
    // Solo se oculta la cara si el vecino es SOLIDO (una antorcha no tapa).
    if neighbor.is_solid() {
        return None; // cara oculta (o vecino en otro chunk)
    }
    // La luz que recibe esta cara es la de la celda de aire de delante.
    let (sky, block_light) = light(nx, ny, nz);
    Some(FaceKey {
        block: block.id(),
        face,
        sky,
        block_light,
    })
}

/// Emite un rectangulo `width` x `height` como 4 vertices + 2 triangulos.
#[allow(clippy::too_many_arguments)]
fn emit_quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    face: Face,
    u: usize,
    v: usize,
    c: usize,
    width: usize,
    height: usize,
    key: FaceKey,
    origin: [f32; 3],
) {
    let block = super::block::Block::from_u8(key.block);
    let tile = block.face_tile(face);

    // Las 4 esquinas en coordenadas de mundo, segun la cara. `du`/`dv` son los
    // incrementos del plano (ya en unidades de bloque). El rectangulo va de
    // `(u, v)` a `(u+width, v+height)` pero `1` de grosor en el eje de la normal.
    let (u0, v0) = (u as f32, v as f32);
    let (u1, v1) = ((u + width) as f32, (v + height) as f32);
    let cf = c as f32;

    // UVs en unidades de tile: una cara de W x H bloques usa 0..W, 0..H para
    // que el tile se repita por bloque (el sampler repite). Asi no se estira.
    let (w, h) = (width as f32, height as f32);

    // `origin` desplaza la columna a su sitio del mundo.
    let (ox, oy, oz) = (origin[0], origin[1], origin[2]);

    // Devolvemos posiciones [x, y, z] para las 4 esquinas y las UV.
    let (corners, uvs): ([[f32; 3]; 4], [[f32; 2]; 4]) = match face {
        Face::PosX => {
            // Plano en x = c+1. u -> z, v -> y.
            let x = cf + 1.0;
            (
                [[x, v0, u1], [x, v0, u0], [x, v1, u0], [x, v1, u1]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
        Face::NegX => {
            let x = cf;
            (
                [[x, v0, u0], [x, v0, u1], [x, v1, u1], [x, v1, u0]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
        Face::PosY => {
            // Plano en y = c+1. u -> x, v -> z.
            let y = cf + 1.0;
            (
                [[u0, y, v0], [u1, y, v0], [u1, y, v1], [u0, y, v1]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
        Face::NegY => {
            let y = cf;
            (
                [[u0, y, v1], [u1, y, v1], [u1, y, v0], [u0, y, v0]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
        Face::PosZ => {
            // Plano en z = c+1. u -> x, v -> y.
            let z = cf + 1.0;
            (
                [[u0, v0, z], [u1, v0, z], [u1, v1, z], [u0, v1, z]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
        Face::NegZ => {
            let z = cf;
            (
                [[u1, v0, z], [u0, v0, z], [u0, v1, z], [u1, v1, z]],
                [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
            )
        }
    };

    let base = vertices.len() as u32;
    for (corner, uv) in corners.iter().zip(uvs.iter()) {
        // Luces normalizadas 0..1 (la cara recibe las de la celda de delante).
        let sky = key.sky as f32 / super::chunk::MAX_LIGHT as f32;
        let block = key.block_light as f32 / super::chunk::MAX_LIGHT as f32;
        vertices.push(Vertex::with_light(
            [corner[0] + ox, corner[1] + oy, corner[2] + oz],
            *uv,
            sky,
            block,
            tile as u32,
        ));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    #[test]
    fn un_solo_bloque_da_6_caras() {
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Stone);
        let (vertices, indices) = greedy_column(&column, [0.0; 3]);
        assert_eq!(vertices.len(), 6 * 4, "6 caras x 4 vertices");
        assert_eq!(indices.len(), 6 * 6, "6 caras x 2 triangulos");
    }

    #[test]
    fn una_pared_plana_se_fusiona_en_un_rectangulo() {
        // Encima de una base solida, ponemos una capa 16x16 de hierba: su cara
        // superior debe fusionarse en UN solo rectangulo (4 vertices), no 256.
        let mut column = Column::empty();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                column.set(x, 0, z, Block::Stone); // base
                column.set(x, 1, z, Block::Grass); // capa visible por arriba
            }
        }
        let (gv, gi) = greedy_column(&column, [0.0; 3]);
        let (nv, ni) = crate::world::mesher::mesh_column(&column, [0.0, 0.0, 0.0])
            .into_iter()
            .fold((0, 0), |acc, s| {
                (acc.0 + s.vertices.len(), acc.1 + s.indices.len())
            });

        // El greedy debe usar MUCHOS menos vertices que el naive.
        assert!(gv.len() < nv, "greedy {} vs naive {nv}", gv.len());
        assert!(gi.len() < ni);
    }

    #[test]
    fn los_indices_son_validos() {
        let column = Column::generate_demo();
        let (vertices, indices) = greedy_column(&column, [0.0; 3]);
        assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));
        assert_eq!(indices.len() % 3, 0);
    }

    #[test]
    fn greedy_reduce_mucho_la_geometria_en_una_capa_plana() {
        // Una capa de 16x16 de hierba sobre piedra: la superficie superior es un
        // unico rectangulo en greedy, pero 256 caras en naive.
        let mut column = Column::empty();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                column.set(x, 0, z, Block::Stone);
                column.set(x, 1, z, Block::Grass);
            }
        }
        let (_, gi) = greedy_column(&column, [0.0; 3]);
        let naive: usize = crate::world::mesher::mesh_column(&column, [0.0; 3])
            .iter()
            .map(|s| s.indices.len())
            .sum();
        // Al menos un 80% menos de indices.
        assert!(gi.len() * 5 < naive, "greedy {} vs naive {naive}", gi.len());
    }

    #[test]
    fn la_antorcha_se_dibuja_como_cruz_no_como_cubo() {
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Torch);
        let (vertices, indices) = greedy_column(&column, [0.0; 3]);
        // Dos planos x 4 vertices, dos planos x 2 triangulos (nada de 6 caras).
        assert_eq!(vertices.len(), 8, "dos quads cruzados");
        assert_eq!(indices.len(), 12);
        assert!(vertices.iter().all(|v| v.tile == 8));
    }

    #[test]
    fn la_antorcha_no_oculta_las_caras_vecinas() {
        // Piedra con antorcha en su cara +X: la cara de la piedra se conserva.
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Stone);
        column.set(9, 8, 8, Block::Torch);
        let (vertices, _) = greedy_column(&column, [0.0; 3]);
        assert_eq!(vertices.len(), 6 * 4 + 8, "6 caras de piedra + cruz");
    }

    #[test]
    fn la_antorcha_emite_luz_de_bloque_en_sus_vertices() {
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Torch);
        column.compute_block_light();
        let (vertices, _) = greedy_column(&column, [0.0; 3]);
        // La cruz de la antorcha lleva luz de bloque (la de la propia celda).
        assert!(
            vertices.iter().any(|v| v.block > 0.0),
            "la antorcha deberia emitir luz de bloque en sus vertices"
        );
    }

    #[test]
    fn la_luz_de_cielo_y_la_de_bloque_van_separadas() {
        // Suelo de piedra a cielo abierto: la cara superior recibe cielo (sky ~1)
        // y nada de bloque (block 0). Asi el dia/noche puede apagar solo el cielo.
        let mut column = Column::empty();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                column.set(x, 0, z, Block::Stone);
            }
        }
        column.compute_skylight();
        column.compute_block_light();
        let (vertices, _) = greedy_column(&column, [0.0; 3]);
        assert!(
            vertices.iter().any(|v| v.sky > 0.9 && v.block == 0.0),
            "deberia haber caras con cielo alto y bloque nulo"
        );
    }
}
