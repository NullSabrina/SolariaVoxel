//! El mesher: convierte una [`Column`] en geometria (vertices + indices).
//!
//! Clave de v0.2.1: generamos la malla **por secciones** y devolvemos solo las
//! que tienen geometria. Como la mayoria de las 24 secciones de una columna
//! estan vacias, el renderer se ahorra dibujarlas.
//!
//! El vecindario se consulta de forma **global** (a traves de [`Column`]), asi
//! que las caras entre dos secciones apiladas se descartan correctamente. Solo
//! generamos las caras que dan al aire (*face culling*).

use super::block::{Block, Face};
use super::chunk::{CHUNK_SIZE, Column, MAX_LIGHT, SECTION_COUNT, WORLD_HEIGHT};
use crate::render::mesh::Vertex;

/// La malla de una seccion concreta de la columna.
pub struct SectionMesh {
    /// Indice de la seccion (0 = la mas baja).
    pub section: usize,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Genera la malla de una columna y devuelve **solo las secciones no vacias**.
///
/// `origin` se suma a todas las posiciones (se usa para centrar la columna en
/// el mundo sin necesidad de una matriz de modelo por seccion).
pub fn mesh_column(column: &Column, origin: [f32; 3]) -> Vec<SectionMesh> {
    // Acumulamos la geometria de cada seccion por separado.
    let mut per_section: Vec<(Vec<Vertex>, Vec<u32>)> = (0..SECTION_COUNT)
        .map(|_| (Vec::new(), Vec::new()))
        .collect();

    for y in 0..WORLD_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = column.get(x, y, z);
                let section = &mut per_section[y / CHUNK_SIZE];

                // La antorcha no es un cubo: se dibuja como dos quads cruzados.
                if block == Block::Torch {
                    let (sky, block_light) = light_pair(column, x, y, z);
                    emit_torch_cross(
                        &mut section.0,
                        &mut section.1,
                        origin,
                        x,
                        y,
                        z,
                        block,
                        sky,
                        block_light,
                    );
                    continue;
                }
                if !block.is_solid() {
                    continue;
                }

                let (xi, yi, zi) = (x as i32, y as i32, z as i32);

                for face in Face::ALL {
                    let (ox, oy, oz) = face.offset();
                    // Vecino global: cruza secciones sin problema.
                    if column.get_or_air(xi + ox, yi + oy, zi + oz).is_solid() {
                        continue;
                    }
                    let tile = block.face_tile(face);
                    add_face(&mut section.0, &mut section.1, origin, x, y, z, face, tile);
                }
            }
        }
    }

    // Nos quedamos con las secciones que de verdad tienen geometria.
    per_section
        .into_iter()
        .enumerate()
        .filter(|(_, (vertices, _))| !vertices.is_empty())
        .map(|(section, (vertices, indices))| SectionMesh {
            section,
            vertices,
            indices,
        })
        .collect()
}

/// Genera la malla de **una sola seccion**. Es lo que usaremos para regenerar
/// rapido el trozo afectado al romper o colocar un bloque (v0.4.0), sin rehacer
/// las 24 secciones de la columna.
pub fn mesh_section(column: &Column, section: usize, origin: [f32; 3]) -> SectionMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let y_start = section * CHUNK_SIZE;
    let y_end = (y_start + CHUNK_SIZE).min(WORLD_HEIGHT);

    for y in y_start..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = column.get(x, y, z);

                // La antorcha no es un cubo: se dibuja como dos quads cruzados.
                if block == Block::Torch {
                    let (sky, block_light) = light_pair(column, x, y, z);
                    emit_torch_cross(
                        &mut vertices,
                        &mut indices,
                        origin,
                        x,
                        y,
                        z,
                        block,
                        sky,
                        block_light,
                    );
                    continue;
                }
                if !block.is_solid() {
                    continue;
                }
                let (xi, yi, zi) = (x as i32, y as i32, z as i32);
                for face in Face::ALL {
                    let (ox, oy, oz) = face.offset();
                    if column.get_or_air(xi + ox, yi + oy, zi + oz).is_solid() {
                        continue;
                    }
                    let tile = block.face_tile(face);
                    add_face(&mut vertices, &mut indices, origin, x, y, z, face, tile);
                }
            }
        }
    }

    SectionMesh {
        section,
        vertices,
        indices,
    }
}

/// Emite una cara (4 vertices + 2 triangulos) de un voxel, ya desplazada por
/// `origin`.
#[allow(clippy::too_many_arguments)]
fn add_face(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    origin: [f32; 3],
    x: usize,
    y: usize,
    z: usize,
    face: Face,
    tile: u16,
) {
    // El voxel ocupa la caja [x, x+1] x [y, y+1] x [z, z+1], desplazada.
    let (x0, y0, z0) = (
        origin[0] + x as f32,
        origin[1] + y as f32,
        origin[2] + z as f32,
    );
    let (x1, y1, z1) = (x0 + 1.0, y0 + 1.0, z0 + 1.0);

    let corners = match face {
        Face::PosX => [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]],
        Face::NegX => [[x0, y0, z1], [x0, y0, z0], [x0, y1, z0], [x0, y1, z1]],
        Face::PosY => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Face::NegY => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Face::PosZ => [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        Face::NegZ => [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
    };

    // Una cara de 1x1 usa el tile entero (0..1); el array de texturas lo aisla.
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let base = vertices.len() as u32;
    for (corner, uv) in corners.iter().zip(uvs.iter()) {
        vertices.push(Vertex::new(*corner, *uv, tile as u32));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Par de luces `(cielo, bloque)` de una celda, normalizadas a 0..1.
fn light_pair(column: &Column, x: usize, y: usize, z: usize) -> (f32, f32) {
    (
        column.light_at(x, y, z) as f32 / MAX_LIGHT as f32,
        column.block_light_at(x, y, z) as f32 / MAX_LIGHT as f32,
    )
}

/// Emite la geometria de una antorcha: **dos quads verticales cruzados**, uno en
/// el plano `X = centro` y otro en el plano `Z = centro` del voxel (la "cruz"
/// que se ve desde arriba). Ambos usan el tile entero de la antorcha; el shader
/// descarta el alfa bajo (cutout), asi que solo se ve la llama y el palo, no el
/// fondo transparente.
///
/// Emite una caja con las 6 caras (ambas orientaciones, para que el culling no
/// oculte ninguna) mapeando `uv` en cada cara. Se usa para el palo del modelo.
#[allow(clippy::too_many_arguments)]
fn emit_box(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    min: [f32; 3],
    max: [f32; 3],
    uv: [f32; 4],
    sky: f32,
    block_light: f32,
    tile: u32,
) {
    let [u0, v0, u1, v1] = uv;
    let faces: [[[f32; 3]; 4]; 6] = [
        [
            [min[0], min[1], min[2]],
            [min[0], min[1], max[2]],
            [min[0], max[1], max[2]],
            [min[0], max[1], min[2]],
        ],
        [
            [max[0], min[1], min[2]],
            [max[0], max[1], min[2]],
            [max[0], max[1], max[2]],
            [max[0], min[1], max[2]],
        ],
        [
            [min[0], min[1], min[2]],
            [max[0], min[1], min[2]],
            [max[0], min[1], max[2]],
            [min[0], min[1], max[2]],
        ],
        [
            [min[0], max[1], min[2]],
            [min[0], max[1], max[2]],
            [max[0], max[1], max[2]],
            [max[0], max[1], min[2]],
        ],
        [
            [min[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
            [max[0], min[1], min[2]],
        ],
        [
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [max[0], max[1], max[2]],
            [min[0], max[1], max[2]],
        ],
    ];
    let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
    for face in faces {
        let base = vertices.len() as u32;
        for (corner, uv) in face.iter().zip(uvs.iter()) {
            vertices.push(Vertex::with_light(*corner, *uv, sky, block_light, tile));
        }
        indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base,
            base + 2,
            base + 3,
            base,
            base + 2,
            base + 1,
            base,
            base + 3,
            base + 2,
        ]);
    }
}

/// El pipeline dibuja con *back-face culling*, asi que cada plano se emite con
/// las **dos orientaciones** (ambos windings comparten los 4 vertices y solo
/// cambian los indices): la cruz se ve por delante y por detras.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_torch_cross(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    origin: [f32; 3],
    x: usize,
    y: usize,
    z: usize,
    block: Block,
    sky: f32,
    block_light: f32,
) {
    let (x0, y0, z0) = (
        origin[0] + x as f32,
        origin[1] + y as f32,
        origin[2] + z as f32,
    );
    let (x1, y1, z1) = (x0 + 1.0, y0 + 1.0, z0 + 1.0);
    let (cx, cz) = (x0 + 0.5, z0 + 0.5);
    let tile = block.face_tile(Face::PosY) as u32;

    // UVs con `v = 0` arriba y `v = 1` abajo: el tile tiene la llama en la parte
    // alta y el palo debajo, como en el modelo de Blockbench.
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    // Plano X = centro: recorre Z (u) e Y (v).
    let plane_x = [[cx, y0, z1], [cx, y0, z0], [cx, y1, z0], [cx, y1, z1]];
    // Plano Z = centro: recorre X (u) e Y (v).
    let plane_z = [[x0, y0, cz], [x1, y0, cz], [x1, y1, cz], [x0, y1, cz]];

    for corners in [plane_x, plane_z] {
        let base = vertices.len() as u32;
        for (corner, uv) in corners.iter().zip(uvs.iter()) {
            vertices.push(Vertex::with_light(*corner, *uv, sky, block_light, tile));
        }
        // Las dos orientaciones (mismos vertices, indices invertidos).
        indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base,
            base + 2,
            base + 3,
            base,
            base + 2,
            base + 1,
            base,
            base + 3,
            base + 2,
        ]);
    }

    // Palo central del modelo (`assets/models/solaria_torch.bbmodel`: cubo
    // 7..9 x 0..10 x 7..9). Se infla un pelin para que no sea coplanar con las
    // tablas cruzadas (evita z-fighting) y se mapea solo la franja del palo del
    // tile, para no repetir la llama.
    let (sx0, sx1) = (x0 + 7.0 / 16.0 - 0.02, x0 + 9.0 / 16.0 + 0.02);
    let (sz0, sz1) = (z0 + 7.0 / 16.0 - 0.02, z0 + 9.0 / 16.0 + 0.02);
    emit_box(
        vertices,
        indices,
        [sx0, y0, sz0],
        [sx1, y0 + 10.0 / 16.0, sz1],
        [7.0 / 16.0, 9.0 / 16.0, 9.0 / 16.0, 1.0],
        sky,
        block_light,
        tile,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    #[test]
    fn un_solo_bloque_genera_una_seccion_de_24_vertices() {
        let mut column = Column::empty();
        column.set(8, 70, 8, Block::Stone);
        let sections = mesh_column(&column, [0.0, 0.0, 0.0]);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].section, 70 / 16);
        assert_eq!(sections[0].vertices.len(), 24);
        assert_eq!(sections[0].indices.len(), 36);
    }

    #[test]
    fn caras_entre_secciones_apiladas_se_descartan() {
        let mut column = Column::empty();
        column.set(8, 15, 8, Block::Stone); // ultima capa de la seccion 0
        column.set(8, 16, 8, Block::Stone); // primera capa de la seccion 1
        let sections = mesh_column(&column, [0.0, 0.0, 0.0]);
        // Dos secciones con geometria.
        assert_eq!(sections.len(), 2);
        // La cara compartida se descarta: 12 - 2 = 10 caras = 40 vertices.
        let total: usize = sections.iter().map(|s| s.vertices.len()).sum();
        assert_eq!(total, 40);
    }

    #[test]
    fn las_secciones_vacias_no_aparecen() {
        let mut column = Column::empty();
        column.set(0, 0, 0, Block::Stone);
        column.set(0, 300, 0, Block::Stone);
        let sections = mesh_column(&column, [0.0, 0.0, 0.0]);
        assert_eq!(sections.len(), 2); // solo secciones 0 y 18
        assert_eq!(sections[0].section, 0);
        assert_eq!(sections[1].section, 300 / 16);
    }

    #[test]
    fn los_indices_apuntan_a_vertices_validos() {
        let column = Column::generate_demo();
        let sections = mesh_column(&column, [-8.0, 0.0, -8.0]);
        for s in &sections {
            assert!(s.indices.iter().all(|&i| (i as usize) < s.vertices.len()));
            assert_eq!(s.indices.len() % 3, 0);
        }
    }

    #[test]
    fn la_antorcha_emite_la_cruz_mas_el_palo_del_modelo() {
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Torch);
        let sections = mesh_column(&column, [0.0, 0.0, 0.0]);
        assert_eq!(sections.len(), 1);
        // 2 quads cruzados (8 verts) + el palo del .bbmodel: 6 caras x 4 verts.
        assert_eq!(sections[0].vertices.len(), 8 + 24);
        assert_eq!(sections[0].indices.len(), 24 + 72);
        // Todas las caras usan el tile de la antorcha.
        assert!(sections[0].vertices.iter().all(|v| v.tile == 8));
        assert!(
            sections[0]
                .indices
                .iter()
                .all(|&i| (i as usize) < sections[0].vertices.len())
        );
    }

    #[test]
    fn la_antorcha_no_tapa_las_caras_vecinas() {
        // Piedra con una antorcha pegada a su cara +X: la cara de la piedra que
        // da a la antorcha sigue dibujandose (la antorcha no ocluye).
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Stone);
        column.set(9, 8, 8, Block::Torch);
        let sections = mesh_column(&column, [0.0, 0.0, 0.0]);
        let total: usize = sections.iter().map(|s| s.vertices.len()).sum();
        // 6 caras de la piedra (24) + cruz y palo de la antorcha (32).
        assert_eq!(total, 24 + 32);
    }
}
