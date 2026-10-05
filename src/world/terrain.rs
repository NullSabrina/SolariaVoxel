//! Generacion de terreno procedural: **clima**, **biomas**, **relieve por
//! bioma**, **cuevas 3D** y **acuiferos**.
//!
//! Flujo de una columna:
//! 1. **Clima 2D** — dos mapas `Fbm` (temperatura y humedad) en 0..1.
//! 2. **Bioma** — se deriva del par (temperatura, humedad).
//! 3. **Altura** — cada bioma aplica su propia **amplitud**, **frecuencia** y
//!    peso de relieve escarpado (`RidgedMulti` para picos de montana).
//! 4. **Superficie** — un ruido de detalle de alta frecuencia elige entre
//!    `Grass`, `CoarseDirt`, `Podzol`, `Gravel` o `Sand`.
//! 5. **Cuevas/acuiferos** — [`crate::world::caves`] decide que celda se cava y
//!    si nace llena de agua.
//!
//! Reemplaza la generacion v6 (Worley + Perlin simple). Sube `GENERATOR_VERSION`.

use std::sync::atomic::{AtomicU32, Ordering};

use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

use super::block::Block;
use super::caves::{Carve, CaveSystem};
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};

/// Altura media del terreno, en bloques (nivel del mar).
pub const SEA_LEVEL: i32 = 64;

/// Altura minima/maxima del terreno (el `clamp` del relieve).
pub const MIN_HEIGHT: i32 = 8;
pub const MAX_HEIGHT: i32 = 200;

/// Los biomas del mundo, derivados del clima.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    /// Calido y seco: dunas de arena.
    Desert,
    /// Calido y semiseco: hierba con tierra gruesa.
    Savanna,
    /// Templado y seco: llanura de hierba.
    Plains,
    /// Templado y humedo: bosque denso.
    Forest,
    /// Templado y muy humedo: tierras bajas y encharcadas.
    Swamp,
    /// Frio y humedo: taiga nevada con picos escarpados.
    Taiga,
    /// Frio y seco: tundra nevada.
    Tundra,
}

impl Biome {
    /// Perfil de relieve: `(amplitud, frecuencia, peso del RidgedMulti)`.
    ///
    /// La amplitud multiplica la onda grande (20 bloques) y la frecuencia
    /// escala la coordenada de entrada (mas alta = colinas mas estrechas). El
    /// peso del ridged anade picos; el desierto es casi llano y la taiga montana.
    fn relief(self) -> (f64, f64, f64) {
        match self {
            Biome::Desert => (0.35, 0.6, 0.0),
            Biome::Savanna => (0.70, 0.85, 0.0),
            Biome::Plains => (0.45, 0.70, 0.0),
            Biome::Forest => (1.00, 1.00, 0.0),
            Biome::Swamp => (0.20, 0.55, 0.0),
            Biome::Taiga => (1.45, 1.20, 0.75),
            Biome::Tundra => (1.05, 0.90, 0.35),
        }
    }

    /// Densidad de arboles por columna (fraccion de columnas con arbol).
    fn tree_density(self) -> f32 {
        match self {
            Biome::Forest => 0.07,
            Biome::Taiga => 0.05,
            Biome::Swamp => 0.03,
            Biome::Plains => 0.01,
            Biome::Desert | Biome::Savanna | Biome::Tundra => 0.0,
        }
    }
}

/// Bioma a partir del par clima (Whittaker simplificado). Funcion **pura**: no
/// toca ruido, de modo que `generate_column` reutiliza el clima ya calculado.
fn biome_of(t: f64, h: f64) -> Biome {
    if t < 0.32 {
        if h > 0.55 {
            Biome::Taiga
        } else {
            Biome::Tundra
        }
    } else if t > 0.68 {
        if h < 0.38 {
            Biome::Desert
        } else {
            Biome::Savanna
        }
    } else if h > 0.72 {
        Biome::Swamp
    } else if h < 0.35 {
        Biome::Plains
    } else {
        Biome::Forest
    }
}

/// ¿El clima esta cerca de un borde de bioma? Sirve para **mezclar** materiales
/// en la transicion (parches del bioma vecino) sin evaluar biomas vecinos.
fn near_climate_edge(t: f64, h: f64) -> bool {
    const EDGES: [f64; 6] = [0.32, 0.68, 0.35, 0.38, 0.55, 0.72];
    EDGES
        .iter()
        .any(|&e| (t - e).abs() < 0.035 || (h - e).abs() < 0.035)
}

/// Generador deterministico: la misma semilla produce siempre el mismo mundo.
pub struct TerrainGenerator {
    /// Clima (2D): temperatura.
    temperature: Fbm<Perlin>,
    /// Clima (2D): humedad.
    humidity: Fbm<Perlin>,
    /// Relieve grande (`Fbm` suave).
    continent: Fbm<Perlin>,
    /// Detalle fino del relieve.
    detail: Perlin,
    /// Crestas escarpadas para montanas.
    ridged: RidgedMulti<Perlin>,
    /// Ruido de alta frecuencia que varia la capa de superficie.
    surface_detail: Perlin,
    /// Nivel del acuifero por columna (2D).
    aquifer: Perlin,
    /// Mascara 2D que decide **que columnas** tienen cuevas. Evita evaluar el
    /// ruido 3D (caro) en columnas macizas: es la mayor parte del coste de
    /// generar una columna, y asi el streaming no da tirones.
    cave_mask: Perlin,
    /// Cuevas 3D.
    caves: CaveSystem,
    seed: u32,
    /// Contador de evaluaciones de ruido **2D** (test de cache). Es atomico
    /// (`AtomicU32`) en vez de `Cell` para que el generador sea `Send + Sync` y
    /// pueda compartirse entre workers de generacion.
    noise_calls: AtomicU32,
}

impl TerrainGenerator {
    /// Crea un generador para una semilla. Cada capa usa una semilla derivada
    /// distinta para que los ruidos no correlacionen (si compartieran semilla,
    /// el clima seguiria al relieve).
    pub fn new(seed: u32) -> Self {
        let mix = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        Self {
            temperature: Fbm::<Perlin>::new(mix(1))
                .set_octaves(3)
                .set_frequency(1.0)
                .set_persistence(0.5),
            humidity: Fbm::<Perlin>::new(mix(2))
                .set_octaves(3)
                .set_frequency(1.0)
                .set_persistence(0.5),
            continent: Fbm::<Perlin>::new(mix(3))
                .set_octaves(4)
                .set_frequency(1.0)
                .set_persistence(0.5),
            detail: Perlin::new(mix(4)),
            ridged: RidgedMulti::<Perlin>::new(mix(5))
                .set_octaves(4)
                .set_frequency(1.0),
            surface_detail: Perlin::new(mix(6)),
            aquifer: Perlin::new(mix(7)),
            cave_mask: Perlin::new(mix(8)),
            caves: CaveSystem::new(seed),
            seed,
            noise_calls: AtomicU32::new(0),
        }
    }

    /// La semilla con la que se creo.
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Cuenta una evaluacion de ruido 2D (micro-coste; solo para el test).
    #[inline]
    fn bump(&self) {
        self.noise_calls.fetch_add(1, Ordering::Relaxed);
    }

    /// Evaluaciones de ruido 2D desde el ultimo reset.
    pub fn noise_calls(&self) -> u32 {
        self.noise_calls.load(Ordering::Relaxed)
    }

    /// Reinicia el contador de ruido 2D.
    pub fn reset_noise_calls(&self) {
        self.noise_calls.store(0, Ordering::Relaxed);
    }

    /// Clima de `(x, z)` -> `(temperatura, humedad)` en 0..1.
    pub fn climate(&self, world_x: i32, world_z: i32) -> (f64, f64) {
        // Frecuencia espacial baja (0.004): las franjas climaticas ocupan cientos
        // de bloques, no unos pocos.
        self.bump();
        let t = self
            .temperature
            .get([world_x as f64 * 0.004, world_z as f64 * 0.004]);
        self.bump();
        let h = self
            .humidity
            .get([world_x as f64 * 0.004, world_z as f64 * 0.004]);
        (
            (t * 0.5 + 0.5).clamp(0.0, 1.0),
            (h * 0.5 + 0.5).clamp(0.0, 1.0),
        )
    }

    /// Bioma en `(x, z)` a partir del clima (diagrama de Whittaker simplificado).
    pub fn biome_at(&self, world_x: i32, world_z: i32) -> Biome {
        let (t, h) = self.climate(world_x, world_z);
        biome_of(t, h)
    }

    /// Nivel del acuifero en `(x, z)`, en 30..56. Por debajo se llenan de agua
    /// las cuevas; por encima, quedan secas.
    pub fn aquifer_level(&self, world_x: i32, world_z: i32) -> i32 {
        self.bump();
        let n = self
            .aquifer
            .get([world_x as f64 * 0.01, world_z as f64 * 0.01]);
        30 + ((n * 0.5 + 0.5) * 26.0) as i32
    }

    /// Altura maxima de las pozas de lava (profundas, sobre la bedrock).
    pub const LAVA_TOP: i32 = 11;

    /// ¿La columna `(x, z)` es el centro de una poza de lava? Celdas de 3x3 con
    /// un 12% de probabilidad (hash determinista, independiente de la columna
    /// vecina, para que el borde de obsidiana coincida).
    fn is_lava_pool(x: i32, z: i32) -> bool {
        hash01(x.div_euclid(3), z.div_euclid(3)) < 0.12
    }

    /// ¿`y` esta en la banda donde puede haber lava?
    fn is_lava_band(y: usize) -> bool {
        (super::caves::BEDROCK_CLEAR as usize) < y && y <= Self::LAVA_TOP as usize
    }

    /// ¿Nace lava en la celda `(x, y, z)`? Se cumplen tres cosas: estar en la
    /// banda profunda, caer en una celda de poza y tener suelo firme (roca sin
    /// cavar justo debajo, que se horneara a obsidiana).
    fn is_lava_here(
        caves: &CaveSystem,
        x: i32,
        y: usize,
        z: i32,
        surface: i32,
        aquifer: i32,
    ) -> bool {
        Self::is_lava_band(y)
            && Self::is_lava_pool(x, z)
            && y > 0
            && caves.carve(x, y as i32 - 1, z, surface, aquifer) == Carve::None
    }

    /// Altura con un bioma **ya conocido** (evita recalcular el clima).
    fn height_for(&self, world_x: i32, world_z: i32, biome: Biome) -> usize {
        let (amp, freq, ridged_w) = biome.relief();
        let (fx, fz) = (world_x as f64, world_z as f64);

        // La frecuencia por bioma se aplica escalando las coordenadas de entrada
        // (el ruido base trabaja a 0.010). Es mas barato que reconfigurar el
        // ruido, que no admite frecuencia variable por muestra.
        self.bump();
        let base = self.continent.get([fx * 0.010 * freq, fz * 0.010 * freq]);
        self.bump();
        let detail = self.detail.get([fx * 0.045 * freq, fz * 0.045 * freq]);
        let mut h = SEA_LEVEL as f64 + base * 20.0 * amp + detail * 4.0;

        if ridged_w > 0.0 {
            self.bump();
            let r = self.ridged.get([fx * 0.010 * freq, fz * 0.010 * freq]);
            h += (r - 0.5) * 26.0 * amp * ridged_w;
        }

        (h.round() as i32).clamp(MIN_HEIGHT, MAX_HEIGHT) as usize
    }

    /// Altura del terreno (numero de bloques solidos) en `(x, z)`.
    pub fn height(&self, world_x: i32, world_z: i32) -> usize {
        let biome = self.biome_at(world_x, world_z);
        self.height_for(world_x, world_z, biome)
    }

    /// Ruido de detalle de superficie (alta frecuencia, por columna): elige la
    /// variante de bloque de la capa superior.
    fn surface_variant(&self, world_x: i32, world_z: i32) -> f64 {
        self.bump();
        self.surface_detail
            .get([world_x as f64 * 0.11, world_z as f64 * 0.11])
    }

    /// Solo las columnas con mascara alta pagan el ruido 3D de cuevas.
    fn has_caves(&self, world_x: i32, world_z: i32) -> bool {
        self.bump();
        self.cave_mask
            .get([world_x as f64 * 0.017, world_z as f64 * 0.017])
            > -0.30
    }

    /// ¿Pendiente admisible para un arbol? Compara la altura con las 4 vecinas:
    /// mas de 1 bloque de diferencia = ladera, no se planta.
    fn slope_ok(&self, world_x: i32, world_z: i32, height: usize) -> bool {
        let h = height as i32;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            if (self.height(world_x + dx, world_z + dz) as i32 - h).abs() > 1 {
                return false;
            }
        }
        true
    }

    /// Rellena una columna del mundo con terreno segun su posicion `(x, z)`.
    pub fn generate_column(&self, world_x: i32, world_z: i32) -> Column {
        let mut column = Column::empty();
        // Candidatos a arboles (pasada 1); se plantan en la pasada 2, cuando la
        // columna ya esta completa.
        let mut tree_candidates: Vec<(usize, usize, usize)> = Vec::new();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = world_x + x as i32;
                let wz = world_z + z as i32;
                // --- Ruido 2D: UNA sola vez por (x, z) ---
                let (t, h) = self.climate(wx, wz);
                let biome = biome_of(t, h);
                let height = self.height_for(wx, wz, biome);
                let aquifer = self.aquifer_level(wx, wz);
                let variant = self.surface_variant(wx, wz);
                let border = near_climate_edge(t, h);
                let has_caves = self.has_caves(wx, wz);
                // Cerca del mar la superficie es arena (playa/fondo marino).
                let coastal = height <= (SEA_LEVEL as usize) + 1;

                for y in 0..height {
                    // Cuevas y acuiferos antes de colocar el terreno.
                    if has_caves {
                        match self.caves.carve(wx, y as i32, wz, height as i32, aquifer) {
                            Carve::Air => {
                                // ¿Poza de lava? Cueva con suelo firme en la
                                // banda profunda: se rellena de lava y el suelo
                                // se "hornea" a obsidiana.
                                if Self::is_lava_here(
                                    &self.caves,
                                    wx,
                                    y,
                                    wz,
                                    height as i32,
                                    aquifer,
                                ) {
                                    column.set(x, y, z, Block::Lava);
                                    column.set(x, y - 1, z, Block::Obsidian);
                                }
                                continue;
                            }
                            Carve::Water => {
                                // En una poza de lava el calor evapora el agua
                                // del acuifero: tambien nace lava.
                                if Self::is_lava_here(
                                    &self.caves,
                                    wx,
                                    y,
                                    wz,
                                    height as i32,
                                    aquifer,
                                ) {
                                    column.set(x, y, z, Block::Lava);
                                    column.set(x, y - 1, z, Block::Obsidian);
                                    continue;
                                }
                                column.set(x, y, z, Block::Water);
                                continue;
                            }
                            Carve::None => {}
                        }
                    }
                    let block = if coastal {
                        coastal_block(y, height, variant)
                    } else {
                        surface_block(y, height, biome, variant, border)
                    };
                    column.set(x, y, z, block);
                }

                // Oceano/lago: rellena de agua el aire entre la superficie y el
                // nivel del mar.
                if height < SEA_LEVEL as usize {
                    for y in height..SEA_LEVEL as usize {
                        if column.get(x, y, z) == Block::Air {
                            column.set(x, y, z, Block::Water);
                        }
                    }
                }

                // Decoracion: candidato a arbol (interior, densidad, pendiente).
                let density = biome.tree_density();
                if !coastal
                    && density > 0.0
                    && (2..=13).contains(&x)
                    && (2..=13).contains(&z)
                    && hash01(wx, wz) < density
                    && self.slope_ok(wx, wz, height)
                {
                    tree_candidates.push((x, z, height));
                }
            }
        }

        // Pasada 2: plantar arboles con hueco libre (nada flotando ni embebido).
        for (x, z, ground) in tree_candidates {
            if headroom_clear(&column, x, ground, z) {
                place_tree(&mut column, x, ground, z);
            }
        }

        column
    }
}

/// Bloque de la capa `y` para un bioma templado/frio/calido.
///
/// `variant` (ruido de alta frecuencia por columna) ensucia la superficie con
/// variantes: tierra gruesa, podzol y grava. Asi dos columnas del mismo bioma
/// no salen identicas.
fn surface_block(y: usize, height: usize, biome: Biome, variant: f64, border: bool) -> Block {
    if y + 1 == height {
        // Borde de bioma: parches del material vecino para mezclar la transicion
        // sin evaluar el bioma de las columnas contiguas.
        if border && variant.abs() < 0.35 {
            return match biome {
                Biome::Desert | Biome::Savanna => Block::CoarseDirt,
                Biome::Taiga | Biome::Tundra => Block::Dirt,
                Biome::Swamp => Block::CoarseDirt,
                _ => Block::Sand,
            };
        }
        // Capa superior.
        match biome {
            Biome::Desert => Block::Sand,
            // Sabana: hierba salpicada de tierra gruesa.
            Biome::Savanna => {
                if variant > 0.55 {
                    Block::CoarseDirt
                } else {
                    Block::Grass
                }
            }
            // Llanura: hierba con manchas de tierra gruesa.
            Biome::Plains => {
                if variant > 0.6 {
                    Block::CoarseDirt
                } else {
                    Block::Grass
                }
            }
            // Bosque: base de hierba con calvas de podzol.
            Biome::Forest => {
                if variant < -0.5 {
                    Block::Podzol
                } else {
                    Block::Grass
                }
            }
            // Pantano: hierba (a menudo encharcada por el nivel del mar).
            Biome::Swamp => Block::Grass,
            // Taiga: nieve con calvas de podzol.
            Biome::Taiga => {
                if variant < -0.5 {
                    Block::Podzol
                } else {
                    Block::Snow
                }
            }
            // Tundra: nieve con pedreras de grava.
            Biome::Tundra => {
                if variant < -0.6 {
                    Block::Gravel
                } else {
                    Block::Snow
                }
            }
        }
    } else if y + 4 >= height {
        // Subsuelo (4 capas).
        match biome {
            Biome::Desert | Biome::Savanna => Block::Sand,
            Biome::Taiga => {
                if variant < 0.0 {
                    Block::Dirt
                } else {
                    Block::CoarseDirt
                }
            }
            Biome::Tundra => {
                if variant < 0.5 {
                    Block::Dirt
                } else {
                    Block::CoarseDirt
                }
            }
            Biome::Swamp => {
                if variant < 0.2 {
                    Block::CoarseDirt
                } else {
                    Block::Dirt
                }
            }
            _ => {
                if variant > 0.7 {
                    Block::Gravel
                } else {
                    Block::Dirt
                }
            }
        }
    } else {
        // Roca madre: piedra, con bolsas de grava donde el detalle es alto.
        if variant > 0.92 {
            Block::Gravel
        } else {
            Block::Stone
        }
    }
}

/// Bloque de una columna **costera/submarina**: arena arriba, piedra debajo,
/// con algun banco de grava.
fn coastal_block(y: usize, height: usize, variant: f64) -> Block {
    if y + 4 >= height {
        Block::Sand
    } else if variant > 0.85 {
        Block::Gravel
    } else {
        Block::Stone
    }
}

/// Hash determinista de `(x, z)` en `[0, 1)`. Reparte los arboles.
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

/// ¿La columna tiene hueco libre alrededor del tronco? Comprueba un 3x3 en
/// `ground..ground+6`: evita que un arbol se plante flotando sobre una cueva o
/// dentro de otra copa.
fn headroom_clear(column: &Column, x: usize, ground: usize, z: usize) -> bool {
    for dy in 0..=6 {
        let y = ground + dy;
        if y >= WORLD_HEIGHT {
            break;
        }
        for dz in -1..=1i32 {
            for dx in -1..=1i32 {
                let lx = x as i32 + dx;
                let lz = z as i32 + dz;
                if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
                    continue;
                }
                if column.get(lx as usize, y, lz as usize) != Block::Air {
                    return false;
                }
            }
        }
    }
    true
}

/// Planta un arbol: tronco de `Wood` y copa de `Leaves`, sin pisar el terreno ni
/// el tronco y cabiendo dentro de la columna.
fn place_tree(column: &mut Column, x: usize, ground: usize, z: usize) {
    let trunk = 4 + (hash_u32(x as i32 * 31 + 7, z as i32 * 17 + 3) % 3) as usize;
    for y in ground..(ground + trunk).min(WORLD_HEIGHT) {
        column.set(x, y, z, Block::Wood);
    }

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

/// Cota superior de la altura (para validaciones externas).
pub fn max_height() -> usize {
    let _ = WORLD_HEIGHT;
    MAX_HEIGHT as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_generador_es_send_y_sync() {
        // Prerequisito para generar columnas en hilos de trabajo.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TerrainGenerator>();
    }

    #[test]
    fn el_ruido_2d_no_se_llama_por_bloque_y() {
        // La clave de la optimizacion: 16x16 = 256 celdas de columna; si el
        // ruido 2D se llamara dentro del bucle `for y` serian ~18000. Debe
        // quedar muy por debajo (O(256), no O(256 * altura)).
        let g = TerrainGenerator::new(7);
        g.reset_noise_calls();
        let _ = g.generate_column(0, 0);
        let calls = g.noise_calls();
        println!("ruido 2D en una columna: {calls} evaluaciones");
        assert!(
            calls < CHUNK_SIZE as u32 * CHUNK_SIZE as u32 * 12,
            "demasiadas evaluaciones 2D: {calls}"
        );
    }

    #[test]
    fn los_arboles_no_se_plantan_en_pendientes_ni_flotando() {
        // Invariante de la decoracion: la base de un tronco se apoya en solido y
        // ninguna vecina difiere mas de 1 bloque de altura.
        let g = TerrainGenerator::new(13_371);
        for cz in -2..2 {
            for cx in -2..2 {
                let (wx0, wz0) = (cx * 16, cz * 16);
                let column = g.generate_column(wx0, wz0);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        for y in 1..WORLD_HEIGHT - 1 {
                            let here = column.get(x, y, z);
                            let below = column.get(x, y - 1, z);
                            if here == Block::Wood && below != Block::Wood {
                                assert!(below.is_solid(), "tronco flotando ({x},{y},{z})");
                                let (wx, wz) = (wx0 + x as i32, wz0 + z as i32);
                                assert!(g.slope_ok(wx, wz, y), "tronco en pendiente ({wx},{z})");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn la_misma_semilla_da_el_mismo_mundo() {
        let a = TerrainGenerator::new(1234);
        let b = TerrainGenerator::new(1234);
        for (x, z) in [(0, 0), (37, -21), (200, 200)] {
            assert_eq!(a.height(x, z), b.height(x, z));
            assert_eq!(a.biome_at(x, z), b.biome_at(x, z));
            assert_eq!(a.aquifer_level(x, z), b.aquifer_level(x, z));
        }
    }

    #[test]
    fn semillas_distintas_dan_mundos_distintos() {
        let a = TerrainGenerator::new(1);
        let b = TerrainGenerator::new(2);
        let distintos = (0..64)
            .filter(|&x| a.height(x, 0) != b.height(x, 0))
            .count();
        assert!(distintos > 0);
    }

    #[test]
    fn la_altura_esta_dentro_de_limites() {
        let g = TerrainGenerator::new(7);
        for x in -80..80 {
            for z in -80..80 {
                let h = g.height(x, z);
                assert!(
                    (MIN_HEIGHT..=MAX_HEIGHT).contains(&(h as i32)),
                    "altura fuera de rango: {h}"
                );
            }
        }
    }

    #[test]
    fn aparecen_todos_los_biomas_en_un_area_grande() {
        let g = TerrainGenerator::new(13_371);
        let mut vistos = [false; 7];
        for x in (-800..800).step_by(8) {
            for z in (-800..800).step_by(8) {
                vistos[match g.biome_at(x, z) {
                    Biome::Desert => 0,
                    Biome::Savanna => 1,
                    Biome::Plains => 2,
                    Biome::Forest => 3,
                    Biome::Swamp => 4,
                    Biome::Taiga => 5,
                    Biome::Tundra => 6,
                }] = true;
            }
        }
        assert!(vistos.iter().all(|&v| v), "faltan biomas: {vistos:?}");
    }

    #[test]
    fn el_clima_esta_normalizado() {
        let g = TerrainGenerator::new(99);
        for x in (-300..300).step_by(7) {
            for z in (-300..300).step_by(7) {
                let (t, h) = g.climate(x, z);
                assert!((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&h));
            }
        }
    }

    #[test]
    fn la_superficie_depende_del_bioma() {
        let g = TerrainGenerator::new(99);
        let column = g.generate_column(0, 0);
        let h = g.height(0, 0);
        let biome = g.biome_at(0, 0);
        let top = column.get(0, h - 1, 0);
        // La capa superior esta entre las variantes validas del sistema; para el
        // desierto es arena y para la tundra/taiga, nieve o podzol.
        match biome {
            Biome::Desert => assert_eq!(top, Block::Sand),
            Biome::Tundra => assert!(matches!(top, Block::Snow | Block::Gravel)),
            Biome::Taiga => assert!(matches!(top, Block::Snow | Block::Podzol)),
            _ => assert!(matches!(
                top,
                Block::Grass | Block::CoarseDirt | Block::Podzol | Block::Sand | Block::Gravel
            )),
        }
    }

    #[test]
    fn hay_arboles_con_tronco_y_hojas() {
        let g = TerrainGenerator::new(13_371);
        let (mut wood, mut leaves) = (0u32, 0u32);
        for cz in -3..3 {
            for cx in -3..3 {
                let column = g.generate_column(cx * 16, cz * 16);
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
        assert!(leaves > wood, "menos hojas que troncos");
    }

    #[test]
    fn el_agua_llena_hasta_el_nivel_del_mar() {
        let g = TerrainGenerator::new(13_371);
        let mar = SEA_LEVEL as usize;
        let mut fondo_ok = false;
        'outer: for cz in -4..4 {
            for cx in -4..4 {
                let column = g.generate_column(cx * 16, cz * 16);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let wx = cx * 16 + x as i32;
                        let wz = cz * 16 + z as i32;
                        let h = g.height(wx, wz);
                        if h + 3 < mar {
                            assert_eq!(column.get(x, mar - 1, z), Block::Water, "tope de agua");
                            fondo_ok = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
        assert!(fondo_ok, "no se encontro fondo marino");
    }

    #[test]
    fn los_acuiferos_rellenan_cuevas_profundas() {
        let g = TerrainGenerator::new(13_371);
        let mut agua_subterranea = 0u32;
        for cz in -2..2 {
            for cx in -2..2 {
                let column = g.generate_column(cx * 16, cz * 16);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let wx = cx * 16 + x as i32;
                        let wz = cz * 16 + z as i32;
                        let aquifer = g.aquifer_level(wx, wz) as usize;
                        // Por debajo del acuifero y por encima de la bedrock,
                        // un bloque de agua en una cueva es acuifero.
                        for y in BEDROCK..aquifer.min(40) {
                            if column.get(x, y, z) == Block::Water {
                                agua_subterranea += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(agua_subterranea > 0, "los acuiferos no llenaron cuevas");
    }

    const BEDROCK: usize = 6;

    /// Cuenta la fraccion de aire subterraneo (cuevas) de una columna.
    fn fraccion_cuevas(g: &TerrainGenerator, wx: i32, wz: i32) -> f32 {
        let column = g.generate_column(wx, wz);
        let mut aire = 0u32;
        let mut subterraneo = 0u32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let surface = (0..WORLD_HEIGHT)
                    .rev()
                    .find(|&y| column.get(x, y, z).is_solid());
                let Some(surface) = surface else { continue };
                for y in 0..surface {
                    if !column.get(x, y, z).is_solid() && column.get(x, y, z) != Block::Water {
                        aire += 1;
                    }
                    subterraneo += 1;
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
        let g = TerrainGenerator::new(13_371);
        let mut suma = 0.0;
        let mut n = 0.0;
        for cz in -2..2 {
            for cx in -2..2 {
                suma += fraccion_cuevas(&g, cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                n += 1.0;
            }
        }
        let frac = suma / n;
        println!("fraccion de aire subterraneo (cuevas): {frac:.3}");
        assert!(frac > 0.002, "apenas hay cuevas: {frac}");
        assert!(frac < 0.35, "demasiadas cuevas: {frac}");
    }

    #[test]
    fn la_corteza_no_se_perfora() {
        let g = TerrainGenerator::new(99);
        let column = g.generate_column(0, 0);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let surface = (0..WORLD_HEIGHT)
                    .rev()
                    .find(|&y| column.get(x, y, z).is_solid());
                if let Some(surface) = surface {
                    for dy in 0..crate::world::caves::CAVE_CRUST as usize {
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

    #[test]
    fn las_pozas_de_lava_nacen_profundas_con_suelo_de_obsidiana() {
        let g = TerrainGenerator::new(13_371);
        let mut lava = 0u32;
        for cz in 0..4 {
            for cx in 0..4 {
                let column = g.generate_column(cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                for y in 0..24usize {
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            if column.get(x, y, z) != Block::Lava {
                                continue;
                            }
                            assert!(
                                (6..=11).contains(&y),
                                "lava fuera de la banda profunda: y={y}"
                            );
                            assert_eq!(
                                column.get(x, y - 1, z),
                                Block::Obsidian,
                                "lava sin suelo de obsidiana en ({x},{y},{z})"
                            );
                            lava += 1;
                        }
                    }
                }
            }
        }
        assert!(lava > 0, "deberia haber pozas de lava en 64x64");
    }
}
