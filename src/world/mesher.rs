//! El mesher: convierte un [`Chunk`] en geometria (vertices + indices).
//!
//! Version "naive" (v0.2.0): recorremos los 4096 bloques y, por cada bloque
//! solido, emitimos SOLO las caras que dan al aire. A esto se le llama
//! *face culling*: una cara entre dos bloques solidos no se ve, asi que no se
//! genera. El resultado de un chunk de terreno son unos pocos miles de
//! triangulos en vez de 4096 cubos completos.
//!
//! En v0.4.1 sustituiremos esto por *greedy meshing*, que fusiona caras
//! contiguas del mismo material en cuadrilateros grandes.

use super::atlas::tile_uv_rect;
use super::block::Face;
use super::chunk::{CHUNK_SIZE, Chunk};
use crate::render::mesh::Vertex;

/// Genera la malla del chunk: `(vertices, indices)` listos para subir a la GPU.
pub fn mesh_chunk(chunk: &Chunk) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z);
                if !block.is_solid() {
                    continue;
                }

                let (xi, yi, zi) = (x as i32, y as i32, z as i32);

                for face in Face::ALL {
                    let (ox, oy, oz) = face.offset();
                    // Si el vecino de esa cara es solido, la cara no se ve.
                    if chunk.get_or_air(xi + ox, yi + oy, zi + oz).is_solid() {
                        continue;
                    }
                    let tile = block.face_tile(face);
                    add_face(&mut vertices, &mut indices, x, y, z, face, tile);
                }
            }
        }
    }

    (vertices, indices)
}

/// Emite una cara (4 vertices + 2 triangulos) de un voxel.
fn add_face(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    x: usize,
    y: usize,
    z: usize,
    face: Face,
    tile: u16,
) {
    // El voxel ocupa la caja [x, x+1] x [y, y+1] x [z, z+1].
    let (x0, y0, z0) = (x as f32, y as f32, z as f32);
    let (x1, y1, z1) = (x0 + 1.0, y0 + 1.0, z0 + 1.0);

    // Las 4 esquinas de la cara, en orden (dos triangulos: 0-1-2 y 0-2-3).
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
    fn un_solo_bloque_genera_24_vertices_y_36_indices() {
        let mut chunk = Chunk::empty();
        chunk.set(8, 8, 8, Block::Stone);
        let (vertices, indices) = mesh_chunk(&chunk);
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn dos_bloques_pegados_no_dibujan_la_cara_compartida() {
        let mut chunk = Chunk::empty();
        chunk.set(8, 8, 8, Block::Stone);
        chunk.set(9, 8, 8, Block::Stone);
        let (vertices, _) = mesh_chunk(&chunk);
        // Cada bloque tendria 6 caras (24 verts); la compartida desaparece, asi
        // que son 10 caras = 40 vertices.
        assert_eq!(vertices.len(), 10 * 4);
    }

    #[test]
    fn los_indices_apuntan_a_vertices_validos() {
        let chunk = Chunk::generate_demo();
        let (vertices, indices) = mesh_chunk(&chunk);
        assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));
        assert_eq!(indices.len() % 3, 0);
    }
}
