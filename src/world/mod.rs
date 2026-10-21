//! # Modulo `world` — el mundo de voxeles
//!
//! Responsabilidades y limites:
//!
//! * **Datos**: [`block`]/[`registry`] (ids y metadata), [`chunk`] (columnas
//!   16x16x384 con luz y flujo), [`atlas`] (texturas).
//! * **Generacion**: [`terrain`] y [`worldgen`] (legacy16, graph y larion),
//!   [`caves`] y [`streaming`]/[`store`] para cargar y descargar columnas.
//! * **Geometria**: [`greedy`]/[`mesher`] para solidos, [`fluid_mesher`] para el
//!   agua y [`mesh_snapshot`] para meshear en hilos.
//! * **Simulacion**: [`water`] (modelo fuente -> distancia) y [`physics`] de
//!   entidades fuera de este modulo.
//! * **Persistencia**: [`save`] (formato binario versionado, migradores y
//!   recuperacion) y [`library`] (mundos multiples con `level.json`).
//!
//! Invariantes clave: el generador es determinista por `(seed, x, z)`; una
//! columna es la unidad de carga/guardado; fuera del radio cargado no se
//! simula agua ni se escribe. Ver `ARCHITECTURE.md`.

pub mod atlas;
#[cfg(test)]
pub mod bench;
pub mod block;
pub mod caves;
pub mod chunk;
pub mod fluid_mesher;
pub mod generator;
pub mod greedy;
pub mod item;
pub mod library;
pub mod memory;
pub mod mesh_snapshot;
pub mod mesher;
pub mod raycast;
pub mod recipe;
pub mod registry;
pub mod save;
pub mod store;
pub mod streaming;
pub mod terrain;
pub mod view;
pub mod water;
pub mod worldgen;

pub use block::{Block, Face};
pub use chunk::{CHUNK_SIZE, CHUNK_VOLUME, Chunk, Column, SECTION_COUNT, WORLD_HEIGHT};
pub use generator::GeneratorKind;
pub use item::ItemStack;
pub use library::{WorldEntry, WorldMeta, seed_from_text};
pub use memory::WorldMemory;
pub use mesher::{SectionMesh, mesh_column, mesh_section};
pub use raycast::{RayHit, raycast};
pub use recipe::{RECIPES, Recipe, match_recipe};
pub use registry::{BlockDefinition, BlockRegistry, CreativeCategory, FluidKind, RenderKind};
pub use save::{
    ChunkPos, ChunkRecord, FORMAT_VERSION, GENERATOR_VERSION, MigrationChain, SaveError,
    WorldHeader, WorldMigrator, WorldSave, load_and_migrate,
};
pub use store::{FluidDirty, StreamChange, VoxelAvailability, World};
pub use terrain::{Biome, SEA_LEVEL, TerrainGenerator};
pub use view::{FogMode, ViewSettings};
pub use water::{Fluid, FluidBudget, MAX_LEVEL};
