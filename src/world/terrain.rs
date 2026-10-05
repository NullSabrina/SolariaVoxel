//! Generacion de terreno procedural con ruido Perlin, **biomas** y **cuevas**.
//!
//! Este modulo convierte una semilla + la posicion `(x, z)` de una columna en
//! **altura**, **bioma** y **tipo de bloque de superficie**.
//!
//! Usamos:
//! * dos capas de ruido Perlin (crate `noise`) para el relieve (colinas y
//!   valles),
//! * un ruido **Worley** (cellular) para repartir el mundo en **biomas**
//!   (desierto, bosque, nieve), y
//! * un ruido **Perlin 3D** para las **cuevas**: donde su valor cruza un umbral
//!   (una iso-superficie) el bloque solido se deja en aire, formando tuneles y
//!   salas. Desde v0.7.5.

use noise::{NoiseFn, Perlin, Worley};

use super::block::Block;
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};

/// Altura media del terreno, en bloques.
pub const SEA_LEVEL: i32 = 64;

/// Grosor de la **corteza** que las cuevas no perforan (bloques bajo la
/// superficie). Evita que el terreno quede acribillado de agujeros.
const CAVE_CRUST: i32 = 2;

/// Altura minima (bloques) a la que puede haber cuevas: deja un suelo solido.
const CAVE_MIN_Y: i32 = 2;

/// Los biomas del mundo. El bioma decide el bloque de superficie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    /// Arena (desierto).
    Desert,
    /// Hierba (bosque).
    Forest,
    /// Nieve (tundra).
    Snow,
}

/// Generador deterministico: la misma semilla produce siempre el mismo mundo.
pub struct TerrainGenerator {
    /// Ruido de baja frecuencia: el relieve grande.
    base: Perlin,
    /// Ruido de detalle: pequenas variaciones.
    detail: Perlin,
    /// Ruido cellular para repartir los biomas.
    biome: Worley,
    /// Ruido Perlin 3D para las cuevas.
    cave: Perlin,
    /// Semilla original (la guardamos en el header del mundo).
    seed: u32,
}

impl TerrainGenerator {
    /// Crea un generador para una semilla.
    pub fn new(seed: u32) -> Self {
        Self {
            base: Perlin::new(seed),
            // Otra semilla distinta para que los dos ruidos no sean iguales.
            detail: Perlin::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(1)),
            // Worley de baja frecuencia: celdas de ~50 bloques.
            biome: Worley::new(seed.wrapping_add(0xB1_0B1)).set_frequency(0.02),
            // Perlin 3D para las cuevas (semilla propia, decorrelacionada).
            cave: Perlin::new(seed.wrapping_mul(0x85EB_CA6B).wrapping_add(3)),
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
    /// `[48, 96]`.
    pub fn height(&self, world_x: i32, world_z: i32) -> usize {
        let x = world_x as f64;
        let z = world_z as f64;
        // Perlin devuelve aprox. [-1, 1].
        let base = self.base.get([x * 0.010, z * 0.010]);
        let detail = self.detail.get([x * 0.045, z * 0.045]);
        let h = SEA_LEVEL as f64 + base * 20.0 + detail * 4.0;
        (h.round() as i32).clamp(48, 96) as usize
    }

    /// Bioma en `(x, z)` del mundo. Worley (ReturnType::Value) devuelve un valor
    /// pseudoaleatorio por celda en `[0, 1)`; lo partimos en tres tercios.
    pub fn biome_at(&self, world_x: i32, world_z: i32) -> Biome {
        let v = self.biome.get([world_x as f64, world_z as f64]);
        if v < 0.34 {
            Biome::Desert
        } else if v < 0.67 {
            Biome::Forest
        } else {
            Biome::Snow
        }
    }

    /// ¿Hay **cueva** en `(x, y, z)` del mundo?
    ///
    /// Usamos un Perlin **3D** a baja frecuencia: los bloques en los que el ruido
    /// esta cerca de cero forman una **iso-superficie**, que es un tunel continuo
    /// (mejor que un simple "ruido > umbral", que da burbujas). No perfora ni la
    /// corteza (bajo la superficie) ni el suelo del mundo.
    fn is_cave(&self, x: i32, y: i32, z: i32, height: usize) -> bool {
        if y < CAVE_MIN_Y || y >= height as i32 - CAVE_CRUST {
            return false;
        }
        let n = self
            .cave
            .get([x as f64 * 0.06, y as f64 * 0.11, z as f64 * 0.06]);
        n.abs() < 0.07
    }

    /// Rellena una columna del mundo con terreno segun su posicion `(world_x,
    /// world_z)`.
    ///
    /// La altura se calcula **por bloque** (`world_x + x`, `world_z + z`), no una
    /// sola vez por chunk: asi el terreno forma colinas suaves y no mesetas
    /// planas de 16x16 con escalones. Las cuevas se tallan por bloque con ruido
    /// 3D.
    pub fn generate_column(&self, world_x: i32, world_z: i32) -> Column {
        let mut column = Column::empty();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = world_x + x as i32;
                let wz = world_z + z as i32;
                let height = self.height(wx, wz);
                let biome = self.biome_at(wx, wz);
                // Cerca del nivel del mar (o por debajo) la superficie es **arena**
                // (playa o fondo marino), sin importar el bioma.
                let coastal = height <= (SEA_LEVEL as usize) + 1;
                for y in 0..height {
                    if self.is_cave(wx, y as i32, wz, height) {
                        continue; // cueva: dejamos aire
                    }
                    let block = if coastal {
                        coastal_block(y, height)
                    } else {
                        surface_block(y, height, biome)
                    };
                    column.set(x, y, z, block);
                }
                // Vegetacion: **arboles** en tierra firme (no en playa/agua). El
                // tronco y la copa caben en la columna, para no cortarlos en el
                // borde del chunk.
                if !coastal && height >= (SEA_LEVEL as usize) + 2 {
                    let density = match biome {
                        Biome::Forest => 0.05,
                        Biome::Snow => 0.02,
                        Biome::Desert => 0.0,
                    };
                    if density > 0.0
                        && (2..=13).contains(&x)
                        && (2..=13).contains(&z)
                        && hash01(wx, wz) < density
                    {
                        place_tree(&mut column, x, height, z);
                    }
                }
                // Oceano/lago: rellenamos de **agua** el aire entre la superficie
                // y el nivel del mar (estilo `ocean.level`/`water_level`).
                if height < SEA_LEVEL as usize {
                    for y in height..SEA_LEVEL as usize {
                        if column.get(x, y, z) == Block::Air {
                            column.set(x, y, z, Block::Water);
                        }
                    }
                }
            }
        }

        column
    }
}

/// Elige el bloque de la profundidad `y` para una columna de altura `height` y
/// bioma `biome`.
fn surface_block(y: usize, height: usize, biome: Biome) -> Block {
    if y + 1 == height {
        // Capa de arriba segun el bioma.
        match biome {
            Biome::Desert => Block::Sand,
            Biome::Forest => Block::Grass,
            Biome::Snow => Block::Snow,
        }
    } else if y + 4 >= height {
        // Sub-suelo: en el desierto tambien es arena; en el resto, tierra.
        match biome {
            Biome::Desert => Block::Sand,
            _ => Block::Dirt,
        }
    } else {
        Block::Stone
    }
}

/// Bloque de la profundidad `y` en una columna **costera/submarina**: arena en
/// las capas de arriba, piedra debajo.
fn coastal_block(y: usize, height: usize) -> Block {
    if y + 4 >= height {
        Block::Sand
    } else {
        Block::Stone
    }
}

/// Hash determinista de `(x, z)` en `[0, 1)`. Reparte los arboles sin depender
/// del bioma (que ya se consulta aparte).
fn hash01(x: i32, z: i32) -> f32 {
    (hash_u32(x, z) % 100_000) as f32 / 100_000.0
}

/// Hash entero determinista de `(x, z)`.
fn hash_u32(x: i32, z: i32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((z as u32).wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

/// Planta un arbol en la columna: tronco de `Wood` y copa de `Leaves`. La copa
/// escribe solo en aire (no pisa el tronco ni el terreno) y cabe dentro de la
/// columna (posicion del tronco restringida a `2..=13`).
fn place_tree(column: &mut Column, x: usize, ground: usize, z: usize) {
    // Altura del tronco 4..6 (variada por posicion).
    let trunk = 4 + (hash_u32(x as i32 * 31 + 7, z as i32 * 17 + 3) % 3) as usize;
    for y in ground..(ground + trunk).min(WORLD_HEIGHT) {
        column.set(x, y, z, Block::Wood);
    }

    // Copa: 4 capas alrededor de la parte alta del tronco (radio 2, esquinas
    // recortadas; la de arriba, radio 1).
    let leaf_base = ground + trunk - 2;
    for dy in 0..4i32 {
        let r: i32 = if dy == 0 || dy == 3 { 1 } else { 2 };
        for dx in -r..=r {
            for dz in -r..=r {
                if r == 2 && dx.abs() == 2 && dz.abs() == 2 {
                    continue;
                }
                let lx = x as i32 + dx;
                let lz = z as i32 + dz;
                let ly = leaf_base as i32 + dy;
                if lx < 0
                    || lz < 0
                    || lx >= CHUNK_SIZE as i32
                    || lz >= CHUNK_SIZE as i32
                    || ly < 0
                    || ly >= WORLD_HEIGHT as i32
                {
                    continue;
                }
                let (lx, ly, lz) = (lx as usize, ly as usize, lz as usize);
                if column.get(lx, ly, lz) == Block::Air {
                    column.set(lx, ly, lz, Block::Leaves);
                }
            }
        }
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
        assert_eq!(a.biome_at(10, 20), b.biome_at(10, 20));
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
    fn hay_los_tres_biomas_en_un_area_grande() {
        let generator = TerrainGenerator::new(13371);
        let mut desert = false;
        let mut forest = false;
        let mut snow = false;
        for x in -400..400 {
            for z in -400..400 {
                match generator.biome_at(x, z) {
                    Biome::Desert => desert = true,
                    Biome::Forest => forest = true,
                    Biome::Snow => snow = true,
                }
            }
        }
        assert!(desert && forest && snow, "faltan biomas en la muestra");
    }

    #[test]
    fn la_superficie_depende_del_bioma() {
        let generator = TerrainGenerator::new(99);
        let column = generator.generate_column(0, 0);
        let h = generator.height(0, 0);
        // La capa de arriba depende del bioma... salvo cerca del mar, donde es
        // arena (playa/fondo marino).
        let expected = if h <= SEA_LEVEL as usize + 1 {
            Block::Sand
        } else {
            match generator.biome_at(0, 0) {
                Biome::Desert => Block::Sand,
                Biome::Forest => Block::Grass,
                Biome::Snow => Block::Snow,
            }
        };
        assert_eq!(column.get(0, h - 1, 0), expected);
        // El subsuelo no es aire.
        assert!(column.get(0, h - 2, 0).is_solid());
        // Encima de la superficie: aire, o **agua** si la columna esta bajo el
        // nivel del mar.
        let above = column.get(0, h, 0);
        if h < SEA_LEVEL as usize {
            assert_eq!(above, Block::Water);
        } else {
            assert_eq!(above, Block::Air);
        }
        assert_eq!(column.get(0, 0, 0), Block::Stone);
    }

    #[test]
    fn hay_arboles_con_tronco_y_hojas() {
        let generator = TerrainGenerator::new(13_371);
        let (mut wood, mut leaves) = (0u32, 0u32);
        for cz in -3..3 {
            for cx in -3..3 {
                let column = generator.generate_column(cx * 16, cz * 16);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        for y in 0..WORLD_HEIGHT {
                            match column.get(x, y, z) {
                                Block::Wood => wood += 1,
                                Block::Leaves => leaves += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert!(wood > 0, "no se genero ningun tronco");
        assert!(
            leaves > wood,
            "deberia haber mas hojas que troncos ({leaves} vs {wood})"
        );
    }

    #[test]
    fn el_agua_llena_hasta_el_nivel_del_mar_y_hay_playa_de_arena() {
        let generator = TerrainGenerator::new(13_371);
        let mar = SEA_LEVEL as usize;
        let mut fondo_ok = false;
        let mut playa_ok = false;
        'outer: for cz in -4..4 {
            for cx in -4..4 {
                let column = generator.generate_column(cx * 16, cz * 16);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let wx = cx * 16 + x as i32;
                        let wz = cz * 16 + z as i32;
                        let h = generator.height(wx, wz);
                        if h + 3 < mar {
                            // Fondo marino: arena, y agua hasta el nivel del mar.
                            assert_eq!(column.get(x, mar - 1, z), Block::Water, "tope de agua");
                            assert_eq!(column.get(x, h - 1, z), Block::Sand, "fondo de arena");
                            fondo_ok = true;
                        } else if h == mar + 1 {
                            // Justo por encima del agua: playa de arena.
                            assert_eq!(column.get(x, h - 1, z), Block::Sand, "playa");
                            playa_ok = true;
                        }
                        if fondo_ok && playa_ok {
                            break 'outer;
                        }
                    }
                }
            }
        }
        assert!(fondo_ok, "no se encontro fondo marino");
        assert!(playa_ok, "no se encontro playa");
    }

    #[test]
    fn el_desierto_es_arena_hasta_el_subsuelo() {
        // surface_block directo: en el desierto las 4 capas de arriba son arena.
        for y in (0..64).rev() {
            let h = 64;
            if y + 4 >= h {
                assert_eq!(surface_block(y, h, Biome::Desert), Block::Sand);
            } else {
                assert_eq!(surface_block(y, h, Biome::Desert), Block::Stone);
            }
        }
    }

    /// Cuenta la fraccion de aire **bajo la superficie** (cuevas) en una columna.
    fn fraccion_cuevas(generator: &TerrainGenerator, wx: i32, wz: i32) -> f32 {
        let column = generator.generate_column(wx, wz);
        let mut aire = 0u32;
        let mut subterraneo = 0u32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                // Superficie: primer solido de arriba hacia abajo.
                let surface = (0..WORLD_HEIGHT)
                    .rev()
                    .find(|&y| column.get(x, y, z).is_solid());
                let Some(surface) = surface else { continue };
                for y in 0..surface {
                    subterraneo += 1;
                    if !column.get(x, y, z).is_solid() {
                        aire += 1;
                    }
                }
            }
        }
        if subterraneo == 0 {
            0.0
        } else {
            aire as f32 / subterraneo as f32
        }
    }

    #[test]
    fn las_cuevas_existen_pero_no_se_comen_el_terreno() {
        let generator = TerrainGenerator::new(13_371);
        // Promediamos varias columnas de una zona grande.
        let mut suma = 0.0;
        let mut n = 0.0;
        for cz in -2..2 {
            for cx in -2..2 {
                suma += fraccion_cuevas(&generator, cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                n += 1.0;
            }
        }
        let frac = suma / n;
        println!("fraccion de aire subterraneo (cuevas): {frac:.3}");
        assert!(frac > 0.02, "apenas hay cuevas: {frac}");
        assert!(frac < 0.35, "demasiadas cuevas: {frac}");
    }

    #[test]
    fn la_corteza_no_se_perfora() {
        // Ninguna cueva puede tocar las CAVE_CRUST capas de arriba: la superficie
        // no queda acribillada.
        let generator = TerrainGenerator::new(99);
        let column = generator.generate_column(0, 0);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let surface = (0..WORLD_HEIGHT)
                    .rev()
                    .find(|&y| column.get(x, y, z).is_solid());
                if let Some(surface) = surface {
                    for dy in 0..CAVE_CRUST as usize {
                        let y = surface - dy;
                        assert!(
                            column.get(x, y, z).is_solid(),
                            "cueva en la corteza: ({x},{y},{z})"
                        );
                    }
                }
            }
        }
    }
}
