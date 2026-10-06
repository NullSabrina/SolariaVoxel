//! Tipos de bloque y como se mapean a la textura.
//!
//! Un bloque es un `enum` pequenito. Guardar bloques como `u8` (en lugar de un
//! struct grande por voxel) es clave para el objetivo de memoria: un chunk de
//! 16^3 ocupa 4096 bytes, 1 byte por bloque.
//!
//! Toda la **metadata** (solidez, liquido, emision, tiles, si es item...) no
//! vive aqui, sino en el **registro central** [`super::registry`]. `Block` solo
//! aporta el `id` y delega. Asi la definicion de un bloque esta en un unico
//! sitio y no puede desincronizarse del inventario o del atlas.

use super::registry::{self, FluidKind, RenderKind};

/// Las 6 caras de un cubo, en coordenadas del mundo.
///
/// El nombre es el eje y el signo: `PosX` es la cara que mira hacia +X.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Face {
    /// Las 6 caras, comodas para iterar.
    pub const ALL: [Face; 6] = [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ];

    /// Desplazamiento (en voxeles) del vecino que hay al otro lado de la cara.
    pub fn offset(self) -> (i32, i32, i32) {
        match self {
            Face::PosX => (1, 0, 0),
            Face::NegX => (-1, 0, 0),
            Face::PosY => (0, 1, 0),
            Face::NegY => (0, -1, 0),
            Face::PosZ => (0, 0, 1),
            Face::NegZ => (0, 0, -1),
        }
    }

    /// Indice de la cara en las tablas del registro (`[PosX, NegX, PosY, NegY,
    /// PosZ, NegZ]`). Es el orden de `BlockDefinition::face_tiles`.
    #[inline]
    pub fn index(self) -> usize {
        match self {
            Face::PosX => 0,
            Face::NegX => 1,
            Face::PosY => 2,
            Face::NegY => 3,
            Face::PosZ => 4,
            Face::NegZ => 5,
        }
    }
}

/// Tipo de bloque. El valor numerico (`u8`) es lo que se guarda en el chunk.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Block {
    /// Aire: no es solido, no se dibuja.
    Air = 0,
    Grass,
    Dirt,
    Stone,
    Sand,
    Wood,
    Leaves,
    /// Antorcha: no es solida (se puede atravesar) pero **emite luz**.
    Torch,
    /// Nieve: superficie de los biomas frios.
    Snow,
    /// Agua: no es solida (se nada/se atraviesa) y **translucida**.
    Water,
    /// Tablones de madera (para crafteo en la etapa 2).
    Planks,
    /// Mesa de crafteo: click derecho sobre ella abre la interfaz de crafteo.
    CraftingTable,
    /// Tierra gruesa: superficie de sabana y bosque sin vegetacion densa.
    CoarseDirt,
    /// Grava: lechos de rio y laderas de montana.
    Gravel,
    /// Podzol: suelo acido de taiga/bosque humedo (capa superior).
    Podzol,
    /// Lava: liquido **estatico** que brilla (no entra en el sim de agua).
    /// Nada en ella (flota igual que en agua) pero no hace dano todavia.
    Lava,
    /// Obsidiana: roca formada bajo las pozas de lava; dura y oscura.
    Obsidian,
}

impl Default for Block {
    /// El bloque que se coloca al empezar (piedra).
    fn default() -> Self {
        Block::Stone
    }
}

impl Block {
    /// Reconstruye un bloque a partir del `u8` guardado. Cualquier valor
    /// desconocido se interpreta como aire (tolerancia hacia adelante). El
    /// mapeo vive en el registro ([`registry::block_from_id`]).
    pub fn from_u8(value: u8) -> Self {
        registry::block_from_id(value)
    }

    /// El `u8` que se guarda en el chunk.
    #[inline]
    pub fn id(self) -> u8 {
        self as u8
    }

    /// Nombre legible del bloque (diagnostico/UI).
    #[inline]
    pub fn name(self) -> &'static str {
        registry::definition(self).name
    }

    /// ¿El id corresponde a un bloque conocido por esta version del motor?
    ///
    /// Los ids son contiguos; un id mayor es de una version futura/mod. Sirve
    /// para **no cargar en silencio** un mundo con bloques desconocidos (se
    /// rechaza con error en vez de convertirlos a aire y destruir datos).
    #[inline]
    pub fn is_known_id(value: u8) -> bool {
        registry::is_known_id(value)
    }

    /// ¿Ocupa espacio? (no bloquean: aire, antorcha, liquidos y **hojas**, que
    /// son transparentes y se atraviesan).
    #[inline]
    pub fn is_solid(self) -> bool {
        registry::definition(self).solid
    }

    /// ¿Es un bloque que se dibuja pero no bloquea? (antorcha, liquidos y hojas).
    #[inline]
    pub fn is_visible(self) -> bool {
        registry::definition(self).visible
    }

    /// ¿Es un liquido? (para la fisica de nado y el render translucido).
    #[inline]
    pub fn is_liquid(self) -> bool {
        registry::definition(self).fluid.is_liquid()
    }

    /// ¿Bloquea el paso del **agua** en la simulacion? Los solidos y la lava
    /// (el agua no fluye dentro de la lava; la reaccion agua+lava queda para
    /// mas adelante).
    #[inline]
    pub fn blocks_fluid(self) -> bool {
        let def = registry::definition(self);
        def.solid || def.fluid == FluidKind::Lava
    }

    /// Luz que **emite** el bloque (0..15). La antorcha emite 14, la lava 15.
    #[inline]
    pub fn light_emission(self) -> u8 {
        registry::definition(self).light_emission
    }

    /// Pipeline de dibujo que le corresponde (opaco / cutout / liquido).
    #[inline]
    pub fn render_kind(self) -> RenderKind {
        registry::definition(self).render
    }

    /// Dureza relativa. **Reservado** para los tiempos de minado (aun no usada).
    #[inline]
    pub fn hardness(self) -> f32 {
        registry::definition(self).hardness
    }

    /// Que tile del atlas usa cada cara de este bloque.
    ///
    /// El atlas se describe en [`crate::world::atlas`]; los tiles concretos, en
    /// el registro. El pasto, por ejemplo, usa verde arriba, tierra abajo y una
    /// cara lateral mixta.
    #[inline]
    pub fn face_tile(self, face: Face) -> u16 {
        registry::definition(self).face_tiles[face.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_ida_y_vuelta() {
        for b in [
            Block::Air,
            Block::Grass,
            Block::Stone,
            Block::Wood,
            Block::Water,
            Block::Planks,
            Block::CraftingTable,
            Block::Lava,
            Block::Obsidian,
        ] {
            assert_eq!(Block::from_u8(b.id()), b);
        }
    }

    #[test]
    fn valor_desconocido_es_aire() {
        assert_eq!(Block::from_u8(200), Block::Air);
    }

    #[test]
    fn no_bloquean_aire_antorcha_agua_y_hojas() {
        assert!(!Block::Air.is_solid());
        assert!(!Block::Torch.is_solid());
        assert!(!Block::Water.is_solid());
        assert!(!Block::Leaves.is_solid());
        assert!(Block::Leaves.is_visible());
        assert!(Block::Grass.is_solid());
    }

    #[test]
    fn la_antorcha_es_visible_pero_no_solida() {
        assert!(!Block::Torch.is_solid());
        assert!(Block::Torch.is_visible());
        // Y emite luz, que es su razon de ser.
        assert_eq!(Block::Torch.light_emission(), 14);
        // Los solidos no son "visibles no solidos".
        assert!(!Block::Stone.is_visible());
    }

    #[test]
    fn el_agua_es_visible_no_solida_y_liquida() {
        assert!(!Block::Water.is_solid());
        assert!(Block::Water.is_visible());
        assert!(Block::Water.is_liquid());
        assert_eq!(Block::Water.face_tile(Face::PosY), 10);
    }

    #[test]
    fn face_tile_es_coherente_en_todas_las_caras() {
        // Cada bloque solido debe devolver un tile valido en sus 6 caras; la
        // hierba y la madera distinguen arriba/abajo del resto.
        for b in [
            Block::Grass,
            Block::Dirt,
            Block::Stone,
            Block::Sand,
            Block::Wood,
            Block::Leaves,
            Block::Snow,
            Block::Water,
            Block::Planks,
            Block::CraftingTable,
            Block::CoarseDirt,
            Block::Gravel,
            Block::Podzol,
            Block::Lava,
            Block::Obsidian,
        ] {
            for face in Face::ALL {
                assert!(b.face_tile(face) < registry::TILE_COUNT, "{b:?} {face:?}");
            }
        }
        // Hierba: verde arriba, tierra abajo, lateral distinto.
        assert_eq!(Block::Grass.face_tile(Face::PosY), 0);
        assert_eq!(Block::Grass.face_tile(Face::NegY), 2);
        assert_eq!(Block::Grass.face_tile(Face::PosX), 1);
        // Madera: anillos arriba/abajo, corteza en los lados.
        assert_eq!(Block::Wood.face_tile(Face::PosY), 6);
        assert_eq!(Block::Wood.face_tile(Face::NegY), 6);
        assert_eq!(Block::Wood.face_tile(Face::PosZ), 5);
        // La antorcha usa siempre el tile 8.
        for face in Face::ALL {
            assert_eq!(Block::Torch.face_tile(face), 8);
        }
        // La nieve usa siempre el tile 9 y es solida.
        assert!(Block::Snow.is_solid());
        for face in Face::ALL {
            assert_eq!(Block::Snow.face_tile(face), 9);
        }
        // La mesa: tapa 13, base 11, lateral 12; solida.
        assert!(Block::CraftingTable.is_solid());
        assert_eq!(Block::CraftingTable.face_tile(Face::PosY), 13);
        assert_eq!(Block::CraftingTable.face_tile(Face::NegY), 11);
        assert_eq!(Block::CraftingTable.face_tile(Face::PosX), 12);
        // La lava es liquida visible que emite 15 y bloquea el flujo de agua.
        assert!(Block::Lava.is_liquid());
        assert!(Block::Lava.is_visible());
        assert!(!Block::Lava.is_solid());
        assert_eq!(Block::Lava.light_emission(), 15);
        assert!(Block::Lava.blocks_fluid());
        assert_eq!(Block::Lava.face_tile(Face::PosY), 17);
        // La obsidiana es solida y no emite.
        assert!(Block::Obsidian.is_solid());
        assert_eq!(Block::Obsidian.light_emission(), 0);
        assert_eq!(Block::Obsidian.face_tile(Face::PosY), 18);
    }
}
