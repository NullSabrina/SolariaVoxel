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

/// Combina una malla opaca y una de agua en una sola (offset de indices).
fn merge_meshes(
    mut v: Vec<Vertex>,
    mut i: Vec<u32>,
    wv: Vec<Vertex>,
    wi: Vec<u32>,
) -> (Vec<Vertex>, Vec<u32>) {
    let base = v.len() as u32;
    v.extend(wv);
    i.extend(wi.into_iter().map(|x| x + base));
    (v, i)
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
    let (v, i, wv, wi) = greedy_range(&query, &light, 0, WORLD_HEIGHT, origin);
    merge_meshes(v, i, wv, wi)
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
    let (v, i, wv, wi) = greedy_section_query(&query, &light, section, origin);
    merge_meshes(v, i, wv, wi)
}

/// Greedy meshing de una seccion usando consultas de bloque y luz **externas**.
///
/// `query(x, y, z)` devuelve el bloque en coordenadas **locales** de la columna
/// (puede mirar fuera, 0..16, para el vecino: eso es lo que evita los muros
/// internos). `light(x, y, z)` devuelve `(cielo, bloque)` 0..15 de esa celda.
///
/// Devuelve `(vertices_opacos, indices_opacos, vertices_agua, indices_agua)`: el
/// agua va aparte porque se dibuja en un **pase translucido** distinto.
#[allow(clippy::type_complexity)]
pub fn greedy_section_query(
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>, Vec<Vertex>, Vec<u32>) {
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
#[allow(clippy::type_complexity)]
fn greedy_range(
    query: &dyn Fn(i32, i32, i32) -> Block,
    light: &dyn Fn(i32, i32, i32) -> (u8, u8),
    y_start: usize,
    y_end: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>, Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut water_vertices = Vec::new();
    let mut water_indices = Vec::new();

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

        // Mascara 2D **plana y reutilizada** entre capas: una sola reserva por
        // cara (antes se reservaba un `Vec<Vec>` por capa, miles de asignaciones
        // por seccion y el grueso del coste de meshear). `idx(u, v) = u*v_hi+v`.
        let mut mask: Vec<Option<FaceKey>> = vec![None; CHUNK_SIZE * v_hi];

        for c in layers.0..layers.1 {
            mask.fill(None);
            for v in 0..v_hi {
                let world_v = v_range.0 + v;
                for u in 0..CHUNK_SIZE {
                    mask[u * v_hi + v] = mask_value(query, light, face, u, world_v, c);
                }
            }

            // Extraemos rectangulos de la mascara.
            let mut v = 0;
            while v < v_hi {
                let mut u = 0;
                while u < CHUNK_SIZE {
                    let Some(key) = mask[u * v_hi + v] else {
                        u += 1;
                        continue;
                    };

                    // Ancho: cuanto se repite `key` hacia +u.
                    let mut width = 1;
                    while u + width < CHUNK_SIZE && mask[(u + width) * v_hi + v] == Some(key) {
                        width += 1;
                    }
                    // Alto: cuantas filas completas se repiten.
                    let mut height = 1;
                    'outer: while v + height < v_hi {
                        for du in 0..width {
                            if mask[(u + du) * v_hi + (v + height)] != Some(key) {
                                break 'outer;
                            }
                        }
                        height += 1;
                    }

                    // Emitimos el rectangulo. `v` local se convierte a coordenada
                    // real del plano. Los liquidos van a su propio buffer
                    // (translucido).
                    let (target_v, target_i) = if Block::from_u8(key.block).is_liquid() {
                        (&mut water_vertices, &mut water_indices)
                    } else {
                        (&mut vertices, &mut indices)
                    };
                    emit_quad(
                        target_v,
                        target_i,
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
                            mask[(u + du) * v_hi + (v + dv)] = None;
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
                super::mesher::emit_torch(
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

    (vertices, indices, water_vertices, water_indices)
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
    let (ox, oy, oz) = face.offset();
    // El vecino puede estar fuera de la columna (otro chunk): la query decide.
    let (nx, ny, nz) = (x as i32 + ox, y as i32 + oy, z as i32 + oz);
    let neighbor = query(nx, ny, nz);
    if block.is_liquid() {
        // Los liquidos (agua, lava) son visibles pero no solidos: solo asoma
        // su cara contra **aire** (no contra liquido ni contra un solido, que
        // la ocluye).
        if neighbor.is_liquid() || neighbor.is_solid() {
            return None;
        }
    } else if block == Block::Leaves {
        // Las hojas son visibles no solidas (cutout): se dibujan contra aire,
        // pero no entre ellas (rendimiento) ni contra un solido.
        if neighbor == Block::Leaves || neighbor.is_solid() {
            return None;
        }
    } else if !block.is_solid() || neighbor.is_solid() {
        // La antorcha (visible no solida) y el aire no entran en el greedy de
        // cubos; solo se oculta una cara si el vecino es SOLIDO.
        return None;
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

    // Convenio de V (igual que `emit_torch`): v=1 es ABAJO en la imagen del
    // tile y v=0 ARRIBA. Las dos primeras esquinas de cada cara lateral estan
    // a `v0` (abajo del bloque), asi que llevan `h`; de otro modo el tile se
    // dibuja del reves (la hierba lateral salia abajo).
    //
    // Devolvemos posiciones [x, y, z] para las 4 esquinas y las UV.
    let (corners, uvs): ([[f32; 3]; 4], [[f32; 2]; 4]) = match face {
        Face::PosX => {
            // Plano en x = c+1. u -> z, v -> y.
            let x = cf + 1.0;
            (
                [[x, v0, u1], [x, v0, u0], [x, v1, u0], [x, v1, u1]],
                [[0.0, h], [w, h], [w, 0.0], [0.0, 0.0]],
            )
        }
        Face::NegX => {
            let x = cf;
            (
                [[x, v0, u0], [x, v0, u1], [x, v1, u1], [x, v1, u0]],
                [[0.0, h], [w, h], [w, 0.0], [0.0, 0.0]],
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
                [[0.0, h], [w, h], [w, 0.0], [0.0, 0.0]],
            )
        }
        Face::NegZ => {
            let z = cf;
            (
                [[u1, v0, z], [u0, v0, z], [u0, v1, z], [u1, v1, z]],
                [[0.0, h], [w, h], [w, 0.0], [0.0, 0.0]],
            )
        }
    };

    // Las caras horizontales estaban con el ciclo al reves (su normal apuntaba
    // hacia dentro), lo que impedia activar el *back-face culling*. Invertimos el
    // ciclo (y las UVs con el) para que la normal mire hacia fuera como el resto.
    let (corners, uvs) = if matches!(face, Face::PosY | Face::NegY) {
        (
            [corners[0], corners[3], corners[2], corners[1]],
            [uvs[0], uvs[3], uvs[2], uvs[1]],
        )
    } else {
        (corners, uvs)
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
    fn las_caras_laterales_no_salen_del_reves() {
        // Convenio: v=1 es ABAJO en la imagen del tile (igual que la antorcha).
        // Las esquinas de abajo del bloque deben llevar v=1 y las de arriba
        // v=0; de otro modo la franja de hierba lateral sale abajo.
        for face in [Face::PosX, Face::NegX, Face::PosZ, Face::NegZ] {
            let mut v = Vec::new();
            let mut i = Vec::new();
            let key = FaceKey {
                block: Block::Grass.id(),
                face,
                sky: 15,
                block_light: 0,
            };
            emit_quad(&mut v, &mut i, face, 3, 5, 7, 1, 1, key, [0.0; 3]);
            assert_eq!(v.len(), 4, "{face:?}");
            assert!(
                v[0].position[1] < v[2].position[1],
                "{face:?}: esquinas 0/1 abajo"
            );
            assert_eq!(v[0].uv[1], 1.0, "{face:?} abajo");
            assert_eq!(v[1].uv[1], 1.0, "{face:?} abajo");
            assert_eq!(v[2].uv[1], 0.0, "{face:?} arriba");
            assert_eq!(v[3].uv[1], 0.0, "{face:?} arriba");
        }
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
        // Dos planos x 4 vertices; cada plano con las dos orientaciones (4 tri).
        // Dos planos cruzados; cada uno con las dos orientaciones.
        assert_eq!(vertices.len(), 8, "dos planos cruzados");
        assert_eq!(indices.len(), 24);
        assert!(vertices.iter().all(|v| v.tile == 8));
    }

    #[test]
    fn la_antorcha_no_oculta_las_caras_vecinas() {
        // Piedra con antorcha en su cara +X: la cara de la piedra se conserva.
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Stone);
        column.set(9, 8, 8, Block::Torch);
        let (vertices, _) = greedy_column(&column, [0.0; 3]);
        assert_eq!(
            vertices.len(),
            6 * 4 + 8,
            "6 caras de piedra + cruz de la antorcha"
        );
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

    #[test]
    fn todas_las_caras_miran_hacia_fuera() {
        // Con `cull_mode: Back` activo, una cara con la normal invertida
        // desaparece. Comprobamos la normal geometrica del primer triangulo de
        // cada cara contra su direccion de salida.
        use crate::math::Vec3;
        let outward = |f: Face| match f {
            Face::PosX => Vec3::new(1.0, 0.0, 0.0),
            Face::NegX => Vec3::new(-1.0, 0.0, 0.0),
            Face::PosY => Vec3::new(0.0, 1.0, 0.0),
            Face::NegY => Vec3::new(0.0, -1.0, 0.0),
            Face::PosZ => Vec3::new(0.0, 0.0, 1.0),
            Face::NegZ => Vec3::new(0.0, 0.0, -1.0),
        };
        for face in Face::ALL {
            let mut verts = Vec::new();
            let mut idx = Vec::new();
            let key = FaceKey {
                block: Block::Stone.id(),
                face,
                sky: 15,
                block_light: 0,
            };
            emit_quad(&mut verts, &mut idx, face, 0, 0, 0, 1, 1, key, [0.0; 3]);
            let p = |i: usize| {
                Vec3::new(
                    verts[i].position[0],
                    verts[i].position[1],
                    verts[i].position[2],
                )
            };
            let normal = (p(1) - p(0)).cross(p(2) - p(0)).normalize();
            assert!(
                normal.dot(outward(face)) > 0.9,
                "{face:?}: normal {normal:?} no mira hacia {:?}",
                outward(face)
            );
        }
    }
}
