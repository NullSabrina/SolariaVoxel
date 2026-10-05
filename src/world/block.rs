//! Tipos de bloque y como se mapean a la textura.
//!
//! Un bloque es un `enum` pequenito. Guardar bloques como `u8` (en lugar de un
//! struct grande por voxel) es clave para el objetivo de memoria: un chunk de
//! 16^3 ocupa 4096 bytes, 1 byte por bloque.

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
}

/// Tipo de bloque. El valor numerico (`u8`) es lo que se guarda en el chunk.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Default for Block {
    /// El bloque que se coloca al empezar (piedra).
    fn default() -> Self {
        Block::Stone
    }
}

impl Block {
    /// Reconstruye un bloque a partir del `u8` guardado. Cualquier valor
    /// desconocido se interpreta como aire (tolerancia hacia adelante).
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Block::Grass,
            2 => Block::Dirt,
            3 => Block::Stone,
            4 => Block::Sand,
            5 => Block::Wood,
            6 => Block::Leaves,
            7 => Block::Torch,
            8 => Block::Snow,
            _ => Block::Air,
        }
    }

    /// El `u8` que se guarda en el chunk.
    #[inline]
    pub fn id(self) -> u8 {
        self as u8
    }

    /// ¿Ocupa espacio? (el aire y la antorcha no bloquean).
    #[inline]
    pub fn is_solid(self) -> bool {
        !matches!(self, Block::Air | Block::Torch)
    }

    /// ¿Es un bloque que se dibuja pero no bloquea? (la antorcha).
    #[inline]
    pub fn is_visible(self) -> bool {
        matches!(self, Block::Torch)
    }

    /// Luz que **emite** el bloque (0..15). La antorcha emite 14.
    #[inline]
    pub fn light_emission(self) -> u8 {
        match self {
            Block::Torch => 14,
            _ => 0,
        }
    }

    /// Que tile del atlas usa cada cara de este bloque.
    ///
    /// El atlas se describe en [`crate::world::atlas`]. El pasto, por ejemplo,
    /// usa verde arriba, tierra abajo y una cara lateral mixta.
    pub fn face_tile(self, face: Face) -> u16 {
        match self {
            Block::Air => 0,
            Block::Grass => match face {
                Face::PosY => 0, // hierba
                Face::NegY => 2, // tierra
                _ => 1,          // lateral
            },
            Block::Dirt => 2,
            Block::Stone => 3,
            Block::Sand => 4,
            Block::Wood => match face {
                Face::PosY | Face::NegY => 6, // anillos
                _ => 5,                       // corteza
            },
            Block::Leaves => 7,
            // La antorcha usa un tile propio. Desde v0.6.2 no se dibuja como
            // cubo, sino como dos quads cruzados (ver `emit_torch_cross`).
            Block::Torch => 8,
            // Nieve (biomas frios).
            Block::Snow => 9,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_ida_y_vuelta() {
        for b in [Block::Air, Block::Grass, Block::Stone, Block::Wood] {
            assert_eq!(Block::from_u8(b.id()), b);
        }
    }

    #[test]
    fn valor_desconocido_es_aire() {
        assert_eq!(Block::from_u8(200), Block::Air);
    }

    #[test]
    fn solo_el_aire_no_es_solido() {
        assert!(!Block::Air.is_solid());
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
        ] {
            for face in Face::ALL {
                assert!(b.face_tile(face) < 10, "{b:?} {face:?}");
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
    }
}
