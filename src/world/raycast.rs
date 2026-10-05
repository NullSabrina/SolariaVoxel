//! Raycast sobre la rejilla de voxeles (algoritmo de Amanatides y Woo).
//!
//! Lanza un rayo desde el ojo del jugador en la direccion en que mira y devuelve
//! el primer **bloque golpeable** que toca, junto con la **cara** por la que
//! entra (para poder colocar el bloque nuevo justo al lado).
//!
//! Que es "golpeable" lo decide el llamante con un predicado. El juego usa
//! `is_solid || is_visible`, para poder apuntar y romper tambien la antorcha
//! (solida no, pero visible si).
//!
//! El algoritmo recorre las celdas de la rejilla en el orden en que el rayo las
//! atraviesa (DDA 3D): en cada paso avanza por el eje cuyo siguiente cruce esta
//! mas cerca. Complejidad proporcional al numero de celdas que toca, no a la
//! distancia.

use crate::math::Vec3;

use super::block::{Block, Face};

/// Resultado de un impacto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RayHit {
    /// Coordenadas del bloque golpeado (en voxeles, no en metros).
    pub block: [i32; 3],
    /// Cara del bloque por la que ha entrado el rayo.
    pub face: Face,
}

/// Lanza un rayo desde `origin` en direccion `dir` (no hace falta normalizar) y
/// devuelve el primer bloque golpeable a una distancia menor que `max_distance`.
///
/// `is_hit` consulta el bloque en coordenadas de voxel; devuelve `true` si el
/// rayo debe detenerse ahi (bloque solido, o visible no solido como la antorcha).
/// Mantener esta funcion como parametro desacopla el raycast del mundo concreto
/// y permite testearlo con un plano simple.
pub fn raycast(
    origin: Vec3,
    dir: Vec3,
    max_distance: f32,
    is_hit: impl Fn(i32, i32, i32) -> bool,
) -> Option<RayHit> {
    let dir = dir.normalize();
    if dir == Vec3::ZERO {
        return None;
    }

    // Celda actual.
    let mut voxel = [
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    ];

    // Direccion de avance y distancia que hay que recorrer para cruzar una celda
    // entera en cada eje (`t_delta`). Si la direccion es 0 en un eje, ese eje
    // nunca avanza.
    let step = [signum_i32(dir.x), signum_i32(dir.y), signum_i32(dir.z)];
    let t_delta = [axis_delta(dir.x), axis_delta(dir.y), axis_delta(dir.z)];

    // Distancia hasta el primer cruce de cada eje (`t_max`).
    let mut t_max = [
        first_crossing(origin.x, voxel[0], dir.x),
        first_crossing(origin.y, voxel[1], dir.y),
        first_crossing(origin.z, voxel[2], dir.z),
    ];

    // Si empezamos dentro de un bloque golpeable, no hay cara de entrada clara.
    if is_hit(voxel[0], voxel[1], voxel[2]) {
        return Some(RayHit {
            block: voxel,
            face: Face::PosY,
        });
    }

    let mut distance = 0.0f32;
    while distance <= max_distance {
        // ¿Que eje cruza antes?
        let axis = if t_max[0] < t_max[1] {
            if t_max[0] < t_max[2] { 0 } else { 2 }
        } else if t_max[1] < t_max[2] {
            1
        } else {
            2
        };

        distance = t_max[axis];
        if distance > max_distance {
            break;
        }
        voxel[axis] += step[axis];
        t_max[axis] += t_delta[axis];

        if is_hit(voxel[0], voxel[1], voxel[2]) {
            // La cara golpeada es la opuesta al avance en ese eje.
            let face = face_from_step(axis, step[axis]);
            return Some(RayHit { block: voxel, face });
        }
    }

    None
}

/// Signo de un flotante como entero (-1, 0, 1).
fn signum_i32(v: f32) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// Distancia (en unidades de `t`) para cruzar una celda entera en un eje.
/// `1 / |dir|`; infinito si la direccion es 0 (nunca avanza).
fn axis_delta(d: f32) -> f32 {
    if d.abs() < 1e-8 {
        f32::INFINITY
    } else {
        (1.0 / d).abs()
    }
}

/// Distancia hasta el primer cruce de celda en un eje.
fn first_crossing(o: f32, voxel: i32, d: f32) -> f32 {
    if d.abs() < 1e-8 {
        return f32::INFINITY;
    }
    let boundary = if d > 0.0 {
        (voxel + 1) as f32
    } else {
        voxel as f32
    };
    ((boundary - o) / d).abs()
}

/// Convierte (eje, sentido de avance) en la cara de entrada.
fn face_from_step(axis: usize, step: i32) -> Face {
    match (axis, step > 0) {
        (0, true) => Face::NegX,
        (0, false) => Face::PosX,
        (1, true) => Face::NegY,
        (1, false) => Face::PosY,
        (2, true) => Face::NegZ,
        _ => Face::PosZ,
    }
}

/// Funcion auxiliar: bloque solido en el plano y<0 de tests.
#[allow(dead_code)]
fn _unused(_: Block) {}

#[cfg(test)]
mod tests {
    use super::*;

    // Mundo de test: solido por debajo de y=0 (un "suelo" plano).
    fn floor_only(x: i32, y: i32, z: i32) -> bool {
        let _ = (x, z);
        y < 0
    }

    #[test]
    fn rayo_hacia_abajo_golpea_el_suelo_por_la_cara_de_arriba() {
        let origin = Vec3::new(0.5, 5.0, 0.5);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let hit = raycast(origin, dir, 20.0, floor_only).expect("deberia golpear");
        assert_eq!(hit.block, [0, -1, 0]);
        assert_eq!(hit.face, Face::PosY);
    }

    #[test]
    fn rayo_en_direccion_recta_sin_obstaculos_devuelve_none() {
        let origin = Vec3::new(0.5, 5.0, 0.5);
        let dir = Vec3::new(0.0, 1.0, 0.0);
        assert!(raycast(origin, dir, 10.0, floor_only).is_none());
    }

    #[test]
    fn rayo_diagonal_avanza_eje_a_eje() {
        // Un unico bloque en (2, 3, 0); el rayo va hacia +X con algo de +Y.
        let origin = Vec3::new(0.5, 0.5, 0.5);
        let dir = Vec3::new(1.0, 1.0, 0.0);
        let is_solid = |x: i32, y: i32, z: i32| {
            let _ = z;
            x == 2 && y == 2
        };
        // Ajustamos el rayo para tocar el bloque en la diagonal.
        let hit = raycast(origin, dir, 10.0, is_solid);
        assert!(hit.is_some());
    }

    #[test]
    fn la_cara_corresponde_a_la_direccion_de_llegada() {
        // Bloque en x=3; el rayo va hacia +X y debe entrar por su cara -X.
        let origin = Vec3::new(0.5, 0.5, 0.5);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let is_solid = |x: i32, _y: i32, _z: i32| x == 3;
        let hit = raycast(origin, dir, 10.0, is_solid).expect("golpe");
        assert_eq!(hit.block, [3, 0, 0]);
        assert_eq!(hit.face, Face::NegX);
    }

    #[test]
    fn golpea_un_bloque_visible_no_solido() {
        // Simula una antorcha: una celda golpeable en x=4 que no seria "solida".
        // El raycast no distingue: se detiene en el primer `is_hit`.
        let origin = Vec3::new(0.5, 0.5, 0.5);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let is_hit = |x: i32, y: i32, z: i32| y == 0 && z == 0 && x == 4;
        let hit = raycast(origin, dir, 10.0, is_hit).expect("deberia golpear la antorcha");
        assert_eq!(hit.block, [4, 0, 0]);
        assert_eq!(hit.face, Face::NegX);
    }
}
