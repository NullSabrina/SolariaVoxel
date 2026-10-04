//! El mesher: convierte una [`Column`] en geometria (vertices + indices).
//!
//! Clave de v0.2.1: generamos la malla **por secciones** y devolvemos solo las
//! que tienen geometria. Como la mayoria de las 24 secciones de una columna
//! estan vacias, el renderer se ahorra dibujarlas.
//!
//! El vecindario se consulta de forma **global** (a traves de [`Column`]), asi
//! que las caras entre dos secciones apiladas se descartan correctamente. Solo
//! generamos las caras que dan al aire (*face culling*).

use super::atlas::tile_uv_rect;
use super::block::Face;
use super::chunk::{CHUNK_SIZE, Column, SECTION_COUNT, WORLD_HEIGHT};
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
                if !block.is_solid() {
                    continue;
                }

                let section = &mut per_section[y / CHUNK_SIZE];
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

    let [u0, v0, u1, v1] = tile_uv_rect(tile);
    let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];

    let base = vertices.len() as u32;
    for (corner, uv) in corners.iter().zip(uvs.iter()) {
        vertices.push(Vertex::new(*corner, *uv));
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
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
}
