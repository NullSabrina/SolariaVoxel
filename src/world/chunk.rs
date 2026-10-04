//! El mundo se organiza en **columnas** verticales.
//!
//! * Un [`Chunk`] es una seccion cubica de 16x16x16 bloques (la unidad de
//!   meshing y, en el futuro, la unidad de carga/descarga).
//! * Una [`Column`] apila `SECTION_COUNT` secciones hasta `WORLD_HEIGHT` (384)
//!   bloques de alto, como en Minecraft.
//!
//! La clave de rendimiento de v0.2.1: la mayoria de las 24 secciones de una
//! columna estan **vacias** (todo aire), asi que no generamos ni dibujamos su
//! geometria.

use std::array;

use super::block::Block;

/// Lado de una seccion, en bloques.
pub const CHUNK_SIZE: usize = 16;

/// Numero de bloques de una seccion (16^3 = 4096).
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Altura total del mundo, en bloques (16 x 24).
pub const WORLD_HEIGHT: usize = 384;

/// Cuantas secciones tiene una columna (384 / 16 = 24).
pub const SECTION_COUNT: usize = WORLD_HEIGHT / CHUNK_SIZE;

/// Una seccion de 16x16x16 bloques.
pub struct Chunk {
    blocks: [Block; CHUNK_VOLUME],
}

impl Chunk {
    /// Una seccion vacia (todo aire).
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

    /// Lee un bloque dentro de la seccion.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.blocks[Self::index(x, y, z)]
    }

    /// Escribe un bloque dentro de la seccion.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.blocks[Self::index(x, y, z)] = block;
    }

    /// ¿Hay algun bloque solido en esta seccion? Sirve para saltarnos secciones
    /// vacias al generar la malla.
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|b| !b.is_solid())
    }
}

/// Niveles de luz, de 0 (oscuridad) a 15 (cielo despejado).
pub const MAX_LIGHT: u8 = 15;

/// Una columna del mundo: `SECTION_COUNT` secciones apiladas.
pub struct Column {
    sections: [Chunk; SECTION_COUNT],
    /// Luz de cielo por bloque (0..15), en el mismo orden que los bloques.
    /// Guardamos `u8` por celda: 16x16x384 = ~98 KB por columna. En v0.12.x
    /// pasaremos a un buffer de 4 bits por celda (mitad de memoria).
    light: Vec<u8>,
}

impl Column {
    /// Una columna vacia.
    pub fn empty() -> Self {
        Self {
            sections: array::from_fn(|_| Chunk::empty()),
            light: vec![0; CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT],
        }
    }

    /// Indice plano de la luz.
    #[inline]
    fn light_index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }

    /// Luz de cielo en una posicion (0..15).
    #[inline]
    pub fn light_at(&self, x: usize, y: usize, z: usize) -> u8 {
        self.light[Self::light_index(x, y, z)]
    }

    /// Ajusta la luz de cielo en una posicion.
    #[inline]
    pub fn set_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        self.light[Self::light_index(x, y, z)] = level.min(MAX_LIGHT);
    }

    /// Calcula la **luz de cielo** de toda la columna.
    ///
    /// En v0.6.0 es una version simplificada (sin propagacion lateral aun):
    /// * Un bloque no solido **a cielo abierto** (por encima de la primera cosa
    ///   solida de su columna vertical) recibe luz 15.
    /// * Todo lo que queda bajo la superficie recibe 0.
    ///
    /// Esto ya da el efecto buscado (superficie iluminada, subsuelo a oscuras).
    /// La propagacion suave de la luz por las caras (flood fill 3D) y la luz de
    /// bloque (antorchas) llegan en v0.6.1/v0.6.2. Por eso el metodo esta
    /// separado: mas adelante lo sustituimos sin tocar el resto.
    pub fn compute_skylight(&mut self) {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                // `open` sigue siendo true mientras no encontremos solido.
                let mut open = true;
                let mut y = WORLD_HEIGHT;
                while y > 0 {
                    y -= 1;
                    let block = self.get(x, y, z);
                    if block.is_solid() {
                        open = false;
                    }
                    let level = if open { MAX_LIGHT } else { 0 };
                    self.set_light(x, y, z, level);
                }
            }
        }
    }

    /// Luz de cielo en coordenadas que pueden salirse de la columna. Fuera
    /// devolvemos 0 (oscuridad).
    #[inline]
    pub fn light_or_zero(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 {
            return 0;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return 0;
        }
        self.light_at(x, y, z)
    }

    /// Lee un bloque con `y` global (0..WORLD_HEIGHT).
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.sections[y / CHUNK_SIZE].get(x, y % CHUNK_SIZE, z)
    }

    /// Escribe un bloque con `y` global.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.sections[y / CHUNK_SIZE].set(x, y % CHUNK_SIZE, z, block);
    }

    /// Lee un bloque en coordenadas que pueden salirse de la columna. Fuera
    /// (incluida la cara superior del mundo) devolvemos aire.
    #[inline]
    pub fn get_or_air(&self, x: i32, y: i32, z: i32) -> Block {
        if x < 0 || y < 0 || z < 0 {
            return Block::Air;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return Block::Air;
        }
        self.get(x, y, z)
    }

    /// Escribe un bloque ignorando si la posicion esta fuera.
    pub fn set_if_inside(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if x >= 0
            && y >= 0
            && z >= 0
            && (x as usize) < CHUNK_SIZE
            && (z as usize) < CHUNK_SIZE
            && (y as usize) < WORLD_HEIGHT
        {
            self.set(x as usize, y as usize, z as usize, block);
        }
    }

    /// Genera un terreno de ejemplo: colina a ~y=64 y un arbol.
    ///
    /// *Placeholder* deterministico. Los tests de meshing lo usan.
    pub fn generate_demo() -> Self {
        let mut column = Self::empty();

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
                    column.set(x, y, z, block);
                }
            }
        }

        // Arbol de ejemplo.
        let (tx, tz) = (11usize, 4usize);
        let ground = demo_height(tx, tz);
        for i in 0..4 {
            column.set_if_inside(tx as i32, (ground + i) as i32, tz as i32, Block::Wood);
        }
        let top = (ground + 4) as i32;
        for dy in -2..=1i32 {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    if dx.abs() == 2 && dz.abs() == 2 {
                        continue;
                    }
                    let (lx, ly, lz) = (tx as i32 + dx, top + dy, tz as i32 + dz);
                    if column.get_or_air(lx, ly, lz) == Block::Air {
                        column.set_if_inside(lx, ly, lz, Block::Leaves);
                    }
                }
            }
        }

        column
    }
}

/// Altura del terreno de ejemplo en la columna `(x, z)`.
fn demo_height(x: usize, z: usize) -> usize {
    let fx = x as f32;
    let fz = z as f32;
    let h = 64.0 + 4.0 * (fx * 0.55).sin() + 3.0 * (fz * 0.50).cos();
    h.round().clamp(40.0, 80.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_es_unico_por_celda() {
        assert_eq!(Chunk::index(0, 0, 0), 0);
        assert_eq!(Chunk::index(15, 15, 15), CHUNK_VOLUME - 1);
        assert_eq!(Chunk::index(1, 0, 0), 1);
    }

    #[test]
    fn la_seccion_vacia_lo_esta() {
        assert!(Chunk::empty().is_empty());
        let mut chunk = Chunk::empty();
        chunk.set(3, 3, 3, Block::Stone);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn get_or_air_fuera_de_la_columna() {
        let column = Column::empty();
        assert_eq!(column.get_or_air(-1, 0, 0), Block::Air);
        assert_eq!(column.get_or_air(16, 0, 0), Block::Air);
        assert_eq!(column.get_or_air(0, WORLD_HEIGHT as i32, 0), Block::Air);
    }

    #[test]
    fn escribir_y_leer_en_secciones_distintas() {
        let mut column = Column::empty();
        // y=5 esta en la seccion 0; y=20, en la 1.
        column.set(1, 5, 2, Block::Stone);
        column.set(1, 20, 2, Block::Sand);
        assert_eq!(column.get(1, 5, 2), Block::Stone);
        assert_eq!(column.get(1, 20, 2), Block::Sand);
        assert!(!column.sections[0].is_empty());
        assert!(!column.sections[1].is_empty());
        assert!(column.sections[2].is_empty());
    }

    #[test]
    fn la_skylight_ilumina_la_superficie_y_oscurece_el_subsuelo() {
        let mut column = Column::empty();
        // Suelo en y=0..4; aire por encima.
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                for y in 0..4 {
                    column.set(x, y, z, Block::Stone);
                }
            }
        }
        column.compute_skylight();
        // El aire sobre el suelo esta a cielo abierto -> luz 15.
        assert_eq!(column.light_at(0, 4, 0), MAX_LIGHT);
        assert_eq!(column.light_at(0, WORLD_HEIGHT - 1, 0), MAX_LIGHT);
        // Bajo el suelo no llega el cielo -> 0.
        assert_eq!(column.light_at(0, 3, 0), 0);
        // El propio bloque solido tambien queda a 0.
        assert_eq!(column.light_at(0, 0, 0), 0);
    }

    #[test]
    fn el_terreno_demo_llena_el_subsuelo() {
        let column = Column::generate_demo();
        assert!(column.get(0, 0, 0).is_solid());
        assert_eq!(column.get(0, WORLD_HEIGHT - 1, 0), Block::Air);
    }
}
