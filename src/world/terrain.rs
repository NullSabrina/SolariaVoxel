//! Generacion de terreno procedural con ruido Perlin.
//!
//! Este modulo convierte una semilla + la posicion `(x, z)` de una columna en
//! **altura** y **tipo de bloque de superficie**. Es el paso clave para pasar
//! de una columna de ejemplo a un mundo continuo (en v0.3.1, de varios chunks).
//!
//! Usamos dos capas de ruido Perlin (crate `noise`):
//! * una de baja frecuencia para el relieve general (colinas y valles), y
//! * otra de mas frecuencia para el detalle.
//!
//! Mas adelante (v0.7.0) este generador se versionara y anadira biomas y
//! cuevas; por eso ya vive en su propio tipo [`TerrainGenerator`].

use noise::{NoiseFn, Perlin};

use super::block::Block;
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};

/// Altura media del terreno, en bloques.
pub const SEA_LEVEL: i32 = 64;

/// Generador deterministico: la misma semilla produce siempre el mismo mundo.
pub struct TerrainGenerator {
    /// Ruido de baja frecuencia: el relieve grande.
    base: Perlin,
    /// Ruido de detalle: pequenas variaciones.
    detail: Perlin,
    /// Semilla original (la guardaremos en el header del mundo en v0.5.0).
    seed: u32,
}

impl TerrainGenerator {
    /// Crea un generador para una semilla.
    pub fn new(seed: u32) -> Self {
        Self {
            base: Perlin::new(seed),
            // Otra semilla distinta para que los dos ruidos no sean iguales.
            detail: Perlin::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1)),
            seed,
        }
    }

    /// La semilla con la que se creo.
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Altura del terreno (numero de bloques solidos) en `(x, z)` del mundo.
    ///
    /// Sumamos una onda grande y otra pequena; el resultado queda en
    /// `[48, 96]`, es decir, dentro de la seccion 3 (y en `48..64`).
    pub fn height(&self, world_x: i32, world_z: i32) -> usize {
        let x = world_x as f64;
        let z = world_z as f64;
        // Perlin devuelve aprox. [-1, 1].
        let base = self.base.get([x * 0.010, z * 0.010]);
        let detail = self.detail.get([x * 0.045, z * 0.045]);
        let h = SEA_LEVEL as f64 + base * 20.0 + detail * 4.0;
        (h.round() as i32).clamp(48, 96) as usize
    }

    /// Rellena una columna del mundo con terreno segun su posicion `(world_x,
    /// world_z)`.
    pub fn generate_column(&self, world_x: i32, world_z: i32) -> Column {
        let mut column = Column::empty();
        let height = self.height(world_x, world_z);

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                for y in 0..height {
                    let block = surface_block(y, height);
                    column.set(x, y, z, block);
                }
            }
        }

        column
    }
}

/// Elige el bloque de la profundidad `y` para una columna de altura `height`.
fn surface_block(y: usize, height: usize) -> Block {
    if y + 1 == height {
        Block::Grass // la capa de arriba es hierba
    } else if y + 4 >= height {
        Block::Dirt // las 3 capas de debajo, tierra
    } else {
        Block::Stone // el resto, piedra
    }
}

/// Comprueba que la altura nunca se sale del mundo.
pub fn max_height() -> usize {
    // La cota superior del `clamp` en `height` (96) debe caber en el mundo.
    let _ = WORLD_HEIGHT;
    96
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_misma_semilla_da_la_misma_altura() {
        let a = TerrainGenerator::new(1234);
        let b = TerrainGenerator::new(1234);
        assert_eq!(a.height(10, 20), b.height(10, 20));
    }

    #[test]
    fn semillas_distintas_dan_terrenos_distintos() {
        let a = TerrainGenerator::new(1);
        let b = TerrainGenerator::new(2);
        // Con muchas columnas, seguro que difieren en alguna.
        let distintos = (0..32)
            .filter(|&x| a.height(x, 0) != b.height(x, 0))
            .count();
        assert!(distintos > 0);
    }

    #[test]
    fn la_altura_esta_dentro_de_limites() {
        let generator = TerrainGenerator::new(7);
        for x in -50..50 {
            for z in -50..50 {
                let h = generator.height(x, z);
                assert!((48..=96).contains(&h), "altura fuera de rango: {h}");
            }
        }
    }

    #[test]
    fn la_columna_generada_tiene_superficie_y_subsuelo() {
        let generator = TerrainGenerator::new(99);
        let column = generator.generate_column(0, 0);
        let h = generator.height(0, 0);
        assert_eq!(column.get(0, h - 1, 0), Block::Grass);
        assert_eq!(column.get(0, h - 2, 0), Block::Dirt);
        assert_eq!(column.get(0, 0, 0), Block::Stone);
        assert_eq!(column.get(0, h, 0), Block::Air);
    }
}
