//! Guardado y versionado del mundo.
//!
//! Este modulo responde a una pregunta que en v0.3.x dolia: **"¿y mis
//! ediciones?"**. Antes el mundo se regeneraba desde la semilla y se perdia
//! todo lo que rompias o colocabas. Aqui definimos:
//!
//! * `WorldHeader` — la "ficha" del mundo: version del formato, version del
//!   generador, version del motor, semilla y fecha.
//! * `ChunkRecord` — los bloques de un chunk que ha sido **modificado** por el
//!   jugador, con su propia version de formato.
//! * `WorldSave` — el contenedor de todo, serializado con `bincode`.
//! * [`MigrationChain`] — migradores para traer mundos de formatos antiguos.
//!
//! Versionado: `FORMAT_VERSION` es la version del **formato de archivo** (sube
//! cuando cambia el layout binario). `GENERATOR_VERSION` es la del generador de
//! terreno (sube cuando cambia el algoritmo y un mundo viejo ya no se reproduce
//! igual). Son independientes, como pide la guia.

use std::collections::HashMap;
use std::path::Path;

use bincode::config::standard;
use bincode::{Decode, Encode};

use super::chunk::{CHUNK_SIZE, CHUNK_VOLUME, Column, WORLD_HEIGHT};

/// Version actual del formato de archivo. Sube SIEMPRE que cambie como se
/// serializan los datos (rompe compatibilidad binaria).
///
/// * v1: `ChunkRecord.blocks` eran 4096 bytes sin comprimir.
/// * v2: `ChunkRecord.blocks` guarda bytes **comprimidos con LZ4** (y un flag).
pub const FORMAT_VERSION: u32 = 2;

/// Version actual del generador de terreno.
///
/// * v1: solo colinas (Perlin), superficie de hierba/tierra/piedra.
/// * v2: biomas (Worley) con superficie de arena/hierba/nieve.
/// * v3: altura y bioma calculados **por bloque** (colinas suaves, no mesetas
///   planas de 16x16).
/// * v4: **cuevas** con ruido Perlin 3D (iso-superficie).
/// * v5: **oceanos/lagos**: agua hasta el nivel del mar y playas de arena.
/// * v6: **vegetacion**: arboles (tronco de madera + copa de hojas) por bioma.
pub const GENERATOR_VERSION: u32 = 6;

/// El "magic number" que identifica un archivo de mundo de Solaria.
pub const MAGIC: [u8; 4] = *b"VFWD"; // VoxelForge World / Solaria

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

/// Ficha del mundo. Es lo primero que se lee de un archivo y permite decidir si
/// hay que migrar.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct WorldHeader {
    /// Identifica el tipo de archivo.
    pub magic: [u8; 4],
    /// Version del formato de archivo con el que se guardo.
    pub format_version: u32,
    /// Version del generador de terreno que lo creo (para reproducir terreno).
    pub generator_version: u32,
    /// Semilla del mundo.
    pub seed: u32,
    /// Version del motor (texto), para diagnostico.
    pub engine_version: String,
    /// Fecha de creacion (segundos desde epoch de UNIX).
    pub created_at: u64,
}

impl Default for WorldHeader {
    /// Una ficha "vacia" en el formato actual (util para `App::default()`).
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl WorldHeader {
    /// Crea una ficha nueva para la version actual.
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

    /// ¿Es un archivo de mundo valido? (comprueba el magic).
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
    }
}

/// Los bloques de un chunk modificado por el jugador.
///
/// Guardamos el chunk **completo** (4096 bloques) en lugar de solo el "diff".
/// Desde el formato v2 los bytes van **comprimidos con LZ4** (un chunk de
/// terreno baja de 4096 a unos pocos cientos de bytes, porque hay muchisimo
/// aire y zonas uniformes).
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ChunkRecord {
    /// Version del formato de ESTE chunk (permite migrar chunk a chunk).
    pub format_version: u32,
    /// Version del generador con el que se genero su terreno base.
    pub generator_version: u32,
    /// ¿`blocks` esta comprimido con LZ4?
    pub compressed: bool,
    /// Los bloques (4096 si `!compressed`; bytes LZ4 si `compressed`).
    pub blocks: Vec<u8>,
}

impl ChunkRecord {
    /// Construye el registro a partir de una columna (solo su chunk `chunk_y`
    /// vertical; de momento guardamos el chunk que contiene el terreno). Los
    /// bloques se comprimen con LZ4.
    pub fn from_column(column: &Column, chunk_y: usize) -> Self {
        let mut raw = Vec::with_capacity(CHUNK_VOLUME);
        let y0 = chunk_y * CHUNK_SIZE;
        for y in y0..(y0 + CHUNK_SIZE).min(WORLD_HEIGHT) {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    raw.push(column.get(x, y, z).id());
                }
            }
        }
        Self {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            compressed: true,
            blocks: lz4_flex::compress_prepend_size(&raw),
        }
    }

    /// Devuelve los 4096 bloques sin comprimir (descomprime si hace falta).
    pub fn decompressed_blocks(&self) -> Vec<u8> {
        if self.compressed {
            lz4_flex::decompress_size_prepended(&self.blocks).unwrap_or_else(|_| Vec::new())
        } else {
            self.blocks.clone()
        }
    }

    /// Ratio de compresion (`raw / compressed`). 1.0 = no comprime.
    pub fn compression_ratio(&self) -> f32 {
        if self.compressed && !self.blocks.is_empty() {
            CHUNK_VOLUME as f32 / self.blocks.len() as f32
        } else {
            1.0
        }
    }

    /// Migra un registro v1 (sin comprimir) al v2 (comprimido). Es un no-op
    /// funcional cuando ya esta comprimido.
    pub fn migrate_to_v2(&mut self) {
        if !self.compressed {
            self.blocks = lz4_flex::compress_prepend_size(&self.blocks);
            self.compressed = true;
        }
        self.format_version = 2;
    }
}

/// Todo lo que se guarda de un mundo.
#[derive(Clone, Debug, Encode, Decode)]
pub struct WorldSave {
    pub header: WorldHeader,
    /// Chunks modificados, indexados por su posicion.
    pub chunks: HashMap<ChunkPos, ChunkRecord>,
}

impl WorldSave {
    /// Crea un mundo vacio (sin ediciones).
    pub fn new(seed: u32, created_at: u64) -> Self {
        Self {
            header: WorldHeader::new(seed, created_at),
            chunks: HashMap::new(),
        }
    }

    /// Marca un chunk como modificado (o lo actualiza).
    pub fn set_chunk(&mut self, pos: ChunkPos, record: ChunkRecord) {
        self.chunks.insert(pos, record);
    }

    /// Escribe el mundo a disco en formato binario.
    ///
    /// `bincode 2` trabaja con un `Vec<u8>`: serializamos y luego escribimos el
    /// archivo. El formato es compacto y rapido (no es texto legible, a
    /// diferencia de JSON; eso es intencionado).
    pub fn save_to(&self, path: &Path) -> Result<(), SaveError> {
        let bytes = bincode::encode_to_vec(self, standard())?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Lee y deserializa un mundo de disco. **No** migra todavia; llama a
    /// [`load_and_migrate`] para eso.
    pub fn load_from(path: &Path) -> Result<Self, SaveError> {
        let bytes = std::fs::read(path)?;
        let (save, _len): (WorldSave, usize) = bincode::decode_from_slice(&bytes, standard())?;
        Ok(save)
    }
}

/// Errores de guardado/carga.
#[derive(Debug)]
pub enum SaveError {
    /// Error de entrada/salida (no existe el archivo, permisos...).
    Io(std::io::Error),
    /// Los bytes no corresponden a un `WorldSave` valido.
    Decode(bincode::error::DecodeError),
    /// No se pudo serializar el mundo (no deberia ocurrir con bincode).
    Encode(bincode::error::EncodeError),
    /// El archivo no tiene el magic esperado.
    BadMagic([u8; 4]),
    /// El formato es mas nuevo que el motor (no sabemos leerlo).
    TooNew { format: u32, supported: u32 },
    /// No hay un migrador que cubra ese salto de version.
    NoMigration { from: u32, to: u32 },
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
    /// Convierte los datos de un chunk. Por defecto, los deja igual (util para
    /// migradores que solo tocan la cabecera).
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        chunk.clone()
    }
}

/// Migrador v1 -> v2: comprime con LZ4 los bloques que iban sin comprimir.
pub struct V1ToV2;

impl WorldMigrator for V1ToV2 {
    fn from_version(&self) -> u32 {
        1
    }
    fn to_version(&self) -> u32 {
        2
    }
    fn migrate_chunk(&self, chunk: &ChunkRecord) -> ChunkRecord {
        // En v1 `compressed` no existia; al deserializar v1 llega en `false`
        // (por defecto de bincode no habia campo). Lo normalizamos a comprimido.
        let mut record = chunk.clone();
        record.compressed = false;
        record.migrate_to_v2();
        record
    }
}

/// Cadena de migradores: los aplica en orden hasta llegar al formato actual.
#[derive(Default)]
pub struct MigrationChain {
    migrators: Vec<Box<dyn WorldMigrator>>,
}

impl MigrationChain {
    /// Crea la cadena con los migradores conocidos por el motor.
    pub fn with_builtins() -> Self {
        Self {
            migrators: vec![Box::new(V1ToV2)],
        }
    }

    /// Anade un migrador (util en tests y para plugins futuros).
    pub fn push(&mut self, m: Box<dyn WorldMigrator>) {
        self.migrators.push(m);
    }

    /// Migra un `WorldSave` desde su version de formato hasta `FORMAT_VERSION`.
    pub fn migrate(&self, mut save: WorldSave) -> Result<WorldSave, SaveError> {
        let mut from = save.header.format_version;
        if from > FORMAT_VERSION {
            return Err(SaveError::TooNew {
                format: from,
                supported: FORMAT_VERSION,
            });
        }

        // Aplicamos migradores en cadena hasta llegar al formato actual.
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

            // Migramos cabecera y chunks.
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

/// Carga un mundo de disco y lo migra al formato actual. Es la funcion que usara
/// el motor al abrir un mundo.
pub fn load_and_migrate(path: &Path) -> Result<WorldSave, SaveError> {
    let save = WorldSave::load_from(path)?;
    if !save.header.is_valid() {
        return Err(SaveError::BadMagic(save.header.magic));
    }
    MigrationChain::with_builtins().migrate(save)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("solaria_test_{name}.vf"))
    }

    #[test]
    fn roundtrip_guardar_y_cargar() {
        let mut save = WorldSave::new(42, 1234);
        let mut column = Column::empty();
        column.set(2, 5, 3, Block::Stone);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column, 0));

        let path = temp_path("roundtrip");
        save.save_to(&path).unwrap();
        let loaded = WorldSave::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.header.seed, 42);
        assert!(loaded.header.is_valid());
        let record = loaded.chunks.get(&ChunkPos::new(0, 0)).unwrap();
        // El bloque de piedra debe seguir ahi (indice del chunk 0), tras
        // descomprimir.
        let blocks = record.decompressed_blocks();
        let idx = (5 * CHUNK_SIZE + 3) * CHUNK_SIZE + 2;
        assert_eq!(blocks[idx], Block::Stone.id());
        // Y el chunk deberia comprimir (terreno mayormente uniforme).
        assert!(
            record.compression_ratio() > 2.0,
            "ratio {}",
            record.compression_ratio()
        );
    }

    #[test]
    fn un_chunk_de_terreno_comprime_bien_con_lz4() {
        // Generamos un chunk de terreno real y medimos el ratio.
        let generator = crate::world::TerrainGenerator::new(13371);
        let column = generator.generate_column(0, 0);
        let record = ChunkRecord::from_column(&column, crate::world::store::TERRAIN_SECTION);
        assert!(record.compressed);
        let raw = 4096;
        let compressed = record.blocks.len();
        let ratio = record.compression_ratio();
        println!("[lz4] {raw} -> {compressed} bytes (x{ratio:.1})");
        // Deberia comprimir al menos 3x (hay mucho aire y zonas uniformes).
        assert!(
            ratio > 3.0,
            "ratio {ratio:.1} ({raw} -> {compressed} bytes)"
        );
        // Y descomprimir devuelve los 4096 bloques.
        assert_eq!(record.decompressed_blocks().len(), 4096);
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
        // Simulamos un mundo "v0" (un formato anterior) y comprobamos que la
        // cadena se niega si no hay migrador.
        let mut save = WorldSave::new(1, 0);
        save.header.format_version = 0;
        let err = MigrationChain::with_builtins()
            .migrate(save.clone())
            .unwrap_err();
        assert!(matches!(err, SaveError::NoMigration { .. }));

        // Con un migrador 0->1 deberia migrar.
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
        // Lo critico de una migracion: los bloques que el jugador rompio/coloco
        // NO se deben perder. Simulamos un mundo viejo con edits y migramos.
        let mut save = WorldSave::new(7, 999);
        save.header.format_version = 0;

        let mut column = Column::empty();
        column.set(1, 2, 3, Block::Torch); // algo que el jugador puso
        column.set(10, 4, 5, Block::Stone);
        // Emulamos un registro v1: bloques SIN comprimir.
        let mut original = ChunkRecord::from_column(&column, 0);
        original.blocks = original.decompressed_blocks();
        original.compressed = false;
        original.format_version = 1;
        save.set_chunk(ChunkPos::new(3, -2), original);

        // Migrador 0->1 de prueba: solo sube la version (los datos no cambian).
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
        // El builtin V1ToV2 se encarga de comprimir. La cadena completa es
        // 0 -> 1 -> 2 (FORMAT_VERSION).
        let mut chain = MigrationChain::with_builtins();
        chain.push(Box::new(V0ToV1));
        let migrated = chain.migrate(save).unwrap();

        let record = migrated.chunks.get(&ChunkPos::new(3, -2)).unwrap();
        let blocks = record.decompressed_blocks();
        // Los dos edits siguen ahi, en sus indices.
        let idx_torch = (2 * CHUNK_SIZE + 3) * CHUNK_SIZE + 1;
        let idx_stone = (4 * CHUNK_SIZE + 5) * CHUNK_SIZE + 10;
        assert_eq!(blocks[idx_torch], Block::Torch.id());
        assert_eq!(blocks[idx_stone], Block::Stone.id());
        // Y el chunk quedo marcado con la version nueva.
        assert_eq!(record.format_version, FORMAT_VERSION);
    }

    #[test]
    fn un_mundo_viejo_se_puede_guardar_y_recargar_tras_migrar() {
        // Punto a punto: guardar v0 en disco, migrar al cargar, y verificar que
        // el resultado se puede volver a guardar/cargar sin corromperse.
        let mut save = WorldSave::new(5, 1);
        save.header.format_version = 0;
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Wood);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column, 0));

        let path = temp_path("migrate_roundtrip");
        save.save_to(&path).unwrap();
        // load_from no migra; load_and_migrate si (pero usa la cadena estandar,
        // que no tiene 0->1). Cargamos y migramos a mano con el migrador.
        let loaded = WorldSave::load_from(&path).unwrap();
        let _ = std::fs::remove_file(&path);

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

        // Re-guardamos el mundo migrado y lo releemos: no debe fallar.
        let path2 = temp_path("migrate_roundtrip2");
        migrated.save_to(&path2).unwrap();
        let reopened = WorldSave::load_from(&path2).unwrap();
        let _ = std::fs::remove_file(&path2);
        assert_eq!(reopened.header.format_version, FORMAT_VERSION);
        assert!(reopened.chunks.contains_key(&ChunkPos::new(0, 0)));
    }
}
