//! **Registro central de bloques**: la unica fuente de verdad de la metadata de
//! cada bloque.
//!
//! Antes esa metadata estaba repartida en tres sitios que podian
//! desincronizarse:
//! * `block.rs` — que es solido/visible/liquido, cuanto emite y que tile usa
//!   cada cara.
//! * `app.rs` (`ITEMS`) — la lista y el orden de los bloques colocables.
//! * `atlas.rs` (`TILES`) — cuantos tiles hay.
//!
//! Aqui se centraliza en [`BlockDefinition`] + la tabla [`BLOCKS`]. El voxel
//! sigue siendo un `u8` (`Block`): la metadata es **externa** y de solo lectura,
//! asi que no engorda el guardado ni la memoria del mundo. `Block` delega sus
//! consultas (`is_solid`, `face_tile`, ...) en esta tabla.
//!
//! Al anadir un id hay que:
//! 1. anadir la variante al `enum Block` (con su discriminante contiguo),
//! 2. anadirla a `ALL_BLOCKS` y su `BlockDefinition` a `BLOCKS`,
//! 3. si usa un tile nuevo, pintarlo en `atlas.rs` y ampliar `tile_color`.
//!
//! Los tests de este modulo fallan si algo de eso queda desincronizado.

use super::block::Block;

/// Tile del atlas por cara, empaquetado como `[PosX, NegX, PosY, NegY, PosZ,
/// NegZ]` (el orden de `Face::index`).
type FaceTiles = [u16; 6];

/// Como debe dibujarse (y ocluir) el bloque. Describe el **pipeline** que le
/// corresponde; `solid`/`visible` siguen siendo los flags de colision/oclusion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderKind {
    /// No se dibuja (aire).
    None,
    /// Solido opaco: ocluye las caras vecinas.
    Opaque,
    /// Visible pero no solido y con transparencia recortada (hojas, antorcha).
    Cutout,
    /// Liquido translucido (agua, lava): pase aparte con blending.
    Liquid,
}

/// Fluido que representa un bloque, si es liquido.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FluidKind {
    None,
    Water,
    Lava,
}

/// Categoria del **inventario creativo** (pestanas). Se deriva del bloque (no hay
/// campo en la tabla) para no tocar las 17 definiciones.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreativeCategory {
    /// Construccion: piedra, tablones, obsidiana, mesa.
    Building,
    /// Naturaleza: tierras, arena, grava, nieve, madera, hojas.
    Nature,
    /// Decoracion/luz: antorcha.
    Decoration,
    /// Especiales: liquidos.
    Special,
}

impl CreativeCategory {
    /// Todas las categorias, en orden de pestanas.
    pub const ALL: [CreativeCategory; 4] = [
        CreativeCategory::Building,
        CreativeCategory::Nature,
        CreativeCategory::Decoration,
        CreativeCategory::Special,
    ];

    /// Clave i18n del nombre de la pestana (p.ej. `cat.building`).
    pub fn key(self) -> &'static str {
        match self {
            CreativeCategory::Building => "cat.building",
            CreativeCategory::Nature => "cat.nature",
            CreativeCategory::Decoration => "cat.decoration",
            CreativeCategory::Special => "cat.special",
        }
    }
}

impl FluidKind {
    /// ¿Es un liquido?
    #[inline]
    pub fn is_liquid(self) -> bool {
        !matches!(self, FluidKind::None)
    }
}

/// Metadata de un tipo de bloque. Es `Copy` y vive en una tabla estatica: no se
/// guarda por voxel.
#[derive(Clone, Copy, Debug)]
pub struct BlockDefinition {
    /// Id numerico (debe coincidir con su indice en [`BLOCKS`]).
    pub id: u8,
    /// Nombre legible (diagnostico/UI).
    pub name: &'static str,
    /// Tile del atlas por cara.
    pub face_tiles: FaceTiles,
    /// ¿Ocupa espacio (colisiona, ocluye)? (aire, antorcha, liquidos y hojas no).
    pub solid: bool,
    /// ¿Se dibuja sin ser solido? (antorcha, liquidos, hojas).
    pub visible: bool,
    /// Pipeline de dibujo.
    pub render: RenderKind,
    /// Fluido que representa (o `None`).
    pub fluid: FluidKind,
    /// Luz que emite (0..15).
    pub light_emission: u8,
    /// ¿Aparece en el inventario/hotbar para colocarlo?
    pub item: bool,
    /// Dureza relativa. **Reservado**: aun no se usa (no hay tiempos de minado).
    pub hardness: f32,
}

/// Todos los bloques conocidos, en orden de id. Es la lista canonica: `from_u8`
/// y `is_known_id` se derivan de su longitud, no de un `match` aparte.
pub const ALL_BLOCKS: [Block; COUNT] = [
    Block::Air,
    Block::Grass,
    Block::Dirt,
    Block::Stone,
    Block::Sand,
    Block::Wood,
    Block::Leaves,
    Block::Torch,
    Block::Snow,
    Block::Water,
    Block::Planks,
    Block::CraftingTable,
    Block::CoarseDirt,
    Block::Gravel,
    Block::Podzol,
    Block::Lava,
    Block::Obsidian,
];

/// Numero de bloques conocidos (ids `0..COUNT`).
pub const COUNT: usize = 17;

/// Constructor de comodidad: el mismo tile en las 6 caras.
const fn all(tile: u16) -> FaceTiles {
    [tile; 6]
}

/// Tabla de definiciones, indexada por id. El orden **debe** coincidir con
/// [`ALL_BLOCKS`] (un test lo comprueba).
pub const BLOCKS: [BlockDefinition; COUNT] = [
    BlockDefinition {
        id: 0,
        name: "air",
        face_tiles: all(0),
        solid: false,
        visible: false,
        render: RenderKind::None,
        fluid: FluidKind::None,
        light_emission: 0,
        item: false,
        hardness: 0.0,
    },
    BlockDefinition {
        id: 1,
        name: "grass",
        // Lateral 1, arriba 0 (verde), abajo 2 (tierra).
        face_tiles: [1, 1, 0, 2, 1, 1],
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.6,
    },
    BlockDefinition {
        id: 2,
        name: "dirt",
        face_tiles: all(2),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.5,
    },
    BlockDefinition {
        id: 3,
        name: "stone",
        face_tiles: all(3),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 1.5,
    },
    BlockDefinition {
        id: 4,
        name: "sand",
        face_tiles: all(4),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.5,
    },
    BlockDefinition {
        id: 5,
        name: "wood",
        // Corteza 5 en los lados, anillos 6 arriba/abajo.
        face_tiles: [5, 5, 6, 6, 5, 5],
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 2.0,
    },
    BlockDefinition {
        id: 6,
        name: "leaves",
        face_tiles: all(7),
        solid: false,
        visible: true,
        render: RenderKind::Cutout,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.2,
    },
    BlockDefinition {
        id: 7,
        name: "torch",
        face_tiles: all(8),
        solid: false,
        visible: true,
        render: RenderKind::Cutout,
        fluid: FluidKind::None,
        light_emission: 14,
        item: true,
        hardness: 0.0,
    },
    BlockDefinition {
        id: 8,
        name: "snow",
        face_tiles: all(9),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.4,
    },
    BlockDefinition {
        id: 9,
        name: "water",
        face_tiles: all(10),
        solid: false,
        visible: true,
        render: RenderKind::Liquid,
        fluid: FluidKind::Water,
        light_emission: 0,
        item: true,
        hardness: 0.0,
    },
    BlockDefinition {
        id: 10,
        name: "planks",
        face_tiles: all(11),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 2.0,
    },
    BlockDefinition {
        id: 11,
        name: "crafting_table",
        // Tapa 13, base de tablones 11, lateral 12.
        face_tiles: [12, 12, 13, 11, 12, 12],
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 2.5,
    },
    BlockDefinition {
        id: 12,
        name: "coarse_dirt",
        face_tiles: all(14),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.5,
    },
    BlockDefinition {
        id: 13,
        name: "gravel",
        face_tiles: all(15),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.6,
    },
    BlockDefinition {
        id: 14,
        name: "podzol",
        // Capa superior 16; el resto, como la tierra (2).
        face_tiles: [2, 2, 16, 2, 2, 2],
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 0.6,
    },
    BlockDefinition {
        id: 15,
        name: "lava",
        face_tiles: all(17),
        solid: false,
        visible: true,
        render: RenderKind::Liquid,
        fluid: FluidKind::Lava,
        light_emission: 15,
        item: true,
        hardness: 0.0,
    },
    BlockDefinition {
        id: 16,
        name: "obsidian",
        face_tiles: all(18),
        solid: true,
        visible: false,
        render: RenderKind::Opaque,
        fluid: FluidKind::None,
        light_emission: 0,
        item: true,
        hardness: 10.0,
    },
];

/// Fachada publica del **registro de bloques**: el punto de entrada unico para
/// quien necesite su metadata (inventario, diagnosticos, tests). Hoy es una
/// tabla estatica; en el futuro albergara un registro cargable (datapacks) sin
/// cambiar a los consumidores.
pub struct BlockRegistry;

impl BlockRegistry {
    /// Definicion de un bloque.
    #[inline]
    pub fn block(block: Block) -> &'static BlockDefinition {
        definition(block)
    }

    /// Bloque por id (aire si el id es desconocido).
    #[inline]
    pub fn by_id(id: u8) -> Block {
        block_from_id(id)
    }

    /// ¿Id conocido por esta version?
    #[inline]
    pub fn is_known_id(id: u8) -> bool {
        is_known_id(id)
    }

    /// Bloques colocables, en orden de inventario.
    #[inline]
    pub fn items() -> &'static [Block] {
        PLACEABLE_ITEMS
    }

    /// Cuantos bloques conoce el registro.
    #[inline]
    pub fn count() -> usize {
        COUNT
    }

    /// Categoria de inventario creativo de un bloque.
    #[inline]
    pub fn category(block: Block) -> CreativeCategory {
        creative_category(block)
    }

    /// Bloques colocables de una categoria, en orden de inventario.
    pub fn items_in(cat: CreativeCategory) -> Vec<Block> {
        PLACEABLE_ITEMS
            .iter()
            .copied()
            .filter(|b| creative_category(*b) == cat)
            .collect()
    }
}

/// Definicion de un bloque. Nunca falla: `Block` siempre tiene una entrada.
#[inline]
pub fn definition(block: Block) -> &'static BlockDefinition {
    &BLOCKS[block.id() as usize]
}

/// Categoria del inventario creativo de un bloque.
pub fn creative_category(block: Block) -> CreativeCategory {
    match block {
        Block::Stone | Block::Planks | Block::Obsidian | Block::CraftingTable => {
            CreativeCategory::Building
        }
        Block::Grass
        | Block::Dirt
        | Block::CoarseDirt
        | Block::Podzol
        | Block::Sand
        | Block::Gravel
        | Block::Snow
        | Block::Wood
        | Block::Leaves => CreativeCategory::Nature,
        Block::Torch => CreativeCategory::Decoration,
        Block::Water | Block::Lava => CreativeCategory::Special,
        Block::Air => CreativeCategory::Building,
    }
}

/// Reconstruye un bloque desde su id. Cualquier id fuera de rango es aire
/// (tolerancia hacia adelante; la validacion real la hace `ChunkRecord`).
#[inline]
pub fn block_from_id(id: u8) -> Block {
    ALL_BLOCKS.get(id as usize).copied().unwrap_or(Block::Air)
}

/// ¿El id corresponde a un bloque conocido?
#[inline]
pub fn is_known_id(id: u8) -> bool {
    (id as usize) < ALL_BLOCKS.len()
}

/// Bloques colocables, en el **orden del inventario** (los 9 primeros van a la
/// hotbar por defecto). Es la unica lista; `app.rs` la consume desde aqui.
pub const PLACEABLE_ITEMS: &[Block] = &[
    Block::Grass,
    Block::Dirt,
    Block::CoarseDirt,
    Block::Podzol,
    Block::Sand,
    Block::Gravel,
    Block::Stone,
    Block::Obsidian,
    Block::Snow,
    Block::Wood,
    Block::Planks,
    Block::Leaves,
    Block::CraftingTable,
    Block::Torch,
    Block::Water,
    Block::Lava,
];

/// Cuenta de tiles del atlas: el tile mas alto usado por cualquier cara, mas 1.
/// `atlas::TILES` se deriva de aqui para que no se desincronicen.
pub const TILE_COUNT: u16 = tile_count(&BLOCKS);

const fn tile_count(blocks: &[BlockDefinition]) -> u16 {
    let mut max = 0u16;
    let mut i = 0;
    while i < blocks.len() {
        let mut f = 0;
        while f < 6 {
            let t = blocks[i].face_tiles[f];
            if t > max {
                max = t;
            }
            f += 1;
        }
        i += 1;
    }
    max + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Face;

    #[test]
    fn los_ids_son_contiguos_y_coinciden_con_su_indice() {
        assert_eq!(ALL_BLOCKS.len(), COUNT);
        assert_eq!(BLOCKS.len(), COUNT);
        for (i, def) in BLOCKS.iter().enumerate() {
            assert_eq!(def.id as usize, i, "definicion {i} con id {}", def.id);
            assert_eq!(
                definition(ALL_BLOCKS[i]).id as usize,
                i,
                "ALL_BLOCKS[{i}] no coincide con BLOCKS"
            );
            assert_eq!(ALL_BLOCKS[i].id() as usize, i);
        }
    }

    #[test]
    fn known_id_cubre_todos_los_bloques_y_rechaza_lo_desconocido() {
        for b in ALL_BLOCKS {
            assert!(is_known_id(b.id()), "{b:?} deberia ser conocido");
            assert_eq!(block_from_id(b.id()), b);
        }
        assert!(!is_known_id(COUNT as u8));
        assert_eq!(block_from_id(200), Block::Air);
    }

    #[test]
    fn los_tiles_caben_en_el_atlas() {
        for def in &BLOCKS {
            for &t in &def.face_tiles {
                assert!(t < TILE_COUNT, "{} usa tile {t} >= {TILE_COUNT}", def.name);
            }
        }
        // El tile mas alto vale TILE_COUNT - 1 (no sobra espacio muerto).
        assert_eq!(TILE_COUNT, 19);
    }

    #[test]
    fn los_colocables_son_exactamente_los_items_y_en_orden() {
        // Primero los 9 de la hotbar por defecto.
        assert_eq!(
            &PLACEABLE_ITEMS[..9],
            &[
                Block::Grass,
                Block::Dirt,
                Block::CoarseDirt,
                Block::Podzol,
                Block::Sand,
                Block::Gravel,
                Block::Stone,
                Block::Obsidian,
                Block::Snow,
            ]
        );
        // El conjunto coincide exactamente con los bloques marcados `item`.
        let mut expected: Vec<Block> = ALL_BLOCKS
            .iter()
            .copied()
            .filter(|b| definition(*b).item)
            .collect();
        let mut got: Vec<Block> = PLACEABLE_ITEMS.to_vec();
        expected.sort();
        got.sort();
        assert_eq!(got, expected, "colocables != bloques `item`");
        // Todos los bloques salvo el aire son colocables.
        assert!(
            ALL_BLOCKS
                .iter()
                .all(|b| b.id() == 0 || definition(*b).item)
        );
    }

    #[test]
    fn las_propiedades_historicas_se_conservan() {
        assert!(Block::Grass.is_solid());
        assert!(!Block::Torch.is_solid());
        assert!(Block::Torch.is_visible());
        assert_eq!(Block::Torch.light_emission(), 14);
        assert!(!Block::Leaves.is_solid());
        assert!(Block::Leaves.is_visible());
        assert!(Block::Water.is_liquid());
        assert!(Block::Lava.is_liquid());
        assert!(Block::Lava.blocks_fluid());
        assert_eq!(Block::Lava.light_emission(), 15);
        assert!(Block::Stone.blocks_fluid());
        assert_eq!(Block::Obsidian.light_emission(), 0);
        // Caras especiales.
        assert_eq!(Block::Grass.face_tile(Face::PosY), 0);
        assert_eq!(Block::Grass.face_tile(Face::NegY), 2);
        assert_eq!(Block::Grass.face_tile(Face::PosX), 1);
        assert_eq!(Block::Wood.face_tile(Face::PosY), 6);
        assert_eq!(Block::CraftingTable.face_tile(Face::NegY), 11);
        assert_eq!(Block::Podzol.face_tile(Face::PosY), 16);
    }

    #[test]
    fn el_render_kind_es_coherente_con_los_flags() {
        assert_eq!(Block::Air.render_kind(), RenderKind::None);
        assert_eq!(Block::Stone.render_kind(), RenderKind::Opaque);
        assert_eq!(Block::Leaves.render_kind(), RenderKind::Cutout);
        assert_eq!(Block::Water.render_kind(), RenderKind::Liquid);
        // Todo liquido es "visible no solido"; todo opaco es solido.
        for def in &BLOCKS {
            match def.render {
                RenderKind::Liquid => {
                    assert!(
                        def.visible && !def.solid,
                        "{} liquido mal marcado",
                        def.name
                    )
                }
                RenderKind::Opaque => assert!(def.solid, "{} opaco no solido", def.name),
                _ => {}
            }
        }
    }
}
