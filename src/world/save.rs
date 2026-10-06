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

use std::collections::HashMap;
use std::io::Write;
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
pub const FORMAT_VERSION: u32 = 5;

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
pub const GENERATOR_VERSION: u32 = 8;

/// El "magic number" que identifica un archivo de mundo de Solaria.
pub const MAGIC: [u8; 4] = *b"VFWD";

/// Identificador de la version del motor que guardo el mundo.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Posicion de un chunk en el mundo, en coordenadas de chunk (no de bloque).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode)]
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
        let mut raw = Vec::with_capacity(COLUMN_VOLUME);
        let mut fluid = Vec::new();
        if column.has_flow_storage() {
            fluid.reserve(COLUMN_VOLUME);
        }
        for y in 0..WORLD_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    raw.push(column.get(x, y, z).id());
                    if column.has_flow_storage() {
                        fluid.push(column.flow_at(x, y, z));
                    }
                }
            }
        }
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
        }
    }

    /// Bytes crudos esperados segun `height` (0 si el registro esta corrupto).
    pub fn raw_len(&self) -> usize {
        self.height as usize * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Bloques sin comprimir. `Vec::new()` si el payload esta corrupto (lo
    /// detecta [`ChunkRecord::is_corrupt`]).
    pub fn decompressed_blocks(&self) -> Vec<u8> {
        if self.compressed {
            lz4_flex::decompress_size_prepended(&self.blocks).unwrap_or_default()
        } else {
            self.blocks.clone()
        }
    }

    /// Niveles de flujo sin comprimir. Vacio si la columna no guardo fluidos
    /// (oceano: todo fuentes) o si el payload esta corrupto.
    pub fn decompressed_fluid(&self) -> Vec<u8> {
        if self.fluid.is_empty() {
            return Vec::new();
        }
        if self.compressed {
            lz4_flex::decompress_size_prepended(&self.fluid).unwrap_or_default()
        } else {
            self.fluid.clone()
        }
    }

    /// ¿El payload no decodifica al tamano esperado? (chunk corrupto). El campo
    /// `fluid` solo se valida si el registro lo trae; vacio es valido.
    pub fn is_corrupt(&self) -> bool {
        if self.decompressed_blocks().len() != self.raw_len() {
            return true;
        }
        !self.fluid.is_empty() && self.decompressed_fluid().len() != self.raw_len()
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
        self.format_version = 2;
    }
}

/// Todo lo que se guarda de un mundo.
#[derive(Clone, Debug, Encode, Decode)]
pub struct WorldSave {
    pub header: WorldHeader,
    /// Columnas modificadas, indexadas por su posicion.
    pub chunks: HashMap<ChunkPos, ChunkRecord>,
    /// Posicion del jugador (guardado completo). Desde el formato v3.
    pub player_pos: [f32; 3],
}

/// Regresion del jugador por defecto (si un mundo viejo no la trae).
pub const DEFAULT_PLAYER_POS: [f32; 3] = [8.0, 76.0, 20.0];

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
    }
}

impl WorldSave {
    pub fn new(seed: u32, created_at: u64) -> Self {
        Self {
            header: WorldHeader::new(seed, created_at),
            chunks: HashMap::new(),
            player_pos: DEFAULT_PLAYER_POS,
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
        Ok(())
    }

    /// Lee y deserializa un mundo de disco (sin migrar). Elige el layout de los
    /// `ChunkRecord` segun la `format_version` de la cabecera: bincode es
    /// posicional, asi que decodificar un archivo v3 con el layout v4 daria
    /// bytes mal interpretados.
    pub fn load_from(path: &Path) -> Result<Self, SaveError> {
        let bytes = std::fs::read(path)?;
        // Espiamos solo la cabecera (esta al principio) para conocer la version.
        let version = bincode::decode_from_slice::<WorldHeader, _>(&bytes, standard())
            .map(|(h, _)| h.format_version)
            .unwrap_or(0);
        if version > FORMAT_VERSION {
            return Err(SaveError::TooNew {
                format: version,
                supported: FORMAT_VERSION,
            });
        }
        match version {
            5 => {
                let (save, _) = bincode::decode_from_slice::<WorldSave, _>(&bytes, standard())?;
                Ok(save)
            }
            4 => {
                let (v4, _) = bincode::decode_from_slice::<WorldSaveV4, _>(&bytes, standard())?;
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
                })
            }
            3 => {
                let (v3, _) = bincode::decode_from_slice::<WorldSaveV3, _>(&bytes, standard())?;
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
                })
            }
            // v1/v2 comparten el layout de `ChunkRecordV3` (aunque v1 sin el flag
            // `compressed` no se soporta de verdad; se documenta).
            _ => {
                let (v2, _): (WorldSaveV2, usize) = bincode::decode_from_slice(&bytes, standard())?;
                let mut header = v2.header;
                header.format_version = header.format_version.min(2);
                Ok(WorldSave {
                    header,
                    chunks: v2
                        .chunks
                        .into_iter()
                        .map(|(p, r)| (p, upgrade_v3_record(r)))
                        .collect(),
                    player_pos: DEFAULT_PLAYER_POS,
                })
            }
        }
    }
}

/// `path` con un sufijo extra (`world.vf` -> `world.vf.tmp`).
fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(suffix);
    std::path::PathBuf::from(os)
}

/// Errores de guardado/carga.
#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    Decode(bincode::error::DecodeError),
    Encode(bincode::error::EncodeError),
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
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        chunk.clone()
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
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        let mut record = chunk.clone();
        record.compressed = false;
        record.migrate_to_v2();
        record
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
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        let mut r = chunk.clone();
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
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        let mut r = chunk.clone();
        r.fluid = Vec::new();
        r.format_version = 5;
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
            for record in save.chunks.values_mut() {
                *record = migrator.migrate_chunk(record);
                record.format_version = to;
            }
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
        if record.is_corrupt() {
            return Err(SaveError::CorruptChunk { x: pos.x, z: pos.z });
        }
        if let Some(id) = record.first_unknown_id() {
            return Err(SaveError::UnknownBlock { id });
        }
    }
    Ok(save)
}

#[cfg(test)]
mod tests {
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
            fn migrate_chunk(&self, record: &ChunkRecord) -> ChunkRecord {
                let mut r = record.clone();
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
        let mut save = WorldSave::new(5, 1);
        save.header.format_version = 0;
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Wood);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column));

        let path = temp_path("migrate_roundtrip");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        cleanup(&path);

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
        let migrated = chain.migrate(loaded).unwrap();

        let path2 = temp_path("migrate_roundtrip2");
        migrated.save_to(&path2).unwrap();
        let reopened = WorldSave::load_from(&path2).unwrap();
        cleanup(&path2);
        assert_eq!(reopened.header.format_version, FORMAT_VERSION);
        assert!(reopened.chunks.contains_key(&ChunkPos::new(0, 0)));
    }
}
