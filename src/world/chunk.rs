//! Un chunk: un trozo cubico de mundo de 16x16x16 bloques.
//!
//! Layout de memoria: guardamos los bloques en un array plano de 4096 `u8` (en
//! realidad `Block`, que es un `u8`). El indice se calcula como
//! `(y * 16 + z) * 16 + x`, es decir, **x es lo mas rapido** y **y lo mas lento**.
//! Mas adelante (v0.11.0) esto pasara a un paleta + bit-packing para ahorrar
//! memoria, pero el acceso seguira siendo por `(x, y, z)`.

use super::block::Block;

/// Lado del chunk, en bloques.
pub const CHUNK_SIZE: usize = 16;

/// Numero total de bloques de un chunk (16^3 = 4096).
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Un chunk lleno de bloques.
pub struct Chunk {
    blocks: [Block; CHUNK_VOLUME],
}

impl Chunk {
    /// Un chunk vacio (todo aire).
    pub fn empty() -> Self {
        Self {
            blocks: [Block::Air; CHUNK_VOLUME],
        }
    }

    /// Indice plano del bloque `(x, y, z)`. `x` varia mas rapido.
    #[inline]
    pub fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }

    /// Lee el bloque en una posicion de dentro del chunk.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.blocks[Self::index(x, y, z)]
    }

    /// Escribe el bloque en una posicion de dentro del chunk.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.blocks[Self::index(x, y, z)] = block;
    }

    /// Lee un bloque en coordenadas que pueden salirse del chunk. Fuera del
    /// chunk devolvemos [`Block::Air`]: asi el mesher dibuja la cara exterior
    /// del chunk sin tener que saber si hay otro chunk al lado.
    #[inline]
    pub fn get_or_air(&self, x: i32, y: i32, z: i32) -> Block {
        if x < 0 || y < 0 || z < 0 {
            return Block::Air;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || y >= CHUNK_SIZE || z >= CHUNK_SIZE {
            return Block::Air;
        }
        self.blocks[Self::index(x, y, z)]
    }

    /// Escribe un bloque ignorando (sin fallar) si la posicion esta fuera.
    pub fn set_if_inside(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if x >= 0
            && y >= 0
            && z >= 0
            && (x as usize) < CHUNK_SIZE
            && (y as usize) < CHUNK_SIZE
            && (z as usize) < CHUNK_SIZE
        {
            self.set(x as usize, y as usize, z as usize, block);
        }
    }

    /// Genera un terreno de ejemplo (una colina suave + un arbolito).
    ///
    /// Es un *placeholder* deterministico hasta v0.3.0, donde llegara el ruido
    /// Perlin de verdad. Solo existe para tener algo interesante que mirar.
    pub fn generate_demo() -> Self {
        let mut chunk = Self::empty();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = demo_height(x, z);
                for y in 0..height {
                    let block = if y + 1 == height {
                        Block::Grass
                    } else if y + 4 >= height {
                        Block::Dirt
                    } else {
                        Block::Stone
                    };
                    chunk.set(x, y, z, block);
                }
            }
        }

        // Un arbol de ejemplo para luciar las texturas de madera y hojas.
        let tx = 11usize;
        let tz = 4usize;
        let ground = demo_height(tx, tz);
        // Tronco (3 bloques).
        for i in 0..3 {
            chunk.set_if_inside(tx as i32, (ground + i) as i32, tz as i32, Block::Wood);
        }
        // Copa de hojas (una bola achatada).
        let top = (ground + 3) as i32;
        for dy in -1..=1i32 {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    // Recortamos las esquinas para que no sea un cubo perfecto.
                    if dx.abs() == 2 && dz.abs() == 2 && dy == 1 {
                        continue;
                    }
                    let (lx, ly, lz) = (tx as i32 + dx, top + dy, tz as i32 + dz);
                    if (lx as usize) < CHUNK_SIZE
                        && (lz as usize) < CHUNK_SIZE
                        && ly >= 0
                        && (ly as usize) < CHUNK_SIZE
                    {
                        // No pisamos bloques que no sean aire (p.ej. el tronco).
                        if chunk.get(lx as usize, ly as usize, lz as usize) == Block::Air {
                            chunk.set(lx as usize, ly as usize, lz as usize, Block::Leaves);
                        }
                    }
                }
            }
        }

        chunk
    }
}

/// Altura del terreno de ejemplo en la columna `(x, z)`, en `1..=15`.
fn demo_height(x: usize, z: usize) -> usize {
    let fx = x as f32;
    let fz = z as f32;
    let h = 8.0 + 3.0 * (fx * 0.55).sin() + 2.5 * (fz * 0.50).cos();
    h.round().clamp(1.0, 15.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_es_unico_por_celda() {
        // Esquinas y centro no deben colisionar.
        let a = Chunk::index(0, 0, 0);
        let b = Chunk::index(15, 15, 15);
        let c = Chunk::index(1, 0, 0);
        assert_eq!(a, 0);
        assert_eq!(b, CHUNK_VOLUME - 1);
        assert_ne!(a, c);
        assert_eq!(c, 1);
    }

    #[test]
    fn get_or_air_fuera_del_chunk() {
        let chunk = Chunk::empty();
        assert_eq!(chunk.get_or_air(-1, 0, 0), Block::Air);
        assert_eq!(chunk.get_or_air(16, 0, 0), Block::Air);
        assert_eq!(chunk.get_or_air(0, 0, 99), Block::Air);
    }

    #[test]
    fn terreno_demo_tiene_suelo_y_cielo_libre() {
        let chunk = Chunk::generate_demo();
        // La capa de abajo no es aire (hay mundo).
        assert!(chunk.get(0, 0, 0).is_solid());
        // La de arriba del todo esta vacia (el terreno no llega a 16).
        assert_eq!(chunk.get(0, 15, 0), Block::Air);
    }
}
