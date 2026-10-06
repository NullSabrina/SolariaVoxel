//! Snapshot de una **seccion** para meshearla fuera del hilo principal.
//!
//! El mesher (greedy + fluido) solo necesita los bloques, la luz y el nivel de
//! agua de la seccion y de su **anillo de 1 bloque** (para caras de borde y
//! esquinas). `SectionSnapshot` copia ese volumen de `18x18x18` (coordenadas
//! locales `-1..=16` en X/Z y en Y) leyendo el mundo; es barato (lecturas
//! directas) y `Send`, asi el greedy corre en un worker sin tocar `wgpu`.

use crate::render::mesh::Vertex;

use super::block::Block;
use super::fluid_mesher;
use super::greedy;
use super::save::ChunkPos;
use super::store::World;

/// Lado del snapshot (`-1..=16` -> 18 posiciones por eje).
pub const SNAP: usize = 18;

/// Volumen de bloques/luz/agua alrededor de una seccion.
pub struct SectionSnapshot {
    blocks: Vec<Block>,
    sky: Vec<u8>,
    block_light: Vec<u8>,
    water: Vec<u8>,
}

impl SectionSnapshot {
    fn empty() -> Self {
        let n = SNAP * SNAP * SNAP;
        Self {
            blocks: vec![Block::Air; n],
            sky: vec![0; n],
            block_light: vec![0; n],
            water: vec![0; n],
        }
    }

    /// Indice del local `(lx, ly, lz)` con cada componente en `-1..=16`.
    #[inline]
    fn index(lx: i32, ly: i32, lz: i32) -> usize {
        let (x, y, z) = ((lx + 1) as usize, (ly + 1) as usize, (lz + 1) as usize);
        (y * SNAP + z) * SNAP + x
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn set(&mut self, lx: i32, ly: i32, lz: i32, b: Block, sky: u8, bl: u8, water: u8) {
        let i = Self::index(lx, ly, lz);
        self.blocks[i] = b;
        self.sky[i] = sky;
        self.block_light[i] = bl;
        self.water[i] = water;
    }

    #[inline]
    fn block(&self, lx: i32, ly: i32, lz: i32) -> Block {
        self.blocks[Self::index(lx, ly, lz)]
    }

    #[inline]
    fn light(&self, lx: i32, ly: i32, lz: i32) -> (u8, u8) {
        let i = Self::index(lx, ly, lz);
        (self.sky[i], self.block_light[i])
    }

    #[inline]
    fn water(&self, lx: i32, ly: i32, lz: i32) -> u8 {
        self.water[Self::index(lx, ly, lz)]
    }
}

/// Copia del volumen `18x18x18` alrededor de la seccion `section` de la columna
/// `pos`, leyendo del mundo (fuera de lo cargado -> aire/0).
pub fn section_snapshot(world: &World, pos: ChunkPos, section: usize) -> SectionSnapshot {
    let bx = pos.x * super::chunk::CHUNK_SIZE as i32;
    let bz = pos.z * super::chunk::CHUNK_SIZE as i32;
    let by = section as i32 * super::chunk::CHUNK_SIZE as i32;
    let mut snap = SectionSnapshot::empty();
    for ly in -1..=16 {
        for lz in -1..=16 {
            for lx in -1..=16 {
                let w = [bx + lx, by + ly, bz + lz];
                snap.set(
                    lx,
                    ly,
                    lz,
                    world.get_block(w),
                    world.sky_light_at(w),
                    world.block_light_at(w),
                    world.water_level(w),
                );
            }
        }
    }
    snap
}

/// Genera la geometria CPU de una seccion a partir del snapshot.
///
/// Devuelve `(opaco_v, opaco_i, agua_v, agua_i)`. Es puro: no toca `wgpu`, asi
/// que puede ejecutarse en un worker.
pub fn mesh_snapshot(
    snap: &SectionSnapshot,
    section: usize,
    origin: [f32; 3],
) -> (Vec<Vertex>, Vec<u32>, Vec<Vertex>, Vec<u32>) {
    let by = section as i32 * super::chunk::CHUNK_SIZE as i32;
    // Greedy usa X/Z locales de columna y **Y global**; lo traducimos a las
    // coordenadas locales del snapshot (`ly = y - by`).
    let query = |x: i32, y: i32, z: i32| -> Block { snap.block(x, y - by, z) };
    let light = |x: i32, y: i32, z: i32| -> (u8, u8) { snap.light(x, y - by, z) };
    let level = |x: i32, y: i32, z: i32| -> u8 { snap.water(x, y - by, z) };

    // El agua del greedy se ignora: la dibuja `fluid_mesher` (rampa continua).
    let (v, i, _, _) = greedy::greedy_section_query(&query, &light, section, origin);
    let (fwv, fwi) = fluid_mesher::fluid_section(&level, &query, &light, section, origin);
    (v, i, fwv, fwi)
}
