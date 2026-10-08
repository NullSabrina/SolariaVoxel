//! Generacion de terreno procedimental: **geografia continental**, **clima**,
//! **biomas**, **cuevas 3D** y **acuiferos**.
//!
//! Flujo de una columna (tras la auditoria de worldgen):
//! 1. **Geografia** — [`super::worldgen`] produce un [`TerrainSample`]
//!    (continentalness con domain warping, red celular, costa de ancho variable,
//!    altura base con relieve macro + cordilleras + valles). El relieve ya **no**
//!    depende del bioma.
//! 2. **Clima 2D** — dos mapas `Fbm` (temperatura y humedad) en 0..1.
//! 3. **Bioma** — se deriva del par (temperatura, humedad) con regionalizacion
//!    por celula (FASE 3); decide materiales y vegetacion.
//! 4. **Superficie** — un ruido de alta frecuencia elige la variante de bloque.
//! 5. **Cuevas/acuiferos** — [`crate::world::caves`] decide que celda se cava y
//!    si nace llena de agua (FASE 6: cuevas jerarquicas).
//!
//! `GENERATOR_VERSION` va por 12 (FASE 6).
//!
//! [`TerrainSample`]: super::worldgen::TerrainSample

use std::sync::atomic::{AtomicU32, Ordering};

use noise::{NoiseFn, Perlin};

use super::block::Block;
use super::caves::{Carve, CaveContext, CaveSystem};
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};
use super::generator::GeneratorKind;
use super::worldgen::decoration::{DecorationKind, Decorator};
use super::worldgen::graph::{
    Graph, Program, climate_graph, default_density_graph, default_height_graph,
};
use super::worldgen::{WorldGen, biomes};

/// Altura media del terreno, en bloques (nivel del mar).
pub const SEA_LEVEL: i32 = 64;

/// Altura minima/maxima del terreno (el `clamp` del relieve).
pub const MIN_HEIGHT: i32 = 8;
pub const MAX_HEIGHT: i32 = 200;

/// Los biomas del mundo (FASE 3: seleccionados por scoring en `worldgen::biomes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// Densidad de arboles por columna (fraccion de columnas con arbol). Sale de
    /// la **definicion** del bioma (FASE 3), no de un `match` aparte.
    pub(crate) fn tree_density(self) -> f32 {
        biomes::definition(self).tree_density
    }
}

/// Generador deterministico: la misma semilla produce siempre el mismo mundo.
pub struct TerrainGenerator {
    /// Generador de mundo por etapas (FASE 1/2/3): continentalness, celular,
    /// costas, relieve, clima y **bioma**. **Sustituye** a los antiguos ruidos y
    /// al clasificador de bioma por umbrales.
    worldgen: WorldGen,
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
    /// Decoracion por reglas (FASE 7): arboles con claros y rocas.
    decorator: Decorator,
    seed: u32,
    /// Contador de evaluaciones de ruido **2D** (test de cache). Es atomico
    /// (`AtomicU32`) en vez de `Cell` para que el generador sea `Send + Sync` y
    /// pueda compartirse entre workers de generacion.
    noise_calls: AtomicU32,
    /// Tipo de generador (coexistencia legacy/graph).
    kind: GeneratorKind,
    /// Grafo de **altura** + programa (solo si `kind == Graph`): superficie para
    /// elegir materiales.
    height_graph: Option<(Graph, Program)>,
    /// Grafo de **densidad 3D** + programa (solo si `kind == Graph`): decide
    /// solido/aire con cuevas y voladizos.
    density_graph: Option<(Graph, Program)>,
    /// Grafo de **clima** `(temperatura, lluvia)` + programas (solo `Graph`).
    climate: Option<((Graph, Program), (Graph, Program))>,
}

impl TerrainGenerator {
    /// Crea un generador para una semilla con el generador **legacy** (por
    /// etapas). Es el camino de los mundos existentes.
    pub fn new(seed: u32) -> Self {
        Self::with_kind(seed, GeneratorKind::Legacy16)
    }

    /// Crea un generador del tipo pedido. El camino `Graph` compila el **grafo de
    /// densidad por defecto** (data-driven) que produce la altura del terreno.
    pub fn with_kind(seed: u32, kind: GeneratorKind) -> Self {
        let mix = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        let compile = |g: Graph| {
            let p = g
                .compile()
                .expect("el grafo de densidad por defecto debe ser valido");
            (g, p)
        };
        let (height_graph, density_graph) = if kind == GeneratorKind::Graph {
            (
                Some(compile(default_height_graph(seed as u64))),
                Some(compile(default_density_graph(seed as u64))),
            )
        } else {
            (None, None)
        };
        let climate = (kind == GeneratorKind::Graph).then(|| {
            (
                compile(climate_graph(seed as u64, 0x51ED_2701_3C6E_F372)),
                compile(climate_graph(seed as u64, 0xA076_1D64_78BD_642F)),
            )
        });
        Self {
            worldgen: WorldGen::new(seed),
            surface_detail: Perlin::new(mix(6)),
            aquifer: Perlin::new(mix(7)),
            cave_mask: Perlin::new(mix(8)),
            caves: CaveSystem::new(seed),
            decorator: Decorator::new(seed),
            seed,
            noise_calls: AtomicU32::new(0),
            kind,
            height_graph,
            density_graph,
            climate,
        }
    }

    /// Tipo de generador.
    pub fn kind(&self) -> GeneratorKind {
        self.kind
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

    /// Evaluaciones de ruido 2D desde el ultimo reset (las propias + las del
    /// `WorldGen`).
    pub fn noise_calls(&self) -> u32 {
        self.noise_calls.load(Ordering::Relaxed) + self.worldgen.noise_calls()
    }

    /// Reinicia los contadores de ruido 2D.
    pub fn reset_noise_calls(&self) {
        self.noise_calls.store(0, Ordering::Relaxed);
        self.worldgen.reset_noise_calls();
    }

    /// Clima efectivo de `(x, z)` -> `(temperatura, humedad)` en 0..1. Incluye la
    /// mezcla con el centro de la celda y el lapse de altitud (FASE 3).
    pub fn climate(&self, world_x: i32, world_z: i32) -> (f64, f64) {
        let s = self.worldgen.sample(world_x as f64, world_z as f64);
        (s.temperature as f64, s.humidity as f64)
    }

    /// Bioma en `(x, z)` seleccionado por scoring (FASE 3).
    pub fn biome_at(&self, world_x: i32, world_z: i32) -> Biome {
        self.worldgen.sample(world_x as f64, world_z as f64).biome
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
        ctx: &CaveContext,
        x: i32,
        y: usize,
        z: i32,
        surface: i32,
        aquifer: i32,
    ) -> bool {
        Self::is_lava_band(y)
            && Self::is_lava_pool(x, z)
            && y > 0
            && caves.carve(ctx, x, y as i32 - 1, z, surface, aquifer) == Carve::None
    }

    /// Muestra geografica de una columna (continentalness, celda, costa,
    /// altura base). Es la interfaz publica de la FASE 1/2 para previews/tests.
    pub fn sample(&self, world_x: i32, world_z: i32) -> super::worldgen::TerrainSample {
        self.worldgen.sample(world_x as f64, world_z as f64)
    }

    /// ¿Se cava la celda `(x, y, z)` (cueva, seca o inundada)? Consulta **barata**
    /// para previews/metricas: muestrea la geografia una vez y decide, sin
    /// generar la columna entera.
    pub fn cave_carve_at(&self, x: i32, y: i32, z: i32) -> bool {
        if !self.has_caves(x, z) {
            return false;
        }
        let s = self.worldgen.sample(x as f64, z as f64);
        let height = (s.base_height.round() as i32).clamp(MIN_HEIGHT, MAX_HEIGHT);
        let ctx = self.caves.context(x, z, s.mountain_mask);
        let aquifer = self.aquifer_level(x, z);
        !matches!(
            self.caves.carve(&ctx, x, y, z, height, aquifer),
            Carve::None
        )
    }

    /// Altura del terreno (numero de bloques solidos) en `(x, z)`.
    ///
    /// El relieve ya **no depende del bioma** (auditoria de worldgen #20): viene
    /// de la geografia continental + relieve macro + cordilleras + valles del
    /// [`WorldGen`]. El bioma solo decide materiales y vegetacion.
    pub fn height(&self, world_x: i32, world_z: i32) -> usize {
        let h = self
            .worldgen
            .sample(world_x as f64, world_z as f64)
            .base_height;
        (h.round() as i32).clamp(MIN_HEIGHT, MAX_HEIGHT) as usize
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

    /// Rellena una columna del mundo con terreno segun su posicion `(x, z)`.
    ///
    /// Muestrea la geografia en una **rejilla con padding de 1** (18x18) para que
    /// la pendiente de cada celda salga de vecinos **ya calculados**, sin volver a
    /// llamar a `height()` por candidato de decoracion (FASE 7).
    pub fn generate_column(&self, world_x: i32, world_z: i32) -> Column {
        match self.kind {
            GeneratorKind::Legacy16 => self.generate_column_legacy(world_x, world_z),
            GeneratorKind::Graph => self.generate_column_graph(world_x, world_z),
        }
    }

    /// Altura de superficie segun el **grafo** (para elegir materiales).
    fn graph_height(&self, world_x: i32, world_z: i32) -> usize {
        let Some((graph, prog)) = &self.height_graph else {
            return self.height(world_x, world_z);
        };
        let h = prog.eval(graph, world_x as f32, 0.0, world_z as f32);
        (h.round() as i32).clamp(MIN_HEIGHT, MAX_HEIGHT) as usize
    }

    /// Clima `(temperatura, lluvia)` del **grafo**, o `fallback` (WorldGen) si no
    /// hay grafo de clima.
    fn graph_climate(&self, world_x: i32, world_z: i32, fallback: (f32, f32)) -> (f32, f32) {
        let Some((t, r)) = &self.climate else {
            return fallback;
        };
        let eval = |(g, p): &(Graph, Program)| {
            p.eval(g, world_x as f32, 0.0, world_z as f32)
                .clamp(0.0, 1.0)
        };
        (eval(t), eval(r))
    }

    /// Genera una columna con el **campo de densidad 3D** del grafo, evaluado en
    /// una **retícula gruesa 4x4x4** e interpolado trilinealmente (C2): asi el
    /// ruido 3D no se paga por voxel. El bioma, los materiales (por altura de
    /// superficie), el agua (nivel del mar) y la decoracion siguen el pipeline
    /// comun. Determinista y `Send + Sync`.
    fn generate_column_graph(&self, world_x: i32, world_z: i32) -> Column {
        let mut column = Column::empty();
        let mut tree_candidates: Vec<(usize, usize, usize)> = Vec::new();
        // Retícula gruesa: x,z cada 4 (5 muestras: 0,4,8,12,16); y cada 4 (97).
        const GX: usize = 5;
        const GZ: usize = 5;
        const GY: usize = (WORLD_HEIGHT / 4) + 1;
        let mut dens = vec![0.0f32; GY * GZ * GX];
        if let Some((graph, prog)) = &self.density_graph {
            for (yi, gy) in (0..=WORLD_HEIGHT).step_by(4).enumerate() {
                for (zi, gz) in (0..=CHUNK_SIZE).step_by(4).enumerate() {
                    for (xi, gx) in (0..=CHUNK_SIZE).step_by(4).enumerate() {
                        dens[(yi * GZ + zi) * GX + xi] = prog.eval(
                            graph,
                            (world_x + gx as i32) as f32,
                            gy as f32,
                            (world_z + gz as i32) as f32,
                        );
                    }
                }
            }
        }
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = world_x + x as i32;
                let wz = world_z + z as i32;
                let geo = self.worldgen.sample(wx as f64, wz as f64);
                let surface = self.graph_height(wx, wz);
                // Clima del grafo -> bioma por scoring (C4); el resto del pipeline
                // (materiales, decoracion) usa ese bioma.
                let (temp, rain) = self.graph_climate(wx, wz, (geo.temperature, geo.humidity));
                let elevation = (surface as f32 / 256.0).clamp(0.0, 1.0);
                let biome = biomes::select(temp, rain, elevation);
                let variant = self.surface_variant(wx, wz);
                let coastal = surface <= (SEA_LEVEL as usize) + 1;
                for y in 0..WORLD_HEIGHT {
                    if sample_density(&dens, x, y, z, GX, GZ) > 0.0 {
                        let block = if coastal {
                            coastal_block(y, surface, variant)
                        } else {
                            surface_block(y, surface, biome, variant, false)
                        };
                        column.set(x, y, z, block);
                    }
                }
                // Oceano: solo rellena por **encima del terreno**; las cuevas bajo
                // el mar quedan secas (el generador graph no tiene acuifero).
                let top = (0..WORLD_HEIGHT)
                    .rev()
                    .find(|&y| column.get(x, y, z).is_solid());
                if let Some(top) = top
                    && top < SEA_LEVEL as usize
                {
                    for y in (top + 1)..SEA_LEVEL as usize {
                        column.set(x, y, z, Block::Water);
                    }
                    column.push_water_surface(x, z, SEA_LEVEL as usize - 1);
                }
                if !coastal && (2..=13).contains(&x) && (2..=13).contains(&z) {
                    match self.decorator.decide(wx, wz, &geo, surface as i32, 0) {
                        Some(DecorationKind::Tree) => {
                            // Base = techo **solido real** de la columna (una cueva
                            // bajo la superficie no debe dejar el arbol flotando).
                            if let Some(top) = (0..WORLD_HEIGHT)
                                .rev()
                                .find(|&y| column.get(x, y, z).is_solid())
                            {
                                tree_candidates.push((x, z, top + 1));
                            }
                        }
                        Some(DecorationKind::Boulder) => {
                            // Solo sobre suelo firme (no flotando).
                            if let Some(gy) =
                                (0..surface).rev().find(|&y| column.get(x, y, z).is_solid())
                            {
                                place_boulder(&mut column, x, gy + 1, z, variant);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for (x, z, ground) in tree_candidates {
            if headroom_clear(&column, x, ground, z) {
                place_tree(&mut column, x, ground, z);
            }
        }
        column
    }

    /// Generador **legacy** (por etapas): el cuerpo historico.
    fn generate_column_legacy(&self, world_x: i32, world_z: i32) -> Column {
        let mut column = Column::empty();
        // Candidatos a arboles (pasada 1); se plantan en la pasada 2, cuando la
        // columna ya esta completa.
        let mut tree_candidates: Vec<(usize, usize, usize)> = Vec::new();

        // --- Rejilla de muestras con padding (x, z en -1..=16) ---
        const G: i32 = CHUNK_SIZE as i32;
        let gw = (G + 2) as usize; // 18
        let mut grid = Vec::with_capacity(gw * gw);
        for gz in -1..=G {
            for gx in -1..=G {
                grid.push(
                    self.worldgen
                        .sample((world_x + gx) as f64, (world_z + gz) as f64),
                );
            }
        }
        let idx = |gx: i32, gz: i32| -> usize { ((gz + 1) as usize) * gw + (gx + 1) as usize };

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = world_x + x as i32;
                let wz = world_z + z as i32;
                // --- Muestra geografica: de la rejilla (ya muestreada) ---
                // Geografia (FASE 1/2) + clima/bioma (FASE 3) del WorldGen.
                let geo = grid[idx(x as i32, z as i32)];
                let biome = geo.biome;
                let height = (geo.base_height.round() as i32).clamp(MIN_HEIGHT, MAX_HEIGHT);
                // Pendiente: maxima diferencia de altura con las 4 vecinas.
                let slope = {
                    [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .map(|&(dx, dz)| {
                            (grid[idx(x as i32 + dx, z as i32 + dz)].base_height.round() as i32)
                                .clamp(MIN_HEIGHT, MAX_HEIGHT)
                                - height
                        })
                        .map(i32::abs)
                        .max()
                        .unwrap_or(0)
                };
                let height = height as usize;
                let aquifer = self.aquifer_level(wx, wz);
                let variant = self.surface_variant(wx, wz);
                // Cerca del borde de una celda se mezclan materiales con el vecino.
                let border = geo.cell_edge < 0.35;
                let has_caves = self.has_caves(wx, wz);
                // Contexto 2D de cuevas de la columna (4 ruidos); solo se calcula
                // si la mascara abre la puerta, para no pagarlo en columnas macizas.
                let caves_ctx = has_caves.then(|| self.caves.context(wx, wz, geo.mountain_mask));
                // Superficie "costera": playa/fondo marino o **lecho de rio**
                // (arena/grava) donde el cauce es claro.
                let coastal = height <= (SEA_LEVEL as usize) + 1 || geo.river_proximity > 0.45;

                for y in 0..height {
                    // Cuevas y acuiferos antes de colocar el terreno.
                    if let Some(ctx) = &caves_ctx {
                        match self
                            .caves
                            .carve(ctx, wx, y as i32, wz, height as i32, aquifer)
                        {
                            Carve::Air => {
                                // ¿Poza de lava? Cueva con suelo firme en la
                                // banda profunda: se rellena de lava y el suelo
                                // se "hornea" a obsidiana.
                                if Self::is_lava_here(
                                    &self.caves,
                                    ctx,
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
                                    ctx,
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

                // Oceano / rio / lago: rellena de agua el aire entre la superficie
                // (ya cavada) y el nivel de agua de la muestra.
                let water_top = geo.surface_water.max(0.0).round() as usize;
                if water_top > height {
                    for y in height..water_top.min(WORLD_HEIGHT) {
                        if column.get(x, y, z) == Block::Air {
                            column.set(x, y, z, Block::Water);
                        }
                    }
                    // Pista de runtime: la superficie de agua, para despertarla al
                    // cargar la columna (que el agua generada se asiente sola).
                    let top_y = water_top.min(WORLD_HEIGHT).saturating_sub(1);
                    column.push_water_surface(x, z, top_y);
                }

                // Decoracion por reglas (FASE 7): arboles con claros y rocas. Se
                // excluyen costas y el borde del chunk (las copas no caben).
                if !coastal && (2..=13).contains(&x) && (2..=13).contains(&z) {
                    match self.decorator.decide(wx, wz, &geo, height as i32, slope) {
                        Some(DecorationKind::Tree) => {
                            // Base = techo **solido real** (una cueva bajo la
                            // superficie no debe dejar el arbol flotando).
                            if let Some(top) =
                                (0..height).rev().find(|&y| column.get(x, y, z).is_solid())
                            {
                                tree_candidates.push((x, z, top + 1));
                            }
                        }
                        // Roca solo sobre suelo firme (no flotando sobre una cueva).
                        Some(DecorationKind::Boulder)
                            if column.get(x, height.saturating_sub(1), z).is_solid() =>
                        {
                            place_boulder(&mut column, x, height, z, variant);
                        }
                        _ => {}
                    }
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

/// Muestrea la **retícula gruesa de densidad** con interpolacion trilineal.
/// `x,z` van cada 4 (`gx`/`gz` muestras), `y` cada 4. Los voxeles del borde usan
/// el ultimo tramo de la retícula (indice acotado a `len-2`).
fn sample_density(dens: &[f32], x: usize, y: usize, z: usize, gx: usize, gz: usize) -> f32 {
    let gy = WORLD_HEIGHT / 4 + 1;
    let (xf, zf, yf) = (x as f32 / 4.0, z as f32 / 4.0, y as f32 / 4.0);
    let xi = (xf as usize).min(gx - 2);
    let zi = (zf as usize).min(gz - 2);
    let yi = (yf as usize).min(gy - 2);
    let (tx, tz, ty) = (xf - xi as f32, zf - zi as f32, yf - yi as f32);
    let at = |xi: usize, yi: usize, zi: usize| dens[(yi * gz + zi) * gx + xi];
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c00 = l(at(xi, yi, zi), at(xi + 1, yi, zi), tx);
    let c10 = l(at(xi, yi + 1, zi), at(xi + 1, yi + 1, zi), tx);
    let c01 = l(at(xi, yi, zi + 1), at(xi + 1, yi, zi + 1), tx);
    let c11 = l(at(xi, yi + 1, zi + 1), at(xi + 1, yi + 1, zi + 1), tx);
    l(l(c00, c10, ty), l(c01, c11, ty), tz)
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

/// Coloca una **roca** (FASE 7) sobre la superficie: un bloque base, a veces uno
/// encima y a veces un apoyo al lado. Material segun el ruido de superficie.
fn place_boulder(column: &mut Column, x: usize, ground: usize, z: usize, variant: f64) {
    if ground >= WORLD_HEIGHT {
        return;
    }
    let block = if variant > 0.5 {
        Block::CoarseDirt
    } else {
        Block::Stone
    };
    column.set(x, ground, z, block);
    let h = hash_u32(x as i32 * 53 + 11, z as i32 * 29 + 5);
    if ground + 1 < WORLD_HEIGHT && !h.is_multiple_of(3) {
        column.set(x, ground + 1, z, block);
    }
    let (dx, dz): (i32, i32) = match (h / 3) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    };
    let (lx, lz) = (x as i32 + dx, z as i32 + dz);
    if (0..CHUNK_SIZE as i32).contains(&lx) && (0..CHUNK_SIZE as i32).contains(&lz) {
        let (lx, lz) = (lx as usize, lz as usize);
        if column.get(lx, ground, lz) == Block::Air {
            column.set(lx, ground, lz, block);
        }
    }
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
        // Con el worldgen por etapas (geografia + clima + celda + landform) son
        // ~20 por muestra, y la rejilla con padding muestrea (16+2)^2 = 324
        // celdas. El invariante es O(muestras), NO O(256 * altura): si el ruido
        // se llamara dentro del bucle `for y` serian ~18000+.
        let grid = (CHUNK_SIZE as u32 + 2) * (CHUNK_SIZE as u32 + 2);
        assert!(
            calls < grid * 24,
            "demasiadas evaluaciones 2D: {calls} (limite {})",
            grid * 24
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
                                let h = y as i32;
                                let ok =
                                    [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|&(dx, dz)| {
                                        (g.height(wx + dx, wz + dz) as i32 - h).abs() <= 1
                                    });
                                assert!(ok, "tronco en pendiente ({wx},{wz})");
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
        // El relieve continental mueve los biomas: buscamos una zona boscosa
        // (tierra, con densidad de arboles) y generamos ahi, parando al hallarla.
        'search: for cz in -12..12 {
            for cx in -12..12 {
                let (wx, wz) = (cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                if g.biome_at(wx, wz).tree_density() <= 0.0 {
                    continue;
                }
                if g.height(wx, wz) <= SEA_LEVEL as usize {
                    continue; // agua
                }
                let column = g.generate_column(wx, wz);
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
                if wood > 0 {
                    break 'search;
                }
            }
        }
        assert!(wood > 0, "no se genero ningun tronco en 24x24 chunks");
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
    fn un_cauce_de_rio_genera_agua_sobre_el_lecho() {
        // Busca un punto claramente dentro de un cauce con agua y comprueba que
        // la columna tiene bloques de agua (el rio no queda "seco").
        let g = TerrainGenerator::new(13_371);
        let mut found = false;
        'find: for x in (-3000..3000).step_by(23) {
            for z in (-3000..3000).step_by(41) {
                let s = g.sample(x, z);
                if s.river_proximity > 0.85 && s.surface_water > s.base_height + 1.0 {
                    let ox = x - x.rem_euclid(CHUNK_SIZE as i32);
                    let oz = z - z.rem_euclid(CHUNK_SIZE as i32);
                    let column = g.generate_column(ox, oz);
                    let (lx, lz) = ((x - ox) as usize, (z - oz) as usize);
                    let agua = (0..WORLD_HEIGHT).any(|y| column.get(lx, y, lz) == Block::Water);
                    assert!(agua, "cauce sin agua en ({x},{z})");
                    found = true;
                    break 'find;
                }
            }
        }
        assert!(found, "no se encontro ningun cauce claro");
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

    #[test]
    fn cave_carve_at_es_determinista_y_encuentra_cuevas() {
        let g = TerrainGenerator::new(13_371);
        let mut count = 0u32;
        for x in 0..24 {
            for z in 0..24 {
                for y in 6..60 {
                    let a = g.cave_carve_at(x, y, z);
                    assert_eq!(a, g.cave_carve_at(x, y, z), "no determinista ({x},{y},{z})");
                    if a {
                        count += 1;
                    }
                }
            }
        }
        assert!(count > 0, "cave_carve_at no encontro cuevas");
    }

    #[test]
    fn el_agua_de_superficie_se_registra_para_despertarla() {
        let g = TerrainGenerator::new(13_371);
        let mut total = 0usize;
        for cz in -6..6 {
            for cx in -6..6 {
                let column = g.generate_column(cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                total += column.water_surface().len();
            }
        }
        assert!(total > 0, "no se registro agua de superficie en la region");
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

    /// Hash FNV-1a de los ids de bloque de una columna.
    fn hash_column(column: &Column) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for y in 0..WORLD_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    h ^= column.get(x, y, z).id() as u64;
                    h = h.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        h
    }

    #[test]
    fn el_generador_graph_produce_columna_solida_y_determinista() {
        let g = TerrainGenerator::with_kind(13_371, GeneratorKind::Graph);
        assert_eq!(g.kind(), GeneratorKind::Graph);
        let a = g.generate_column(0, 0);
        let b = g.generate_column(0, 0);
        assert_eq!(hash_column(&a), hash_column(&b), "graph no determinista");
        // Hay terreno solido en algun `y`.
        let solid = (0..WORLD_HEIGHT as i32).any(|y| a.get(8, y as usize, 8).is_solid());
        assert!(solid, "la columna graph no tiene bloque solido");
    }

    #[test]
    fn graph_y_legacy_dan_terrenos_distintos() {
        let legacy = TerrainGenerator::with_kind(7, GeneratorKind::Legacy16);
        let graph = TerrainGenerator::with_kind(7, GeneratorKind::Graph);
        assert_ne!(
            hash_column(&legacy.generate_column(0, 0)),
            hash_column(&graph.generate_column(0, 0)),
            "graph y legacy deberian diferir"
        );
        // El legacy no cambia al anadir el camino graph.
        let legacy2 = TerrainGenerator::new(7);
        assert_eq!(
            hash_column(&legacy.generate_column(4, -4)),
            hash_column(&legacy2.generate_column(4, -4))
        );
    }

    #[test]
    fn el_graph_es_determinista_secuencial_vs_paralelo() {
        use crate::world::streaming::TerrainScheduler;
        use std::sync::Arc;
        let positions: Vec<super::super::save::ChunkPos> = {
            let mut v = Vec::new();
            for z in -2..=2 {
                for x in -2..=2 {
                    v.push(super::super::save::ChunkPos::new(x, z));
                }
            }
            v
        };
        let seq = TerrainGenerator::with_kind(42, GeneratorKind::Graph);
        let sequential: Vec<u64> = positions
            .iter()
            .map(|p| {
                hash_column(&seq.generate_column(p.x * CHUNK_SIZE as i32, p.z * CHUNK_SIZE as i32))
            })
            .collect();

        let shared = Arc::new(TerrainGenerator::with_kind(42, GeneratorKind::Graph));
        let mut sched = TerrainScheduler::new(shared, 4);
        for (i, &p) in positions.iter().enumerate() {
            assert!(sched.request(i as u64, p));
        }
        sched.join();
        let mut parallel = vec![0u64; positions.len()];
        while let Some(res) = sched.try_recv() {
            let idx = positions.iter().position(|p| *p == res.pos).unwrap();
            parallel[idx] = hash_column(&res.column);
        }
        assert_eq!(sequential, parallel, "graph difiere 1 hilo vs N hilos");
    }

    #[test]
    fn el_campo_de_densidad_cava_cuevas_y_es_determinista() {
        let g = TerrainGenerator::with_kind(13_371, GeneratorKind::Graph);
        // Escanea varias columnas: hay aire subterraneo (cuevas) si alguna tiene
        // aire por debajo de su bloque solido mas alto.
        let mut with_cave = 0;
        for cz in 0..3 {
            for cx in 0..3 {
                let c = g.generate_column(cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        let mut top = 0;
                        for y in 0..WORLD_HEIGHT {
                            if c.get(x, y, z).is_solid() {
                                top = y;
                            }
                        }
                        if (0..top).any(|y| c.get(x, y, z) == Block::Air) {
                            with_cave += 1;
                        }
                    }
                }
            }
        }
        assert!(with_cave > 0, "el campo de densidad deberia cavar cuevas");
        assert_eq!(
            hash_column(&g.generate_column(0, 0)),
            hash_column(&g.generate_column(0, 0)),
            "el campo de densidad no es determinista"
        );
    }

    #[test]
    fn el_clima_del_grafo_varia_y_da_varios_biomas() {
        let g = TerrainGenerator::with_kind(13_371, GeneratorKind::Graph);
        let (mut min, mut max) = (1.0f32, 0.0f32);
        let mut biomas: Vec<Biome> = Vec::new();
        for i in 0..300 {
            let x = i * 97 - 9000;
            let z = i * 53 - 5000;
            let (t, r) = g.graph_climate(x, z, (0.5, 0.5));
            min = min.min(t);
            max = max.max(t);
            let b = biomes::select(t, r, 0.3);
            if !biomas.contains(&b) {
                biomas.push(b);
            }
        }
        assert!(max - min > 0.3, "el clima del grafo deberia variar");
        assert!(biomas.len() > 1, "deberia haber varios biomas");
    }

    /// Devuelve `Some((x, z, base_y))` del primer arbol flotante (tronco sin
    /// suelo solido debajo), si lo hay.
    fn arbol_flotante(col: &Column, ox: i32, oz: i32) -> Option<(i32, i32, usize)> {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                if let Some(base) = (0..WORLD_HEIGHT).find(|&y| col.get(x, y, z) == Block::Wood) {
                    let supported = base > 0 && col.get(x, base - 1, z).is_solid();
                    if !supported {
                        return Some((ox + x as i32, oz + z as i32, base));
                    }
                }
            }
        }
        None
    }

    #[test]
    fn ningun_arbol_flota_en_cinco_semillas() {
        // C5: cero decoracion flotante en 5 semillas, en AMBOS generadores.
        for seed in [1u32, 7, 42, 13_371, 999_983] {
            for kind in [GeneratorKind::Legacy16, GeneratorKind::Graph] {
                let g = TerrainGenerator::with_kind(seed, kind);
                for cz in -3..3 {
                    for cx in -3..3 {
                        let (ox, oz) = (cx * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);
                        let col = g.generate_column(ox, oz);
                        if let Some((x, z, y)) = arbol_flotante(&col, ox, oz) {
                            panic!("arbol flotante en ({x},{y},{z}) seed {seed} {kind:?}");
                        }
                    }
                }
            }
        }
    }
}
