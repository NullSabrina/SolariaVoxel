//! Modelo del jugador: la **mano en primera persona** (brazo + item) y, en su
//! caso, el **personaje** completo. Son cubos de color (ver `render::model`),
//! no voxeles texturizados, para poder posarlos con transformaciones libres.
//!
//! Espacio local de la mano: el **hombro** esta en el origen y el brazo baja por
//! `-Y`. La matriz de la mano lo coloca en el **espacio de vista** (delante de
//! la camara, abajo a la derecha) y le aplica el balanceo/el golpe.

use crate::math::{Mat4, Vec3};
use crate::world::Block;

/// Un cubo del modelo: esquinas opuestas + color sRGB.
#[derive(Clone, Copy, Debug)]
pub struct Cuboid {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub color: [f32; 3],
}

// Paleta del personaje/mano.
const SKIN: [f32; 3] = [0.92, 0.74, 0.60];
const SLEEVE: [f32; 3] = [0.22, 0.46, 0.74];
const TRIM: [f32; 3] = [0.86, 0.78, 0.32];

/// Cubos del **brazo derecho** en primera persona (manga, puño y mano de piel).
///
/// La manga tiene un puño de color (como un cambio de tela) para que no sea un
/// cilindro plano. Es la mejora sobre el brazo liso de Minecraft.
pub fn first_person_arm() -> Vec<Cuboid> {
    let a = 0.075;
    vec![
        // Manga (hombro -> codo).
        Cuboid {
            from: [-a, -0.40, -a],
            to: [a, 0.0, a],
            color: SLEEVE,
        },
        // Puño de la manga (borde).
        Cuboid {
            from: [-a - 0.012, -0.47, -a - 0.012],
            to: [a + 0.012, -0.40, a + 0.012],
            color: TRIM,
        },
        // Antebrazo y mano (piel).
        Cuboid {
            from: [-a + 0.006, -0.80, -a + 0.006],
            to: [a - 0.006, -0.47, a - 0.006],
            color: SKIN,
        },
    ]
}

/// Cubo del **item** en la mano (centrado en su origen local).
pub fn held_item(color: [f32; 3]) -> Vec<Cuboid> {
    let h = 0.11;
    vec![Cuboid {
        from: [-h, -h, -h],
        to: [h, h, h],
        color,
    }]
}

/// Color aproximado de un bloque, para el cubo que se sostiene.
pub fn item_color(block: Block) -> [f32; 3] {
    match block {
        Block::Grass => [0.36, 0.55, 0.25],
        Block::Dirt | Block::CoarseDirt => [0.45, 0.32, 0.20],
        Block::Podzol => [0.42, 0.28, 0.14],
        Block::Stone => [0.55, 0.55, 0.57],
        Block::Gravel => [0.58, 0.55, 0.52],
        Block::Sand => [0.85, 0.79, 0.55],
        Block::Wood => [0.42, 0.30, 0.16],
        Block::Planks => [0.62, 0.46, 0.28],
        Block::CraftingTable => [0.55, 0.40, 0.24],
        Block::Leaves => [0.20, 0.55, 0.22],
        Block::Torch => [0.85, 0.65, 0.25],
        Block::Snow => [0.92, 0.94, 0.97],
        Block::Water => [0.20, 0.45, 0.85],
        Block::Lava => [0.90, 0.35, 0.10],
        Block::Obsidian => [0.10, 0.09, 0.14],
        Block::Air => [0.70, 0.70, 0.70],
    }
}

/// Matriz de la mano en **espacio de vista** (delante de la camara, abajo a la
/// derecha). `swing` en `0..1` = golpe (el brazo baja y avanza); `bob` = fase de
/// balanceo al andar.
pub fn hand_transform(swing: f32, bob: f32) -> Mat4 {
    // Balanceo: la mano sube/baja y se mece un poco al andar.
    let bob_y = (bob * 2.0).sin() * 0.018;
    let bob_x = (bob).sin() * 0.012;
    // El golpe empuja la mano hacia abajo-delante y la inclina.
    let swing = swing.clamp(0.0, 1.0);
    // El brazo cuelga por `-Y`. Se inclina hacia delante (rotacion X), se gira un
    // poco hacia dentro (Y) y se ladea (Z) para verlo de canto, entrando desde
    // abajo a la derecha. El golpe lo baja y lo inclina mas.
    let pitch = 2.35 - swing * 0.6;
    let roll = 0.35 + swing * 0.2;
    let z = -0.60 + swing * 0.06;
    Mat4::translation(Vec3::new(0.42 + bob_x, -0.78 + bob_y - swing * 0.12, z))
        * Mat4::rotation_y(-0.30)
        * Mat4::rotation_x(pitch)
        * Mat4::rotation_z(roll)
}

/// Transformacion del item dentro de la mano (local a la mano).
pub fn item_transform() -> Mat4 {
    Mat4::translation(Vec3::new(0.0, -0.78, -0.02)) * Mat4::rotation_x(0.6) * Mat4::rotation_z(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_mano_tiene_cubos_y_colores_validos() {
        let arm = first_person_arm();
        assert!(arm.len() >= 3);
        for c in &arm {
            // `from` es la esquina inferior y `to` la superior.
            assert!(c.from[1] < c.to[1], "el cubo debe tener from.y < to.y");
            for ch in c.color {
                assert!((0.0..=1.0).contains(&ch));
            }
        }
    }

    #[test]
    fn el_golpe_baja_la_mano() {
        let idle = hand_transform(0.0, 0.0).transform_point(Vec3::ZERO);
        let hit = hand_transform(1.0, 0.0).transform_point(Vec3::ZERO);
        assert!(hit.y < idle.y, "el golpe deberia bajar la mano");
    }

    #[test]
    fn el_item_va_delante_de_la_camara() {
        // El item debe quedar delante de la camara (z negativa en vista).
        let p = (hand_transform(0.0, 0.0) * item_transform()).transform_point(Vec3::ZERO);
        assert!(p.z < -0.2, "el item deberia estar delante: z={}", p.z);
    }
}
