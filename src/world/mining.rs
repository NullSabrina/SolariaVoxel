//! **Estadisticas de minado** por bloque: dureza, resistencia a explosiones y
//! pico minimo.
//!
//! Vive aparte de [`super::registry`] a proposito: el registro describe la
//! **identidad y el render** del bloque (ids, tiles, flags), mientras que esto es
//! **gameplay**. Asi un bloque puede cambiar de balance sin tocar su definicion
//! grafica, y el catalogo de `docs/materials.md` tiene un unico sitio donde
//! aterrizar cuando se anadan rocas nuevas.
//!
//! La tabla se indexa por `Block::id()`. Un test comprueba que su longitud
//! coincide con [`super::registry::COUNT`].

use super::block::Block;

/// Pico minimo para obtener drop. Los valores siguen la escala de diseno:
/// 1 madera, 2 piedra, 3 hierro, 4 diamante, 5 especial.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolTier {
    /// No exige pico (mano, pala, hacha o tijeras).
    None = 0,
    Wood = 1,
    Stone = 2,
    Iron = 3,
    Diamond = 4,
    Special = 5,
}

/// Perfil de minado de un bloque. Escala de diseno 0..=10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningProfile {
    /// Dureza relativa (0 = instantaneo; 10 = el mas lento).
    pub hardness: u8,
    /// Resistencia a explosiones (0 = se borra; 10 = resiste TNT).
    pub blast_resistance: u8,
    /// Pico minimo para que haya drop.
    pub tool_tier: ToolTier,
}

const fn p(hardness: u8, blast_resistance: u8, tool_tier: ToolTier) -> MiningProfile {
    MiningProfile {
        hardness,
        blast_resistance,
        tool_tier,
    }
}

/// Perfiles en orden de `Block::id()` (ver `registry::BLOCKS`).
pub const MINING: [MiningProfile; super::registry::COUNT] = [
    p(0, 0, ToolTier::None),     // 0  air
    p(1, 1, ToolTier::None),     // 1  grass
    p(1, 1, ToolTier::None),     // 2  dirt
    p(5, 6, ToolTier::Wood),     // 3  stone
    p(1, 1, ToolTier::None),     // 4  sand
    p(2, 3, ToolTier::None),     // 5  wood
    p(1, 1, ToolTier::None),     // 6  leaves
    p(1, 1, ToolTier::None),     // 7  torch
    p(1, 1, ToolTier::None),     // 8  snow
    p(0, 0, ToolTier::None),     // 9  water
    p(2, 3, ToolTier::None),     // 10 planks
    p(2, 3, ToolTier::None),     // 11 crafting_table
    p(2, 2, ToolTier::None),     // 12 coarse_dirt
    p(2, 2, ToolTier::None),     // 13 gravel
    p(1, 1, ToolTier::None),     // 14 podzol
    p(0, 0, ToolTier::None),     // 15 lava
    p(9, 10, ToolTier::Diamond), // 16 obsidian
    p(0, 0, ToolTier::None),     // 17 flower
];

/// Perfil de un bloque. Nunca falla: todo `Block` tiene entrada.
#[inline]
pub fn profile(block: Block) -> MiningProfile {
    MINING[block.id() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tabla_cubre_todos_los_bloques() {
        assert_eq!(MINING.len(), super::super::registry::COUNT);
    }

    #[test]
    fn las_estadisticas_estan_en_la_escala() {
        for (id, m) in MINING.iter().enumerate() {
            assert!(m.hardness <= 10, "bloque {id}: dureza {}", m.hardness);
            assert!(
                m.blast_resistance <= 10,
                "bloque {id}: resistencia {}",
                m.blast_resistance
            );
        }
    }

    #[test]
    fn obsidiana_es_el_techo_de_minado() {
        let m = profile(Block::Obsidian);
        assert_eq!(m.hardness, 9);
        assert_eq!(m.blast_resistance, 10);
        assert_eq!(m.tool_tier, ToolTier::Diamond);
    }

    #[test]
    fn tierras_y_maderas_no_exigen_pico() {
        for b in [
            Block::Grass,
            Block::Dirt,
            Block::Sand,
            Block::Gravel,
            Block::Podzol,
            Block::Wood,
            Block::Planks,
        ] {
            assert_eq!(profile(b).tool_tier, ToolTier::None, "{b:?}");
        }
    }

    #[test]
    fn piedra_exige_pico_de_madera() {
        assert_eq!(profile(Block::Stone).tool_tier, ToolTier::Wood);
    }

    #[test]
    fn los_tiers_estan_ordenados() {
        assert!(ToolTier::None < ToolTier::Wood);
        assert!(ToolTier::Wood < ToolTier::Stone);
        assert!(ToolTier::Stone < ToolTier::Iron);
        assert!(ToolTier::Iron < ToolTier::Diamond);
        assert!(ToolTier::Diamond < ToolTier::Special);
    }
}
