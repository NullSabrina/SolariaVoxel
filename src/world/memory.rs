//! Contabilidad de memoria del mundo, **por categorias**.
//!
//! FASE 10 de la auditoria: la regla es **medir antes de cambiar la
//! representacion**. Este modulo define el informe; lo rellena
//! [`super::store::World::memory_report`] (que tiene acceso a las columnas).
//!
//! Solo cuenta lo que el `World` **posee** en CPU. Los buffers de GPU viven en
//! `render` y se reportan aparte; tampoco cuenta la memoria de pila transitoria.
//!
//! Categorias:
//! * `blocks_bytes` — los voxeles (`16^3` por seccion x 24 secciones x columna).
//! * `skylight_bytes` — luz de cielo (`u8` por celda, siempre reservada).
//! * `blocklight_bytes` — luz de bloque (dispersa: 0 sin emisores).
//! * `fluid_bytes` — niveles de flujo de agua (dispersa).
//! * `struct_overhead_bytes` — cabeceras `Vec`/`Option`, `surface` y el puntero
//!   del `Box<Column>` (lo que no son datos de voxel/luz/fluido).
//! * `modified_bytes` — registros de columnas editadas (payload LZ4).
//! * `water_queue_cells` — active set de agua (no bytes, pero da escala).

use std::mem::size_of;

use super::save::ChunkRecord;

/// Memoria del mundo en CPU, por categoria (bytes ya multiplicados por columna).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldMemory {
    /// Columnas cargadas.
    pub columns: usize,
    /// Voxeles (`1 byte` por celda).
    pub blocks_bytes: usize,
    /// Luz de cielo.
    pub skylight_bytes: usize,
    /// Luz de bloque (0 si ninguna columna tiene emisores).
    pub blocklight_bytes: usize,
    /// Niveles de flujo de agua.
    pub fluid_bytes: usize,
    /// Cabeceras y campos que no son voxeles/luz/fluido.
    pub struct_overhead_bytes: usize,
    /// Columnas editadas persistidas.
    pub modified_chunks: usize,
    /// Payload de esos registros.
    pub modified_bytes: usize,
    /// Celdas de agua en el active set.
    pub water_queue_cells: usize,
}

impl WorldMemory {
    /// Suma de todas las categorias.
    pub fn total_bytes(&self) -> usize {
        self.blocks_bytes
            + self.skylight_bytes
            + self.blocklight_bytes
            + self.fluid_bytes
            + self.struct_overhead_bytes
            + self.modified_bytes
    }
}

/// Convierte bytes a mebibytes (para trazas legibles).
pub fn mib(bytes: usize) -> f32 {
    bytes as f32 / (1024.0 * 1024.0)
}

/// Tamano del payload de un registro persistente (bloques + fluido comprimidos
/// mas la cabecera del struct).
pub fn record_bytes(record: &ChunkRecord) -> usize {
    size_of::<ChunkRecord>() + record.blocks.len() + record.fluid.len()
}
