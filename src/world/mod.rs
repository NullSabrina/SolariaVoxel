//! # Modulo `world` — el mundo de voxeles
//!
//! Un mundo de voxeles es, en el fondo, una rejilla 3D de bloques. Aqui vive:
//!
//! * [`block`] — que tipos de bloque existen y como se texturizan.
//! * [`chunk`] — un trozo cubico de mundo de 16x16x16 bloques.
//! * [`atlas`] — la textura que contiene todos los bloques y su layout.
//! * [`mesher`] — convierte un chunk en geometria (vertices + indices).
//!
//! De momento hay UN chunk estatico en el origen. En v0.3.x llegaran los mundos
//! de varios chunks y la generacion procedural de verdad.

pub mod atlas;
#[cfg(test)]
pub mod bench;
pub mod block;
pub mod caves;
pub mod chunk;
pub mod fluid_mesher;
pub mod greedy;
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
