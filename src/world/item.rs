//! **Stacks de items**: un bloque y una cantidad (1..=`MAX`). Es el modelo que
//! usan el inventario y la hotbar (MEGA PROMPT 3, Fase C). Sin GPU ni estado de
//! ventana: es un dato puro y `Copy`.

use super::block::Block;

/// Una pila de bloques. `count` en `1..=MAX`; `Air` o `count == 0` = vacia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub block: Block,
    pub count: u8,
}

impl ItemStack {
    /// Tamano maximo de una pila (como en Minecraft).
    pub const MAX: u8 = 64;

    /// Crea una pila recortando la cantidad a `MAX`.
    pub fn new(block: Block, count: u8) -> Self {
        Self {
            block,
            count: count.min(Self::MAX),
        }
    }

    /// ¿La pila esta vacia? (aire o cantidad cero).
    pub fn is_empty(self) -> bool {
        self.count == 0 || self.block == Block::Air
    }

    /// Pila de un bloque con la cantidad maxima (creativo).
    pub fn full(block: Block) -> Self {
        Self::new(block, Self::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_cantidad_se_recorta_al_maximo() {
        assert_eq!(ItemStack::new(Block::Stone, 200).count, ItemStack::MAX);
        assert_eq!(ItemStack::new(Block::Stone, 0).count, 0);
    }

    #[test]
    fn una_pila_de_aire_o_cero_esta_vacia() {
        assert!(ItemStack::new(Block::Air, 64).is_empty());
        assert!(ItemStack::new(Block::Stone, 0).is_empty());
        assert!(!ItemStack::new(Block::Stone, 1).is_empty());
    }
}
