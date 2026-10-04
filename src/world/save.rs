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
pub const FORMAT_VERSION: u32 = 1;

/// Version actual del generador de terreno.
pub const GENERATOR_VERSION: u32 = 1;

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
/// Guardamos el chunk **completo** (4096 `u8`) en lugar de solo el "diff".
/// Gastamos mas, pero es simple y robusto; el hermano optimizado (paleta +
/// compresion LZ4) llega en v0.5.2/v0.11.0.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ChunkRecord {
    /// Version del formato de ESTE chunk (permite migrar chunk a chunk).
    pub format_version: u32,
    /// Version del generador con el que se genero su terreno base.
    pub generator_version: u32,
    /// Los 4096 bloques.
    pub blocks: Vec<u8>,
}

impl ChunkRecord {
    /// Construye el registro a partir de una columna (solo su chunk `chunk_y`
    /// vertical; de momento guardamos el chunk que contiene el terreno).
    pub fn from_column(column: &Column, chunk_y: usize) -> Self {
        let mut blocks = Vec::with_capacity(CHUNK_VOLUME);
        let y0 = chunk_y * CHUNK_SIZE;
        for y in y0..(y0 + CHUNK_SIZE).min(WORLD_HEIGHT) {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    blocks.push(column.get(x, y, z).id());
                }
            }
        }
        Self {
            format_version: FORMAT_VERSION,
            generator_version: GENERATOR_VERSION,
            blocks,
        }
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

/// Cadena de migradores: los aplica en orden hasta llegar al formato actual.
#[derive(Default)]
pub struct MigrationChain {
    migrators: Vec<Box<dyn WorldMigrator>>,
}

impl MigrationChain {
    /// Crea la cadena con los migradores conocidos por el motor.
    pub fn with_builtins() -> Self {
        // Aun no hay migradores reales (solo existe el formato v1). Cuando
        // subamos a v2, se registra aqui el `V1ToV2`.
        Self {
            migrators: Vec::new(),
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
        // El bloque de piedra debe seguir ahi (indice del chunk 0).
        let idx = (5 * CHUNK_SIZE + 3) * CHUNK_SIZE + 2;
        assert_eq!(record.blocks[idx], Block::Stone.id());
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
}
