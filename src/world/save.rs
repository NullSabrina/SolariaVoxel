//! Guardado y versionado del mundo.
//!
//! Este modulo responde a una pregunta que en v0.3.x dolia: **"¿y mis
//! ediciones?"**. Aqui definimos:
//!
//! * `WorldHeader` — la "ficha" del mundo.
//! * `ChunkRecord` — el estado persistente de una **columna** modificada. Desde
//!   el formato v4 guarda **toda la columna** (384 bloques de alto), no solo una
//!   seccion: antes `TERRAIN_SECTION` fijo hacia que las ediciones por encima o
//!   por debajo de `y=64..80` se perdieran al reabrir.
//! * `WorldSave` — el contenedor de todo, serializado con `bincode`.
//! * [`MigrationChain`] — migradores para traer mundos de formatos antiguos.
//!
//! Versionado: `FORMAT_VERSION` es la version del **formato de archivo** (sube
//! cuando cambia el layout binario). `GENERATOR_VERSION` es la del generador de
//! terreno. Son independientes, como pide la guia.
//!
//! El guardado es **atomico**: se escribe a `world.vf.tmp`, se sincroniza, se
//! rota el anterior a `world.vf.bak` y se renombra el temporal al definitivo.
//! El archivo principal nunca queda truncado por una escritura a medias.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::Path;

use bincode::config::standard;
use bincode::{Decode, Encode};

use super::block::Block;
use super::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};

/// Bytes de una columna completa (16 x 16 x 384).
pub const COLUMN_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT;

/// Y en la que los formatos < v4 guardaban su unica seccion (era
/// `TERRAIN_SECTION = 4`). Se conserva para migrar.
pub const LEGACY_TERRAIN_Y0: u32 = 64;

/// Version actual del formato de archivo.
///
/// * v1: `ChunkRecord.blocks` eran 4096 bytes sin comprimir (una seccion).
/// * v2: `ChunkRecord.blocks` va **comprimido con LZ4**.
/// * v3: `WorldSave` guarda ademas la **posicion del jugador**.
/// * v4: `ChunkRecord` guarda **toda la columna** (`y0`, `height` y bloques de
///   las 24 secciones). Antes solo se persistia `y=64..80`.
/// * v5: `ChunkRecord` guarda ademas los **niveles de flujo del agua**
///   (`fluid`), para que el agua que fluye no vuelva a fuente al recargar. Un
///   registro v4 migra con `fluid` vacio = todo `Water` es fuente (comportamiento
///   anterior, sin perdida de datos).
/// * v6: `WorldSave` guarda la **hotbar** como stacks.
/// * v7: `ChunkRecord` guarda un **checksum** (FNV-1a 64) de los bloques y del
///   fluido **descomprimidos**. LZ4 no trae checksum: una corrupcion que
///   conserva el tamano pasaba inadvertida. Los mundos v6 migran calculando el
///   checksum una vez. Ademas el mapa de chunks se serializa ordenado
///   (`BTreeMap`), asi dos guardados del mismo estado dan bytes identicos.
pub const FORMAT_VERSION: u32 = 7;

/// Version actual del generador de terreno.
///
/// * v1: solo colinas (Perlin).
/// * v2: biomas (Worley).
/// * v3: altura/bioma por bloque.
/// * v4: cuevas con Perlin 3D.
/// * v5: oceanos/lagos + playas.
/// * v6: vegetacion (arboles).
/// * v7: clima/biomas avanzados, relieve por bioma, cuevas 3D y acuiferos.
/// * v8: pozas de lava + bloques `Lava`/`Obsidian`.
/// * v9: relieve **continental** por etapas (`world::worldgen`): continentalness
///   con domain warping, red celular, costa de ancho variable, cordilleras por
///   mascara de rango + cresta y valles. El bioma sigue derivandose del clima.
/// * v10: **bioma por region celular** (FASE 3): definiciones con scoring,
///   mezcla con el centro de la celda y lapse de altitud. Los materiales y la
///   vegetacion cambian (el relieve es el mismo que v9).
/// * v11: **hidrologia** (FASE 5): rios serpenteantes (cresta con domain warp)
///   que cavan cauces y se llenan de agua, mas lagos en cuencas humedas.
/// * v12: **cuevas jerarquicas** (FASE 6): tubos spaghetti, tuneles regionales,
///   camaras `cheese`, pozos verticales, canones, pilares de preservacion y
///   entradas que rompen la corteza; densidad por profundidad y por montana.
/// * v13: **decoracion por reglas** (FASE 7): arboles con claros (clusters) y
///   rocas en laderas altas, con reglas de bioma/altura/humedad/pendiente/rio y
///   rejilla de muestras con padding.
/// * v14: **landforms** (FASE 4): perfiles de relieve por region (mesetas de
///   cima plana, terrazas geologicas y acantilados) aplicados a la altura.
/// * v15: **tooling** (FASE 9) + calibracion de landforms: previews de cuevas
///   (slices), seed gallery y metricas; umbrales de landform ajustados para que
///   mesetas/terrazas/acantilados aparezcan con una proporcion util.
/// * v16: **tuning del clima y la costa**: contraste climatico (mas biomas
///   frios/aridos) y perfil continental mas empinado cerca de la costa (playas y
///   plataformas mas estrechas).
/// * v17: **decoracion sin flotantes** (Parte C, C5): los arboles se apoyan en el
///   techo **solido real** de la columna (una cueva bajo la superficie ya no deja
///   el tronco flotando), en ambos generadores.
/// * v18: **bioma unico y ecotonos** (MEGA PROMPT 1, Fase B): un solo `biome_at`
///   (WorldGen) para los tres caminos y transiciones de material por ruido.
/// * v19: **superficie por pendiente** (Fase C): roca en laderas, sedimento en
///   valles y pendiente real en el camino graph.
/// * v20: **arboles procedimentales** (Fase D): 4 especies, copa procedural y
///   colocacion con margen entre chunks.
/// * v21: **generador Larion** (MEGA PROMPT 4): tercer camino de generacion
///   (continentalidad con domain warping horizontal, erosion que decide la forma,
///   crestas multifractales, densidad 3D en banda, clima en bandas, rios sinuosos
///   y biomas multi-parametricos). `Legacy16` y `Graph` **no cambian**.
/// * v22: **P0.1**: hidrologia global opcional (`generator_config = 2`). El
///   pipeline historico (`generator_config = 1`) conserva byte a byte el terreno
///   de v21. La version del pipeline vive en `level.json`
///   (`WorldMeta::generator_config`), no en este header bincode, para no romper
///   `FORMAT_VERSION = 7`. `generator_config = 3` (v0.52) anade arboles
///   organicos, cuevas worm y Larion SIMD sin tocar 1 ni 2.
pub const GENERATOR_VERSION: u32 = 22;

/// El "magic number" que identifica un archivo de mundo de Solaria.
pub const MAGIC: [u8; 4] = *b"VFWD";

/// Identificador de la version del motor que guardo el mundo.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Posicion de un chunk en el mundo, en coordenadas de chunk (no de bloque).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Ficha del mundo.
///
/// La version del **pipeline** Larion (`generator_config = 1/2/3/4`) no vive aqui:
/// esta en `WorldMeta` (`level.json`) para no cambiar el layout bincode de
/// `FORMAT_VERSION = 7`. `generator_version` sigue siendo informativo.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct WorldHeader {
    pub magic: [u8; 4],
    pub format_version: u32,
    pub generator_version: u32,
    pub seed: u32,
    pub engine_version: String,
    pub created_at: u64,
}

impl Default for WorldHeader {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl WorldHeader {
    pub fn new(seed: u32, created_at: u64) -> Self {
        Self {
            magic: MAGIC,
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            seed,
            engine_version: ENGINE_VERSION.to_string(),
            created_at,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
    }
}

/// Estado persistente de una **columna** modificada.
///
/// `blocks` guarda `height` capas de `16x16` empezando en `y0`, en orden
/// `(y, z, x)`. En el formato v4 `y0 = 0` y `height = WORLD_HEIGHT` (columna
/// completa); los registros migrados de v3 conservan `y0 = 64` y `height = 16`.
///
/// `fluid` (desde v5) guarda el **nivel de flujo** del agua con el mismo orden e
/// indice que `blocks`: `0` = sin flujo (una celda `Water` sin flujo es fuente),
/// `1..=MAX_LEVEL` = agua que fluye. Va **vacio** si la columna no tiene agua que
/// fluya (lo normal: un oceano es todo fuentes), asi no se paga nada por ello.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ChunkRecord {
    pub format_version: u32,
    pub generator_version: u32,
    pub compressed: bool,
    /// Primera capa Y almacenada.
    pub y0: u32,
    /// Numero de capas Y almacenadas.
    pub height: u32,
    /// Bloques (comprimidos con LZ4 si `compressed`).
    pub blocks: Vec<u8>,
    /// Niveles de flujo del agua (comprimidos con LZ4 si `compressed`).
    pub fluid: Vec<u8>,
    /// FNV-1a 64 de los bloques **descomprimidos** (desde v7). `0` = sin
    /// verificar (registros antiguos: la migracion lo calcula).
    pub checksum: u64,
    /// FNV-1a 64 del fluido **descomprimido** (desde v7). `0` si no hay fluido.
    pub fluid_checksum: u64,
}

/// Layout de `ChunkRecord` en el formato **v6**: igual que el actual pero **sin
/// checksums**. Bincode es posicional: leer un v6 con el layout v7 tomaria los
/// bytes siguientes como checksum y desalinearia el archivo.
#[derive(Clone, Debug, Encode, Decode)]
struct ChunkRecordV6 {
    format_version: u32,
    generator_version: u32,
    compressed: bool,
    y0: u32,
    height: u32,
    blocks: Vec<u8>,
    fluid: Vec<u8>,
}

/// Layout de `ChunkRecord` en el formato **v1**: una seccion de 4096 bytes
/// **sin comprimir** y sin el flag `compressed`. Bincode es posicional: leer un
/// v1 con el layout v2 (que espera `compressed`) desalinearia todo el archivo.
#[derive(Clone, Debug, Encode, Decode)]
struct ChunkRecordV1 {
    format_version: u32,
    generator_version: u32,
    blocks: Vec<u8>,
}

/// Layout de `ChunkRecord` en los formatos v2/v3 (sin `y0`/`height`). Bincode es
/// posicional: sin este espejo no se puede deserializar un archivo viejo.
#[derive(Clone, Debug, Encode, Decode)]
struct ChunkRecordV3 {
    format_version: u32,
    generator_version: u32,
    compressed: bool,
    blocks: Vec<u8>,
}

/// Layout de `ChunkRecord` en el formato **v4** (columna completa, pero sin
/// `fluid`). Bincode es posicional: decodificar un v4 con el layout v5 leería los
/// bytes de `fluid` a continuacion y corromperia el resto del archivo.
#[derive(Clone, Debug, Encode, Decode)]
struct ChunkRecordV4 {
    format_version: u32,
    generator_version: u32,
    compressed: bool,
    y0: u32,
    height: u32,
    blocks: Vec<u8>,
}

impl ChunkRecord {
    /// Construye el registro a partir de una columna cargada (terreno generado
    /// mas ediciones). Guarda las 24 secciones (bloques y niveles de flujo) y
    /// comprime con LZ4. El campo `fluid` queda **vacio** si la columna no tiene
    /// agua que fluya (el caso normal: un oceano entero son fuentes).
    pub fn from_column(column: &Column) -> Self {
        // P2-2: `has_flow_storage` se consulta una vez (no por celda) y los
        // buffers se reservan de antemano.
        let has_flow = column.has_flow_storage();
        let mut raw = Vec::with_capacity(COLUMN_VOLUME);
        let mut fluid = if has_flow {
            Vec::with_capacity(COLUMN_VOLUME)
        } else {
            Vec::new()
        };
        for y in 0..WORLD_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    raw.push(column.get(x, y, z).id());
                    if has_flow {
                        fluid.push(column.flow_at(x, y, z));
                    }
                }
            }
        }
        let checksum = fnv1a64(&raw);
        let fluid_checksum = if fluid.is_empty() { 0 } else { fnv1a64(&fluid) };
        Self {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: lz4_flex::compress_prepend_size(&raw),
            // Vacio se deja vacio: comprimir un array de 0 bytes daria un
            // payload no vacio que pareceria "hay fluido" (y `is_corrupt` lo
            // confundiria con un chunk truncado).
            fluid: if fluid.is_empty() {
                Vec::new()
            } else {
                lz4_flex::compress_prepend_size(&fluid)
            },
            checksum,
            fluid_checksum,
        }
    }

    /// Bytes crudos esperados segun `height` (0 si el registro esta corrupto).
    pub fn raw_len(&self) -> usize {
        self.height as usize * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Decodifica y valida los bloques: tamano esperado **y** checksum. `None`
    /// si el payload esta corrupto.
    fn decode_blocks(&self) -> Option<Vec<u8>> {
        let expected = self.raw_len();
        let raw = if self.compressed {
            // P2-4: se acota la longitud declarada **antes** de asignar.
            lz4_decompress_checked(&self.blocks, expected)?
        } else {
            if self.blocks.len() != expected {
                return None;
            }
            self.blocks.clone()
        };
        if raw.len() != expected {
            return None;
        }
        // `0` = registro antiguo sin checksum; el resto debe coincidir.
        if self.checksum != 0 && fnv1a64(&raw) != self.checksum {
            return None;
        }
        Some(raw)
    }

    /// Decodifica y valida los niveles de flujo. `None` si el payload esta
    /// corrupto; vacio si la columna no guardo fluidos (oceano: todo fuentes).
    fn decode_fluid(&self) -> Option<Vec<u8>> {
        if self.fluid.is_empty() {
            // Sin payload no puede haber checksum: un valor distinto de 0 es
            // metadata corrupta.
            return (self.fluid_checksum == 0).then(Vec::new);
        }
        let expected = self.raw_len();
        let raw = if self.compressed {
            lz4_decompress_checked(&self.fluid, expected)?
        } else {
            if self.fluid.len() != expected {
                return None;
            }
            self.fluid.clone()
        };
        if raw.len() != expected {
            return None;
        }
        if self.fluid_checksum != 0 && fnv1a64(&raw) != self.fluid_checksum {
            return None;
        }
        Some(raw)
    }

    /// Bloques sin comprimir. `Vec::new()` si el payload esta corrupto (lo
    /// detecta [`ChunkRecord::is_corrupt`]).
    pub fn decompressed_blocks(&self) -> Vec<u8> {
        self.decode_blocks().unwrap_or_default()
    }

    /// Niveles de flujo sin comprimir. Vacio si la columna no guardo fluidos
    /// (oceano: todo fuentes) o si el payload esta corrupto.
    pub fn decompressed_fluid(&self) -> Vec<u8> {
        self.decode_fluid().unwrap_or_default()
    }

    /// ¿El payload no decodifica al tamano esperado o no cuadra su checksum?
    /// (chunk corrupto). El campo `fluid` solo se valida si el registro lo trae;
    /// vacio es valido.
    pub fn is_corrupt(&self) -> bool {
        // Rango vertical valido: si `y0 + height` se sale del mundo, `apply_record`
        // recortaria y **descartaria ediciones en silencio**; mejor rechazarlo.
        let (y0, h) = (self.y0 as usize, self.height as usize);
        if h == 0 || y0 >= WORLD_HEIGHT || y0 + h > WORLD_HEIGHT {
            return true;
        }
        if self.decode_blocks().is_none() {
            return true;
        }
        self.decode_fluid().is_none()
    }

    /// Recalcula los checksums a partir del payload actual (si decodifica).
    fn recompute_checksums(&mut self) {
        if let Some(blocks) = self.decode_blocks() {
            self.checksum = fnv1a64(&blocks);
        }
        self.fluid_checksum = match self.decode_fluid() {
            Some(fluid) if !fluid.is_empty() => fnv1a64(&fluid),
            _ => 0,
        };
    }

    /// Ratio de compresion (`raw / compressed`). 1.0 = no comprime.
    pub fn compression_ratio(&self) -> f32 {
        if self.compressed && !self.blocks.is_empty() {
            self.raw_len() as f32 / self.blocks.len() as f32
        } else {
            1.0
        }
    }

    /// Primer id de bloque **desconocido** en el registro, si lo hay.
    ///
    /// Cargar un id desconocido como aire destruiria datos de una version
    /// futura; preferimos rechazar el mundo con un error claro.
    pub fn first_unknown_id(&self) -> Option<u8> {
        self.decompressed_blocks()
            .into_iter()
            .find(|&id| !Block::is_known_id(id))
    }

    /// Migra un registro v1 (sin comprimir) al v2 (comprimido).
    pub fn migrate_to_v2(&mut self) {
        if !self.compressed {
            self.blocks = lz4_flex::compress_prepend_size(&self.blocks);
            if !self.fluid.is_empty() {
                self.fluid = lz4_flex::compress_prepend_size(&self.fluid);
            }
            self.compressed = true;
        }
        self.recompute_checksums();
        self.format_version = 2;
    }
}

/// Todo lo que se guarda de un mundo.
#[derive(Clone, Debug, Encode, Decode)]
pub struct WorldSave {
    pub header: WorldHeader,
    /// Columnas modificadas, indexadas por su posicion. `BTreeMap` (v7) para que
    /// el archivo sea **determinista**: el mismo estado produce los mismos bytes.
    pub chunks: BTreeMap<ChunkPos, ChunkRecord>,
    /// Posicion del jugador (guardado completo). Desde el formato v3.
    pub player_pos: [f32; 3],
    /// Hotbar como stacks `(id_de_bloque, cantidad)`. Desde el formato v6.
    pub hotbar: Vec<(u8, u8)>,
}

/// Regresion del jugador por defecto (si un mundo viejo no la trae).
pub const DEFAULT_PLAYER_POS: [f32; 3] = [8.0, 76.0, 20.0];

/// Hotbar por defecto: los primeros `HOTBAR_SLOTS` bloques colocables, a pila
/// llena (creativo). Es lo que reciben los mundos viejos (v5) al migrar.
pub fn default_hotbar() -> Vec<(u8, u8)> {
    super::registry::BlockRegistry::items()
        .iter()
        .take(9)
        .map(|b| (b.id(), crate::world::ItemStack::MAX))
        .collect()
}

/// Espejo del `WorldSave` **v5**: sin hotbar y con registros sin checksum.
#[derive(Encode, Decode)]
struct WorldSaveV5 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV6>,
    player_pos: [f32; 3],
}

/// Espejo del `WorldSave` **v6**: con hotbar, sin checksums.
#[derive(Encode, Decode)]
struct WorldSaveV6 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV6>,
    player_pos: [f32; 3],
    hotbar: Vec<(u8, u8)>,
}

/// Espejo del `WorldSave` **v3**: mismo `player_pos` pero con los `ChunkRecord`
/// antiguos (una sola seccion, sin `y0`/`height`).
#[derive(Encode, Decode)]
struct WorldSaveV3 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV3>,
    player_pos: [f32; 3],
}

/// Espejo del `WorldSave` **v4** (columna completa sin fluido).
#[derive(Encode, Decode)]
struct WorldSaveV4 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV4>,
    player_pos: [f32; 3],
}

/// Espejo del `WorldSave` **v2** (sin `player_pos`).
#[derive(Encode, Decode)]
struct WorldSaveV2 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV3>,
}

/// Espejo del `WorldSave` **v1**: sin `player_pos` y con bloques sin comprimir
/// (sin el flag `compressed`).
#[derive(Encode, Decode)]
struct WorldSaveV1 {
    header: WorldHeader,
    chunks: HashMap<ChunkPos, ChunkRecordV1>,
}

/// Convierte un registro antiguo (una seccion) al formato de columna completo.
/// La seccion vivia en `y = LEGACY_TERRAIN_Y0`. Sin datos de fluido (vacio).
fn upgrade_v3_record(old: ChunkRecordV3) -> ChunkRecord {
    ChunkRecord {
        format_version: 3,
        generator_version: old.generator_version,
        compressed: old.compressed,
        y0: LEGACY_TERRAIN_Y0,
        height: CHUNK_SIZE as u32,
        blocks: old.blocks,
        fluid: Vec::new(),
        checksum: 0,
        fluid_checksum: 0,
    }
}

/// Convierte un registro v1 (bloques sin comprimir, una seccion en `y=64`) al
/// tipo actual. `compressed = false` deja que el migrador `V1ToV2` comprima.
fn upgrade_v1_record(old: ChunkRecordV1) -> ChunkRecord {
    ChunkRecord {
        format_version: 1,
        generator_version: old.generator_version,
        compressed: false,
        y0: LEGACY_TERRAIN_Y0,
        height: CHUNK_SIZE as u32,
        blocks: old.blocks,
        fluid: Vec::new(),
        checksum: 0,
        fluid_checksum: 0,
    }
}

/// Convierte un registro v4 (columna completa, sin fluido) al v5. `fluid` vacio
/// significa "todo `Water` es fuente", que es exactamente el comportamiento de
/// v4: no se pierde ni se inventa agua.
fn upgrade_v4_record(old: ChunkRecordV4) -> ChunkRecord {
    ChunkRecord {
        format_version: 4,
        generator_version: old.generator_version,
        compressed: old.compressed,
        y0: old.y0,
        height: old.height,
        blocks: old.blocks,
        fluid: Vec::new(),
        checksum: 0,
        fluid_checksum: 0,
    }
}

/// Convierte un registro v6 (sin checksum) al v7. El checksum se calcula una
/// sola vez descomprimiendo el payload; si no decodifica, queda `0` y la
/// validacion de carga lo rechazara como chunk corrupto.
fn upgrade_v6_record(old: ChunkRecordV6) -> ChunkRecord {
    let mut record = ChunkRecord {
        format_version: 6,
        generator_version: old.generator_version,
        compressed: old.compressed,
        y0: old.y0,
        height: old.height,
        blocks: old.blocks,
        fluid: old.fluid,
        checksum: 0,
        fluid_checksum: 0,
    };
    record.recompute_checksums();
    record
}

impl WorldSave {
    pub fn new(seed: u32, created_at: u64) -> Self {
        Self {
            header: WorldHeader::new(seed, created_at),
            chunks: BTreeMap::new(),
            player_pos: DEFAULT_PLAYER_POS,
            hotbar: default_hotbar(),
        }
    }

    pub fn set_chunk(&mut self, pos: ChunkPos, record: ChunkRecord) {
        self.chunks.insert(pos, record);
    }

    /// Escribe el mundo a disco de forma **atomica**.
    ///
    /// `path.tmp` se escribe y sincroniza; el archivo anterior se rota a
    /// `path.bak`; y `path.tmp` se renombra a `path`. Nunca queda un `path`
    /// truncado a medias.
    pub fn save_to(&self, path: &Path) -> Result<(), SaveError> {
        let bytes = bincode::encode_to_vec(self, standard())?;
        let tmp = with_suffix(path, ".tmp");
        let bak = with_suffix(path, ".bak");
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            // Asegura que los datos llegan al disco antes de rotar el archivo.
            file.sync_all()?;
        }
        if path.exists() {
            let _ = std::fs::remove_file(&bak);
            std::fs::rename(path, &bak)?;
        }
        std::fs::rename(&tmp, path)?;
        // P2-7: durabilidad de los `rename` (best-effort; en Windows no se puede
        // abrir un directorio con `std`, asi que solo aplica en Unix).
        sync_dir(path);
        Ok(())
    }

    /// Lee y deserializa un mundo de disco (sin migrar). Elige el layout de los
    /// `ChunkRecord` segun la `format_version` de la cabecera: bincode es
    /// posicional, asi que decodificar un archivo v3 con el layout v4 daria
    /// bytes mal interpretados.
    pub fn load_from(path: &Path) -> Result<Self, SaveError> {
        let bytes = std::fs::read(path)?;
        // Espiamos solo la cabecera (esta al principio) para conocer la version.
        // P2-5: una cabecera ilegible es su propio error, no `NoMigration { 0 }`.
        let (header, _) = bincode::decode_from_slice::<WorldHeader, _>(&bytes, standard())
            .map_err(|_| SaveError::CorruptHeader)?;
        let version = header.format_version;
        if version > FORMAT_VERSION {
            return Err(SaveError::TooNew {
                format: version,
                supported: FORMAT_VERSION,
            });
        }
        // P2-6: exige que el decodificador consuma **todo** el archivo (detecta
        // basura al final / truncamientos que la parte util ocultaria).
        macro_rules! decode_exact {
            ($ty:ty) => {{
                let (value, used) = bincode::decode_from_slice::<$ty, _>(&bytes, standard())?;
                if used != bytes.len() {
                    return Err(SaveError::TrailingBytes {
                        used,
                        len: bytes.len(),
                    });
                }
                value
            }};
        }
        match version {
            7 => Ok(decode_exact!(WorldSave)),
            6 => {
                let v6 = decode_exact!(WorldSaveV6);
                let mut header = v6.header;
                header.format_version = 6;
                Ok(WorldSave {
                    header,
                    chunks: v6
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v6_record(r)))
                        .collect(),
                    player_pos: v6.player_pos,
                    hotbar: v6.hotbar,
                })
            }
            5 => {
                // v5 no guardaba hotbar: se le da la por defecto (pila llena), sin
                // perder nada del mundo (bloques/posicion se conservan).
                let v5 = decode_exact!(WorldSaveV5);
                let mut header = v5.header;
                header.format_version = 5;
                Ok(WorldSave {
                    header,
                    chunks: v5
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v6_record(r)))
                        .collect(),
                    player_pos: v5.player_pos,
                    hotbar: default_hotbar(),
                })
            }
            4 => {
                let v4 = decode_exact!(WorldSaveV4);
                let mut header = v4.header;
                header.format_version = 4;
                Ok(WorldSave {
                    header,
                    chunks: v4
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v4_record(r)))
                        .collect(),
                    player_pos: v4.player_pos,
                    hotbar: default_hotbar(),
                })
            }
            3 => {
                let v3 = decode_exact!(WorldSaveV3);
                let mut header = v3.header;
                header.format_version = 3;
                Ok(WorldSave {
                    header,
                    chunks: v3
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v3_record(r)))
                        .collect(),
                    player_pos: v3.player_pos,
                    hotbar: default_hotbar(),
                })
            }
            2 => {
                let v2 = decode_exact!(WorldSaveV2);
                let mut header = v2.header;
                header.format_version = 2;
                Ok(WorldSave {
                    header,
                    chunks: v2
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v3_record(r)))
                        .collect(),
                    player_pos: DEFAULT_PLAYER_POS,
                    hotbar: default_hotbar(),
                })
            }
            // v1: bloques sin comprimir, sin flag `compressed` ni `player_pos`.
            1 => {
                let v1 = decode_exact!(WorldSaveV1);
                let mut header = v1.header;
                header.format_version = 1;
                Ok(WorldSave {
                    header,
                    chunks: v1
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v1_record(r)))
                        .collect(),
                    player_pos: DEFAULT_PLAYER_POS,
                    hotbar: default_hotbar(),
                })
            }
            from => Err(SaveError::NoMigration {
                from,
                to: FORMAT_VERSION,
            }),
        }
    }

    /// Lee **solo la cabecera** (sin descomprimir el mundo). Util para importar
    /// un mundo legado sin pagar la carga completa (P2-14).
    pub fn peek_header(path: &Path) -> Result<WorldHeader, SaveError> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = [0u8; 512];
        let mut len = 0usize;
        while len < buf.len() {
            let n = file.read(&mut buf[len..])?;
            if n == 0 {
                break;
            }
            len += n;
        }
        let (header, _) = bincode::decode_from_slice::<WorldHeader, _>(&buf[..len], standard())
            .map_err(|_| SaveError::CorruptHeader)?;
        if !header.is_valid() {
            return Err(SaveError::BadMagic(header.magic));
        }
        Ok(header)
    }
}

/// `path` con un sufijo extra (`world.vf` -> `world.vf.tmp`).
fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(suffix);
    std::path::PathBuf::from(os)
}

/// Descomprime un payload LZ4 con prefijo de tamano **acotado**: si el prefijo
/// no coincide con `expected`, no se reserva ni se descomprime nada (P2-4).
fn lz4_decompress_checked(payload: &[u8], expected: usize) -> Option<Vec<u8>> {
    let prefix: [u8; 4] = payload.get(..4)?.try_into().ok()?;
    if u32::from_le_bytes(prefix) as usize != expected {
        return None;
    }
    let out = lz4_flex::decompress_size_prepended(payload).ok()?;
    (out.len() == expected).then_some(out)
}

/// FNV-1a de 64 bits: checksum estable, rapido y sin dependencias. No es
/// criptografico; basta para detectar corrupcion de datos.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `fsync` del directorio que contiene `path` (durabilidad de los `rename`).
/// Best-effort y solo en Unix: Windows no permite abrir un directorio con `std`.
fn sync_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(dir) = path.parent()
            && let Ok(file) = std::fs::File::open(dir)
        {
            let _ = file.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Resultado de cargar un mundo de disco, distinguiendo "no existe" (mundo
/// nuevo legitimo) de "existe pero no se puede leer" (no se debe sobrescribir).
#[derive(Debug)]
pub enum WorldLoad {
    /// No hay archivo principal ni backup: mundo nuevo.
    Missing,
    /// Se cargo el archivo principal.
    Loaded(WorldSave),
    /// El principal fallo y se recupero el `.bak`. El principal corrupto queda
    /// en `world.vf.corrupt` y el principal ya contiene la copia recuperada.
    Recovered(WorldSave),
    /// El archivo existe pero no se puede leer y no hay backup valido. **No** se
    /// debe tratar como mundo vacio ni sobrescribir.
    Unreadable(SaveError),
}

/// Carga un mundo aplicando la politica de recuperacion (P0-1 + P1-5):
///
/// 1. Si el principal no existe pero hay `.bak`, se restaura desde el backup.
/// 2. Si el principal carga, se usa.
/// 3. Si el principal existe pero falla (decode/corrupto/ids/migracion), se
///    intenta el `.bak`. Si vale, se **cuarentena** el principal danado
///    (`world.vf.corrupt`) y se reescribe el principal con la copia recuperada,
///    conservando el `.bak`.
/// 4. Un formato **futuro** (`TooNew`) nunca se degrada a un backup: se
///    devuelve `Unreadable` para no perder datos de una version mas nueva.
pub fn load_world(path: &Path) -> WorldLoad {
    let bak = with_suffix(path, ".bak");
    if !path.exists() {
        if bak.exists() {
            // Hay backup: el mundo existia. Si el backup tampoco se puede leer,
            // **no** es un mundo nuevo (no se debe sobrescribir).
            match load_and_migrate(&bak) {
                Ok(save) => {
                    if save_to_keeping_backup(&save, path).is_ok() {
                        return WorldLoad::Recovered(save);
                    }
                    return WorldLoad::Unreadable(SaveError::Io(std::io::Error::other(
                        "no se pudo restaurar el backup",
                    )));
                }
                Err(err) => return WorldLoad::Unreadable(err),
            }
        }
        return WorldLoad::Missing;
    }
    match load_and_migrate(path) {
        Ok(save) => WorldLoad::Loaded(save),
        Err(err) => {
            if matches!(err, SaveError::TooNew { .. }) {
                return WorldLoad::Unreadable(err);
            }
            if let Ok(save) = load_and_migrate(&bak) {
                // Cuarentena del principal danado + reescritura del recuperado.
                let corrupt = with_suffix(path, ".corrupt");
                let _ = std::fs::remove_file(&corrupt);
                if std::fs::rename(path, &corrupt).is_ok()
                    && save_to_keeping_backup(&save, path).is_ok()
                {
                    return WorldLoad::Recovered(save);
                }
            }
            WorldLoad::Unreadable(err)
        }
    }
}

/// Escribe `save` en `path` **sin rotar** el `.bak` existente (no lo pisa con el
/// archivo danado que acabamos de recuperar).
fn save_to_keeping_backup(save: &WorldSave, path: &Path) -> Result<(), SaveError> {
    let bytes = bincode::encode_to_vec(save, standard())?;
    let tmp = with_suffix(path, ".tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    sync_dir(path);
    Ok(())
}

/// Errores de guardado/carga.
#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    Decode(bincode::error::DecodeError),
    Encode(bincode::error::EncodeError),
    /// La cabecera no se puede decodificar (archivo truncado o que no es un
    /// mundo). Antes se enmascaraba como `NoMigration { from: 0 }` (P2-5).
    CorruptHeader,
    /// El archivo decodifica pero quedan bytes sin consumir al final (P2-6).
    TrailingBytes {
        used: usize,
        len: usize,
    },
    BadMagic([u8; 4]),
    TooNew {
        format: u32,
        supported: u32,
    },
    NoMigration {
        from: u32,
        to: u32,
    },
    /// Un registro guarda un id de bloque que este motor no conoce.
    UnknownBlock {
        id: u8,
    },
    /// Un chunk no decodifica al tamano esperado (payload corrupto).
    CorruptChunk {
        x: i32,
        z: i32,
    },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "error de E/S: {e}"),
            SaveError::Decode(e) => write!(f, "no se pudo decodificar el mundo: {e}"),
            SaveError::Encode(e) => write!(f, "no se pudo codificar el mundo: {e}"),
            SaveError::CorruptHeader => {
                write!(
                    f,
                    "la cabecera del mundo esta corrupta o el archivo no es un mundo"
                )
            }
            SaveError::TrailingBytes { used, len } => write!(
                f,
                "el archivo tiene {len} bytes pero solo {used} se decodifican (basura al final)"
            ),
            SaveError::BadMagic(m) => {
                write!(f, "el archivo no es un mundo de Solaria (magic {m:?})")
            }
            SaveError::TooNew { format, supported } => write!(
                f,
                "el mundo usa formato v{format}, mas nuevo que el soportado v{supported}"
            ),
            SaveError::NoMigration { from, to } => {
                write!(f, "no hay migrador de v{from} a v{to}")
            }
            SaveError::UnknownBlock { id } => {
                write!(f, "el mundo contiene un bloque desconocido (id {id})")
            }
            SaveError::CorruptChunk { x, z } => {
                write!(f, "chunk corrupto en ({x}, {z}): no decodifica")
            }
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

impl From<bincode::error::DecodeError> for SaveError {
    fn from(e: bincode::error::DecodeError) -> Self {
        SaveError::Decode(e)
    }
}

impl From<bincode::error::EncodeError> for SaveError {
    fn from(e: bincode::error::EncodeError) -> Self {
        SaveError::Encode(e)
    }
}

/// Un migrador trae datos de una version de formato a la siguiente.
#[allow(clippy::wrong_self_convention)]
pub trait WorldMigrator {
    fn from_version(&self) -> u32;
    fn to_version(&self) -> u32;
    /// Consume el registro (P2-8: evita clonar ~98 KB por paso de migracion).
    fn migrate_chunk(&self, chunk: ChunkRecord) -> ChunkRecord {
        chunk
    }
}

/// Migrador v1 -> v2: comprime con LZ4 los bloques sin comprimir.
pub struct V1ToV2;

impl WorldMigrator for V1ToV2 {
    fn from_version(&self) -> u32 {
        1
    }
    fn to_version(&self) -> u32 {
        2
    }
    fn migrate_chunk(&self, mut chunk: ChunkRecord) -> ChunkRecord {
        chunk.compressed = false;
        chunk.migrate_to_v2();
        chunk
    }
}

/// Migrador v2 -> v3: solo sube la version (la posicion es del `WorldSave`).
pub struct V2ToV3;

impl WorldMigrator for V2ToV3 {
    fn from_version(&self) -> u32 {
        2
    }
    fn to_version(&self) -> u32 {
        3
    }
}

/// Migrador v3 -> v4: los registros de una seccion pasan a `y0`/`height`
/// explicitos (la seccion vivia en `y=64`).
pub struct V3ToV4;

impl WorldMigrator for V3ToV4 {
    fn from_version(&self) -> u32 {
        3
    }
    fn to_version(&self) -> u32 {
        4
    }
    fn migrate_chunk(&self, mut r: ChunkRecord) -> ChunkRecord {
        if r.height == 0 {
            r.y0 = LEGACY_TERRAIN_Y0;
            r.height = CHUNK_SIZE as u32;
        }
        r.format_version = 4;
        r
    }
}

/// Migrador v4 -> v5: anade el campo `fluid` vacio. Los mundos v4 no guardaban
/// niveles de flujo, asi que todo `Water` se interpreta como **fuente**, que es
/// exactamente como se comportaban (cero perdida ni invencion de agua).
pub struct V4ToV5;

impl WorldMigrator for V4ToV5 {
    fn from_version(&self) -> u32 {
        4
    }
    fn to_version(&self) -> u32 {
        5
    }
    fn migrate_chunk(&self, mut r: ChunkRecord) -> ChunkRecord {
        r.fluid = Vec::new();
        r.fluid_checksum = 0;
        r.format_version = 5;
        r
    }
}

/// Migrador v5 -> v6: anade la **hotbar** (stacks). Los mundos v5 no la
/// guardaban; `load_from` les pone la por defecto (pila llena) sin perder nada.
pub struct V5ToV6;

impl WorldMigrator for V5ToV6 {
    fn from_version(&self) -> u32 {
        5
    }
    fn to_version(&self) -> u32 {
        6
    }
}

/// Migrador v6 -> v7: anade los **checksums** de bloques y fluido. Se calculan
/// una vez a partir del payload descomprimido.
pub struct V6ToV7;

impl WorldMigrator for V6ToV7 {
    fn from_version(&self) -> u32 {
        6
    }
    fn to_version(&self) -> u32 {
        7
    }
    fn migrate_chunk(&self, mut r: ChunkRecord) -> ChunkRecord {
        // `load_from` ya calculo el checksum al adaptar el layout v6; no se
        // descomprime dos veces.
        if r.checksum == 0 {
            r.recompute_checksums();
        }
        r.format_version = 7;
        r
    }
}

/// Cadena de migradores.
#[derive(Default)]
pub struct MigrationChain {
    migrators: Vec<Box<dyn WorldMigrator>>,
}

impl MigrationChain {
    pub fn with_builtins() -> Self {
        Self {
            migrators: vec![
                Box::new(V1ToV2),
                Box::new(V2ToV3),
                Box::new(V3ToV4),
                Box::new(V4ToV5),
                Box::new(V5ToV6),
                Box::new(V6ToV7),
            ],
        }
    }

    pub fn push(&mut self, m: Box<dyn WorldMigrator>) {
        self.migrators.push(m);
    }

    /// Migra un `WorldSave` hasta `FORMAT_VERSION`.
    pub fn migrate(&self, mut save: WorldSave) -> Result<WorldSave, SaveError> {
        let mut from = save.header.format_version;
        if from > FORMAT_VERSION {
            return Err(SaveError::TooNew {
                format: from,
                supported: FORMAT_VERSION,
            });
        }
        while from < FORMAT_VERSION {
            let Some(migrator) = self
                .migrators
                .iter()
                .find(|m| m.from_version() == from && m.to_version() <= FORMAT_VERSION)
            else {
                return Err(SaveError::NoMigration {
                    from,
                    to: FORMAT_VERSION,
                });
            };
            let to = migrator.to_version();
            // Movemos los registros fuera del mapa (sin clonar) y los reinsertamos.
            let old = std::mem::take(&mut save.chunks);
            let mut migrated = BTreeMap::new();
            for (pos, record) in old {
                let mut record = migrator.migrate_chunk(record);
                record.format_version = to;
                migrated.insert(pos, record);
            }
            save.chunks = migrated;
            save.header.format_version = to;
            from = to;
        }
        Ok(save)
    }
}

/// Carga un mundo de disco, lo migra al formato actual y **valida** que no haya
/// chunks corruptos ni ids de bloque desconocidos (antes de tocar el mundo).
pub fn load_and_migrate(path: &Path) -> Result<WorldSave, SaveError> {
    let save = WorldSave::load_from(path)?;
    if !save.header.is_valid() {
        return Err(SaveError::BadMagic(save.header.magic));
    }
    let save = MigrationChain::with_builtins().migrate(save)?;
    for (pos, record) in &save.chunks {
        validate_record(*pos, record)?;
    }
    Ok(save)
}

/// Valida un registro **una sola vez** (P2-3): decodifica bloques y fluido, y
/// busca ids desconocidos sobre el mismo buffer ya descomprimido.
fn validate_record(pos: ChunkPos, record: &ChunkRecord) -> Result<(), SaveError> {
    let (y0, h) = (record.y0 as usize, record.height as usize);
    if h == 0 || y0 >= WORLD_HEIGHT || y0 + h > WORLD_HEIGHT {
        return Err(SaveError::CorruptChunk { x: pos.x, z: pos.z });
    }
    let Some(blocks) = record.decode_blocks() else {
        return Err(SaveError::CorruptChunk { x: pos.x, z: pos.z });
    };
    if let Some(id) = blocks.into_iter().find(|&id| !Block::is_known_id(id)) {
        return Err(SaveError::UnknownBlock { id });
    }
    if record.decode_fluid().is_none() {
        return Err(SaveError::CorruptChunk { x: pos.x, z: pos.z });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::store::World;
    use super::super::water::Fluid;
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("solaria_test_{name}.vf"))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(with_suffix(path, ".tmp"));
        let _ = std::fs::remove_file(with_suffix(path, ".bak"));
    }

    #[test]
    fn roundtrip_guardar_y_cargar() {
        let mut save = WorldSave::new(42, 1234);
        let mut column = Column::empty();
        column.set(2, 5, 3, Block::Stone);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column));

        let path = temp_path("roundtrip");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);

        assert_eq!(loaded.header.seed, 42);
        assert!(loaded.header.is_valid());
        let record = loaded.chunks.get(&ChunkPos::new(0, 0)).unwrap();
        let blocks = record.decompressed_blocks();
        // Indice global `(y * 16 + z) * 16 + x` con y = 5.
        let idx = (5 * CHUNK_SIZE + 3) * CHUNK_SIZE + 2;
        assert_eq!(blocks[idx], Block::Stone.id());
        assert!(
            record.compression_ratio() > 3.0,
            "ratio {}",
            record.compression_ratio()
        );
    }

    #[test]
    fn roundtrip_guarda_y_carga_la_hotbar() {
        let mut save = WorldSave::new(9, 1);
        save.hotbar = vec![(Block::Dirt.id(), 3), (0, 0), (Block::Stone.id(), 64)];
        let path = temp_path("hotbar_roundtrip");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);
        assert_eq!(loaded.hotbar, save.hotbar);
    }

    #[test]
    fn guardado_migra_hotbar_vieja_a_stacks_sin_perder_bloques() {
        // Un mundo v5 (sin hotbar) se carga con la hotbar por defecto (pila
        // llena) sin perder bloques/posicion del mundo.
        let mut header = WorldHeader::new(7, 1);
        header.format_version = 5;
        let v5 = WorldSaveV5 {
            header,
            chunks: HashMap::new(),
            player_pos: [1.0, 2.0, 3.0],
        };
        let bytes = bincode::encode_to_vec(&v5, standard()).unwrap();
        let path = temp_path("hotbar_v5");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);
        assert_eq!(loaded.header.format_version, FORMAT_VERSION);
        assert_eq!(loaded.player_pos, [1.0, 2.0, 3.0]);
        assert_eq!(loaded.hotbar.len(), 9);
        assert!(loaded.hotbar.iter().all(|&(_, c)| c == 64));
        assert_eq!(loaded.hotbar, default_hotbar());
    }

    #[test]
    fn una_columna_completa_comprime_bien_conoce_su_tamano() {
        let generator = crate::world::TerrainGenerator::new(13371);
        let column = generator.generate_column(0, 0);
        let record = ChunkRecord::from_column(&column);
        assert!(record.compressed);
        assert_eq!(record.y0, 0);
        assert_eq!(record.height as usize, WORLD_HEIGHT);
        assert_eq!(record.decompressed_blocks().len(), COLUMN_VOLUME);
        assert!(!record.is_corrupt());
        let ratio = record.compression_ratio();
        println!(
            "[lz4] columna {COLUMN_VOLUME} -> {} bytes (x{ratio:.1})",
            record.blocks.len()
        );
        assert!(ratio > 5.0, "ratio {ratio:.1}");
    }

    #[test]
    fn guarda_y_recupera_ediciones_en_todas_las_alturas() {
        // El bug P0: antes solo se persistia y=64..80 y el resto se perdia.
        let alturas: [usize; 9] = [0, 5, 63, 64, 79, 80, 100, 200, 383];
        let mut column = Column::empty();
        for (i, &y) in alturas.iter().enumerate() {
            let block = match i % 3 {
                0 => Block::Stone,
                1 => Block::Obsidian,
                _ => Block::Planks,
            };
            column.set(3, y, 7, block);
        }
        let record = ChunkRecord::from_column(&column);

        let mut save = WorldSave::new(1, 0);
        save.set_chunk(ChunkPos::new(0, 0), record);
        let path = temp_path("alturas");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);

        // Aplicamos el registro sobre una columna vacia: cada edicion vuelve.
        let mut restored = Column::empty();
        crate::world::store::apply_record(
            &mut restored,
            loaded.chunks.get(&ChunkPos::new(0, 0)).unwrap(),
        );
        for (i, &y) in alturas.iter().enumerate() {
            let expected = match i % 3 {
                0 => Block::Stone,
                1 => Block::Obsidian,
                _ => Block::Planks,
            };
            assert_eq!(restored.get(3, y, 7), expected, "y={y}");
        }
    }

    #[test]
    fn el_guardado_es_atomico_y_deja_bak_sin_tmp() {
        let mut save = WorldSave::new(1, 0);
        save.set_chunk(
            ChunkPos::new(0, 0),
            ChunkRecord::from_column(&Column::empty()),
        );
        let path = temp_path("atomic");
        cleanup(&path);

        // Primer guardado: crea el archivo, sin bak (no habia anterior).
        save.save_to(&path).unwrap();
        assert!(path.exists());
        assert!(!with_suffix(&path, ".tmp").exists());

        // Segundo guardado: rota el anterior a `.bak`.
        save.player_pos = [1.0, 2.0, 3.0];
        save.save_to(&path).unwrap();
        assert!(path.exists());
        assert!(
            with_suffix(&path, ".bak").exists(),
            "deberia existir el .bak"
        );
        assert!(!with_suffix(&path, ".tmp").exists());
        let loaded = WorldSave::load_from(&path).unwrap();
        assert_eq!(loaded.player_pos, [1.0, 2.0, 3.0]);
        cleanup(&path);
    }

    #[test]
    fn un_id_de_bloque_desconocido_se_detecta() {
        // 200 no es un id valido: no se debe cargar en silencio.
        let mut raw = vec![Block::Stone.id(); COLUMN_VOLUME];
        raw[100] = 200;
        let record = ChunkRecord {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: lz4_flex::compress_prepend_size(&raw),
            fluid: Vec::new(),
            checksum: fnv1a64(&raw),
            fluid_checksum: 0,
        };
        assert_eq!(record.first_unknown_id(), Some(200));
        assert!(!Block::is_known_id(200));
        assert!(Block::is_known_id(Block::Podzol.id()));
        assert!(
            Block::is_known_id(Block::Obsidian.id()),
            "obsidiana es valida"
        );
    }

    #[test]
    fn un_chunk_corrupto_se_detecta() {
        let record = ChunkRecord {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: vec![1, 2, 3, 4], // LZ4 invalido
            fluid: Vec::new(),
            checksum: 0,
            fluid_checksum: 0,
        };
        assert!(record.is_corrupt());
    }

    #[test]
    fn la_posicion_del_jugador_se_guarda() {
        let mut save = WorldSave::new(1, 0);
        save.player_pos = [12.5, 70.25, -3.75];
        let path = temp_path("player_pos");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);
        assert_eq!(loaded.player_pos, [12.5, 70.25, -3.75]);
    }

    #[test]
    fn un_mundo_v2_sin_posicion_se_lee_con_la_por_defecto() {
        let mut header = WorldHeader::new(9, 123);
        header.format_version = 2;
        let v2 = WorldSaveV2 {
            header,
            chunks: HashMap::new(),
        };
        let bytes = bincode::encode_to_vec(&v2, standard()).unwrap();
        let path = temp_path("v2_nopos");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);
        assert_eq!(loaded.header.seed, 9);
        assert_eq!(loaded.player_pos, DEFAULT_PLAYER_POS);
    }

    #[test]
    fn un_mundo_v3_se_migra_conservando_la_seccion_antigua() {
        // Emulamos un archivo v3: un registro con una seccion (4096 bytes) en
        // layout antiguo. Debe migrar a v4 en `y0 = 64, height = 16`.
        let mut raw = vec![Block::Air.id(); CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
        // Edicion en la capa local 5 del chunk -> y global 69.
        let local = (5 * CHUNK_SIZE + 3) * CHUNK_SIZE + 2;
        raw[local] = Block::Wood.id();
        let old = ChunkRecordV3 {
            format_version: 3,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            blocks: lz4_flex::compress_prepend_size(&raw),
        };
        let mut chunks = HashMap::new();
        chunks.insert(ChunkPos::new(2, 3), old);
        let mut header = WorldHeader::new(7, 1);
        header.format_version = 3;
        let v3 = WorldSaveV3 {
            header,
            chunks,
            player_pos: [4.0, 5.0, 6.0],
        };
        let bytes = bincode::encode_to_vec(&v3, standard()).unwrap();
        let path = temp_path("v3_migrate");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);

        assert_eq!(loaded.header.format_version, FORMAT_VERSION);
        assert_eq!(loaded.player_pos, [4.0, 5.0, 6.0]);
        let record = loaded.chunks.get(&ChunkPos::new(2, 3)).unwrap();
        assert_eq!(record.y0, LEGACY_TERRAIN_Y0);
        assert_eq!(record.height, CHUNK_SIZE as u32);
        // Aplicado, la edicion cae en y=69.
        let mut col = Column::empty();
        crate::world::store::apply_record(&mut col, record);
        assert_eq!(col.get(2, 69, 3), Block::Wood);
    }

    #[test]
    fn el_nivel_de_flujo_sobrevive_al_guardado_y_la_carga() {
        let mut column = Column::empty();
        column.set(3, 70, 4, Block::Water);
        column.set(4, 70, 4, Block::Water);
        // La celda 3 es fuente (flujo 0); la 4 fluye con nivel 5.
        column.set_flow(4, 70, 4, 5);
        let record = ChunkRecord::from_column(&column);
        assert!(!record.fluid.is_empty(), "deberia guardar fluido");

        let mut save = WorldSave::new(1, 0);
        save.set_chunk(ChunkPos::new(0, 0), record);
        let path = temp_path("fluid_roundtrip");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);

        let mut restored = Column::empty();
        crate::world::store::apply_record(
            &mut restored,
            loaded.chunks.get(&ChunkPos::new(0, 0)).unwrap(),
        );
        assert_eq!(restored.get(3, 70, 4), Block::Water);
        assert_eq!(restored.flow_at(3, 70, 4), 0, "la fuente no tiene flujo");
        assert_eq!(restored.flow_at(4, 70, 4), 5, "el flujo debe sobrevivir");
    }

    #[test]
    fn una_columna_sin_agua_que_fluya_no_guarda_fluido() {
        // Un oceano es todo fuentes: `from_column` no reserva el array de flujo.
        let mut column = Column::empty();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                column.set(x, 70, z, Block::Water);
            }
        }
        let record = ChunkRecord::from_column(&column);
        assert!(
            record.decompressed_fluid().is_empty(),
            "sin agua que fluya, `fluid` debe ir vacio"
        );
        assert!(!record.is_corrupt());
    }

    #[test]
    fn un_mundo_v1_sin_comprimir_se_migra() {
        // v1: una seccion de 4096 bytes sin comprimir, sin `compressed` ni pos.
        let mut raw = vec![Block::Air.id(); CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
        let local = (5 * CHUNK_SIZE + 3) * CHUNK_SIZE + 2; // capa local 5 -> y=69
        raw[local] = Block::Stone.id();
        let old = ChunkRecordV1 {
            format_version: 1,
            generator_version: GENERATOR_VERSION,
            blocks: raw,
        };
        let mut chunks = HashMap::new();
        chunks.insert(ChunkPos::new(1, 2), old);
        let mut header = WorldHeader::new(3, 1);
        header.format_version = 1;
        let v1 = WorldSaveV1 { header, chunks };
        let bytes = bincode::encode_to_vec(&v1, standard()).unwrap();
        let path = temp_path("v1_migrate");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);

        assert_eq!(loaded.header.format_version, FORMAT_VERSION);
        let record = loaded.chunks.get(&ChunkPos::new(1, 2)).unwrap();
        assert!(
            record.compressed,
            "v1 sin comprimir debe migrar a comprimido"
        );
        assert_eq!(record.y0, LEGACY_TERRAIN_Y0);
        assert_eq!(record.height, CHUNK_SIZE as u32);
        let mut col = Column::empty();
        crate::world::store::apply_record(&mut col, record);
        assert_eq!(col.get(2, 69, 3), Block::Stone);
    }

    #[test]
    fn un_mundo_v4_se_migra_con_todo_el_agua_como_fuente() {
        // Emulamos un archivo v4: columna completa sin campo `fluid`.
        let mut raw = vec![Block::Air.id(); COLUMN_VOLUME];
        let idx = (70 * CHUNK_SIZE + 4) * CHUNK_SIZE + 3;
        raw[idx] = Block::Water.id();
        let old = ChunkRecordV4 {
            format_version: 4,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: lz4_flex::compress_prepend_size(&raw),
        };
        let mut chunks = HashMap::new();
        chunks.insert(ChunkPos::new(0, 0), old);
        let mut header = WorldHeader::new(7, 1);
        header.format_version = 4;
        let v4 = WorldSaveV4 {
            header,
            chunks,
            player_pos: [1.0, 2.0, 3.0],
        };
        let bytes = bincode::encode_to_vec(&v4, standard()).unwrap();
        let path = temp_path("v4_fluid_migrate");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);

        assert_eq!(loaded.header.format_version, FORMAT_VERSION);
        assert_eq!(loaded.player_pos, [1.0, 2.0, 3.0]);
        let record = loaded.chunks.get(&ChunkPos::new(0, 0)).unwrap();
        assert!(record.fluid.is_empty(), "v4 no traia fluido");
        let mut col = Column::empty();
        crate::world::store::apply_record(&mut col, record);
        assert_eq!(col.get(3, 70, 4), Block::Water);
        assert_eq!(col.flow_at(3, 70, 4), 0, "el agua v4 es fuente");
    }

    #[test]
    fn roundtrip_completo_del_mundo_guardado_y_recargado() {
        // Flujo real: editar un mundo, volcarlo, guardarlo, cargarlo y
        // reconstruirlo. Comprueba ediciones en varias alturas y el agua.
        let mut world = World::new(7, 1, vec![]);
        world.warm_streaming([0.0, 100.0, 0.0]);
        world.set_block([2, 5, 3], Block::Stone);
        world.set_block([2, 200, 3], Block::Planks);
        world.set_block([2, 100, 3], Block::Water); // fuente
        world.sync_modified();

        let mut save = WorldSave::new(7, 0);
        for (pos, record) in world.modified_records() {
            save.set_chunk(*pos, record.clone());
        }
        let path = temp_path("world_roundtrip");
        cleanup(&path);
        save.save_to(&path).unwrap();

        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);
        let restored: Vec<(ChunkPos, ChunkRecord)> =
            loaded.chunks.iter().map(|(p, r)| (*p, r.clone())).collect();
        let mut world2 = World::new(7, 1, restored);
        world2.warm_streaming([0.0, 100.0, 0.0]);

        assert_eq!(world2.get_block([2, 5, 3]), Block::Stone, "y=5");
        assert_eq!(world2.get_block([2, 200, 3]), Block::Planks, "y=200");
        assert_eq!(world2.water_at([2, 100, 3]), Fluid::Source, "agua fuente");
    }

    #[test]
    fn un_archivo_principal_corrupto_no_pierde_el_bak() {
        // Simula un crash a mitad de escritura: el principal queda truncado y
        // debe poder recuperarse el `.bak` (guardado anterior).
        let path = temp_path("crash");
        cleanup(&path);
        let mut column = Column::empty();
        column.set(1, 2, 3, Block::Stone);
        let mut save = WorldSave::new(7, 0);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column));
        save.save_to(&path).unwrap();
        // Segundo guardado: rota el primero a `.bak`.
        save.player_pos = [1.0, 2.0, 3.0];
        save.save_to(&path).unwrap();
        let bak = with_suffix(&path, ".bak");
        assert!(bak.exists(), "deberia existir el .bak");

        // Crash: el archivo principal queda corrupto/truncado.
        std::fs::write(&path, b"\x00\x01\x02").unwrap();
        assert!(
            WorldSave::load_from(&path).is_err(),
            "el principal corrupto no debe cargar"
        );
        // El `.bak` sigue siendo cargable.
        let recovered = WorldSave::load_from(&bak).expect("el .bak debe recuperarse");
        assert!(recovered.chunks.contains_key(&ChunkPos::new(0, 0)));
        cleanup(&path);
    }

    #[test]
    fn el_checksum_detecta_corrupcion_que_conserva_el_tamano() {
        // P1-4: LZ4 no trae checksum. Alterar el payload **descomprimido** y
        // recomprimirlo da un tamano correcto pero datos distintos: debe fallar.
        let mut raw = vec![Block::Stone.id(); COLUMN_VOLUME];
        raw[100] = Block::Dirt.id();
        let mut record = ChunkRecord {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: lz4_flex::compress_prepend_size(&raw),
            fluid: Vec::new(),
            checksum: fnv1a64(&raw),
            fluid_checksum: 0,
        };
        assert!(!record.is_corrupt(), "el original es valido");
        raw[100] = Block::Planks.id();
        record.blocks = lz4_flex::compress_prepend_size(&raw);
        assert!(record.is_corrupt(), "el checksum debe detectar el cambio");
    }

    #[test]
    fn un_prefijo_de_tamano_absurdo_se_rechaza_sin_descomprimir() {
        // P2-4: un prefijo que pide gigabytes no debe asignarse.
        let record = ChunkRecord {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            y0: 0,
            height: WORLD_HEIGHT as u32,
            blocks: vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 2],
            fluid: Vec::new(),
            checksum: 0,
            fluid_checksum: 0,
        };
        assert!(record.is_corrupt());
    }

    #[test]
    fn los_bytes_sobrantes_se_rechazan() {
        // P2-6: un archivo con basura al final no debe aceptarse en silencio.
        let path = temp_path("trailing");
        cleanup(&path);
        let save = WorldSave::new(7, 1);
        save.save_to(&path).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(b"basura");
        std::fs::write(&path, &bytes).unwrap();
        let err = WorldSave::load_from(&path).unwrap_err();
        cleanup(&path);
        assert!(matches!(err, SaveError::TrailingBytes { .. }), "{err}");
    }

    #[test]
    fn una_cabecera_corrupta_es_su_propio_error() {
        // P2-5: no debe reportarse como "no hay migrador de v0".
        let path = temp_path("bad_header");
        cleanup(&path);
        std::fs::write(&path, b"\x00\x01\x02").unwrap();
        let err = WorldSave::load_from(&path).unwrap_err();
        cleanup(&path);
        assert!(matches!(err, SaveError::CorruptHeader), "{err}");
    }

    #[test]
    fn el_guardado_es_determinista() {
        // P2-9: el mismo estado produce exactamente los mismos bytes.
        let mut a = WorldSave::new(7, 1);
        let mut b = WorldSave::new(7, 1);
        let rec = ChunkRecord::from_column(&Column::empty());
        a.set_chunk(ChunkPos::new(1, 2), rec.clone());
        a.set_chunk(ChunkPos::new(-3, 4), rec.clone());
        b.set_chunk(ChunkPos::new(-3, 4), rec.clone());
        b.set_chunk(ChunkPos::new(1, 2), rec);
        assert_eq!(
            bincode::encode_to_vec(&a, standard()).unwrap(),
            bincode::encode_to_vec(&b, standard()).unwrap()
        );
    }

    #[test]
    fn se_recupera_del_bak_si_el_principal_esta_corrupto() {
        // P0-1 + P1-5: el principal corrupto se cuarentena y el .bak se usa.
        let path = temp_path("recover");
        cleanup(&path);
        let _ = std::fs::remove_file(with_suffix(&path, ".corrupt"));
        let mut column = Column::empty();
        column.set(1, 2, 3, Block::Stone);
        let mut save = WorldSave::new(7, 0);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column));
        save.save_to(&path).unwrap();
        save.player_pos = [1.0, 2.0, 3.0];
        save.save_to(&path).unwrap();
        let bak = with_suffix(&path, ".bak");
        assert!(bak.exists());
        std::fs::write(&path, b"\x00\x01\x02").unwrap();

        match load_world(&path) {
            WorldLoad::Recovered(save) => {
                assert!(save.chunks.contains_key(&ChunkPos::new(0, 0)));
            }
            other => panic!("se esperaba Recovered, {other:?}"),
        }
        // El .bak bueno sigue existiendo y el principal ya es valido.
        assert!(bak.exists(), "el .bak bueno no debe destruirse");
        assert!(with_suffix(&path, ".corrupt").exists(), "cuarentena");
        assert!(load_and_migrate(&path).is_ok(), "el principal se restauro");
        cleanup(&path);
        let _ = std::fs::remove_file(with_suffix(&path, ".corrupt"));
    }

    #[test]
    fn sin_principal_pero_con_bak_se_recupera() {
        // Ventana de crash entre los dos `rename` de `save_to` (P2-7).
        let path = temp_path("recover_missing");
        cleanup(&path);
        let save = WorldSave::new(7, 0);
        save.save_to(&path).unwrap();
        save.clone().save_to(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(load_world(&path), WorldLoad::Recovered(_)));
        assert!(load_and_migrate(&path).is_ok());
        cleanup(&path);
    }

    #[test]
    fn sin_principal_y_con_backup_ilegible_no_es_un_mundo_nuevo() {
        // Si existe un `.bak`, el mundo existio: un backup tambien ilegible no
        // debe abrirse como mundo vacio (P0-1).
        let path = temp_path("missing_bad_bak");
        cleanup(&path);
        std::fs::write(with_suffix(&path, ".bak"), b"\x00\x01\x02").unwrap();
        assert!(matches!(
            load_world(&path),
            WorldLoad::Unreadable(SaveError::CorruptHeader)
        ));
        cleanup(&path);
    }

    #[test]
    fn un_mundo_inexistente_es_missing() {
        let path = temp_path("missing");
        cleanup(&path);
        assert!(matches!(load_world(&path), WorldLoad::Missing));
    }

    #[test]
    fn un_formato_futuro_no_se_degrada_a_un_backup() {
        let path = temp_path("future");
        cleanup(&path);
        let save = WorldSave::new(7, 0);
        save.save_to(&path).unwrap();
        save.clone().save_to(&path).unwrap();
        let bak = with_suffix(&path, ".bak");
        assert!(bak.exists());
        // Principal "del futuro" (cabecera valida, formato desconocido); el
        // .bak sigue siendo valido y no debe usarse para degradarlo.
        let mut future = WorldSave::new(7, 0);
        future.header.format_version = FORMAT_VERSION + 5;
        std::fs::write(&path, bincode::encode_to_vec(&future, standard()).unwrap()).unwrap();
        match load_world(&path) {
            WorldLoad::Unreadable(SaveError::TooNew { .. }) => {}
            other => panic!("se esperaba Unreadable(TooNew), {other:?}"),
        }
        assert!(bak.exists(), "el backup no se toca");
        cleanup(&path);
    }

    #[test]
    fn un_mundo_v6_se_migra_y_calcula_checksum() {
        let mut raw = vec![Block::Air.id(); COLUMN_VOLUME];
        raw[3] = Block::Stone.id();
        let v6 = WorldSaveV6 {
            header: {
                let mut h = WorldHeader::new(7, 1);
                h.format_version = 6;
                h
            },
            chunks: HashMap::from([(
                ChunkPos::new(0, 0),
                ChunkRecordV6 {
                    format_version: 6,
                    generator_version: GENERATOR_VERSION,
                    compressed: true,
                    y0: 0,
                    height: WORLD_HEIGHT as u32,
                    blocks: lz4_flex::compress_prepend_size(&raw),
                    fluid: Vec::new(),
                },
            )]),
            player_pos: DEFAULT_PLAYER_POS,
            hotbar: default_hotbar(),
        };
        let bytes = bincode::encode_to_vec(&v6, standard()).unwrap();
        let path = temp_path("v6_migrate");
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_and_migrate(&path).unwrap();
        cleanup(&path);
        assert_eq!(loaded.header.format_version, FORMAT_VERSION);
        let record = &loaded.chunks[&ChunkPos::new(0, 0)];
        assert_ne!(record.checksum, 0, "se calcula el checksum al migrar");
        assert!(!record.is_corrupt());
        assert_eq!(record.decompressed_blocks()[3], Block::Stone.id());
    }

    #[test]
    fn el_header_actual_es_el_formato_actual() {
        let save = WorldSave::new(1, 0);
        assert_eq!(save.header.format_version, FORMAT_VERSION);
        assert!(save.header.is_valid());
    }

    #[test]
    fn formato_del_futuro_no_se_puede_leer() {
        let mut save = WorldSave::new(1, 0);
        save.header.format_version = FORMAT_VERSION + 5;
        let err = MigrationChain::with_builtins().migrate(save).unwrap_err();
        assert!(matches!(err, SaveError::TooNew { .. }));
    }

    #[test]
    fn la_cadena_aplica_migradores_hasta_el_actual() {
        let mut save = WorldSave::new(1, 0);
        save.header.format_version = 0;
        let err = MigrationChain::with_builtins()
            .migrate(save.clone())
            .unwrap_err();
        assert!(matches!(err, SaveError::NoMigration { .. }));

        struct V0ToV1;
        impl WorldMigrator for V0ToV1 {
            fn from_version(&self) -> u32 {
                0
            }
            fn to_version(&self) -> u32 {
                1
            }
        }
        let mut chain = MigrationChain::with_builtins();
        chain.push(Box::new(V0ToV1));
        let migrated = chain.migrate(save).unwrap();
        assert_eq!(migrated.header.format_version, FORMAT_VERSION);
    }

    #[test]
    fn migrar_preserva_las_ediciones_del_jugador() {
        let mut save = WorldSave::new(7, 999);
        save.header.format_version = 0;

        let mut column = Column::empty();
        column.set(1, 2, 3, Block::Torch);
        column.set(10, 100, 5, Block::Stone); // y=100 (fuera de la antigua seccion)
        // Emulamos un registro v1: bloques sin comprimir.
        let mut original = ChunkRecord::from_column(&column);
        original.blocks = original.decompressed_blocks();
        original.compressed = false;
        original.format_version = 1;
        save.set_chunk(ChunkPos::new(3, -2), original);

        struct V0ToV1;
        impl WorldMigrator for V0ToV1 {
            fn from_version(&self) -> u32 {
                0
            }
            fn to_version(&self) -> u32 {
                1
            }
            fn migrate_chunk(&self, mut r: ChunkRecord) -> ChunkRecord {
                r.format_version = self.to_version();
                r
            }
        }
        let mut chain = MigrationChain::with_builtins();
        chain.push(Box::new(V0ToV1));
        let migrated = chain.migrate(save).unwrap();

        let record = migrated.chunks.get(&ChunkPos::new(3, -2)).unwrap();
        let blocks = record.decompressed_blocks();
        let idx_torch = (2 * CHUNK_SIZE + 3) * CHUNK_SIZE + 1;
        let idx_stone = (100 * CHUNK_SIZE + 5) * CHUNK_SIZE + 10;
        assert_eq!(blocks[idx_torch], Block::Torch.id());
        assert_eq!(blocks[idx_stone], Block::Stone.id());
        assert_eq!(record.format_version, FORMAT_VERSION);
    }

    #[test]
    fn un_mundo_viejo_se_puede_guardar_y_recargar_tras_migrar() {
        // Ciclo completo: archivo v1 real -> migrar -> volver a guardar (ya v5)
        // -> recargar. El chunk no se pierde.
        let mut raw = vec![Block::Air.id(); CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
        raw[0] = Block::Wood.id();
        let old = ChunkRecordV1 {
            format_version: 1,
            generator_version: GENERATOR_VERSION,
            blocks: raw,
        };
        let mut chunks = HashMap::new();
        chunks.insert(ChunkPos::new(0, 0), old);
        let mut header = WorldHeader::new(5, 1);
        header.format_version = 1;
        let v1 = WorldSaveV1 { header, chunks };
        let bytes = bincode::encode_to_vec(&v1, standard()).unwrap();
        let path = temp_path("migrate_roundtrip");
        std::fs::write(&path, &bytes).unwrap();

        let migrated = load_and_migrate(&path).unwrap();
        cleanup(&path);
        assert_eq!(migrated.header.format_version, FORMAT_VERSION);

        let path2 = temp_path("migrate_roundtrip2");
        migrated.save_to(&path2).unwrap();
        let reopened = WorldSave::load_from(&path2).unwrap();
        cleanup(&path2);
        assert_eq!(reopened.header.format_version, FORMAT_VERSION);
        assert!(reopened.chunks.contains_key(&ChunkPos::new(0, 0)));
    }
}
