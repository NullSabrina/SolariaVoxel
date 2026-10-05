//! Fisica de **entidades** con caja AABB, desde cero.
//!
//! El jugador tiene su propia fisica (`player::controller`, aproximado por un
//! cilindro). Este modulo es la base **generica** para entidades (mobs, items,
//! drops) que aun no existen: gravedad, resolucion de colision **eje a eje** en
//! orden X, Z, Y (la vertical al final, para que `on_ground` sea exacto),
//! anti-tunelado, auto-step opcional y **flotabilidad** en el agua.
//!
//! Es logica pura: recibe una consulta `is_solid` en coordenadas de bloque, asi
//! que se testea sin GPU ni chunks.

use crate::math::Vec3;

/// Aceleracion de la gravedad, en bloques/s^2.
pub const GRAVITY: f32 = 28.0;

/// Velocidad vertical maxima (evita tunelar por caer demasiado rapido).
pub const MAX_FALL_SPEED: f32 = 50.0;

/// Friccion de suelo (se aplica a X/Z cuando la entidad esta apoyada).
pub const GROUND_FRICTION: f32 = 10.0;

/// Cuanto se reduce la gravedad dentro del agua.
pub const WATER_GRAVITY_SCALE: f32 = 0.30;

/// Arrastre dentro del agua (multiplica la velocidad por `1 - drag*dt`).
pub const WATER_DRAG: f32 = 4.0;

/// Flotabilidad: empuje hacia arriba dentro del agua, en bloques/s^2. Supera a
/// `GRAVITY * WATER_GRAVITY_SCALE`, asi que la entidad **flota** (sube).
pub const WATER_BUOYANCY: f32 = 12.0;

/// Margen para no "chocar" con el bloque sobre el que se apoya la caja.
const EPS: f32 = 1e-3;

/// Caja alineada a los ejes (AABB) del volumen de una entidad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Construye la caja a partir del punto de los **pies** (`pos`, centro
    /// horizontal) y los semitamanos: X/Z centrados, Y de `pos.y` hacia arriba
    /// `2 * half.y` (la altura de la entidad).
    pub fn from_feet(pos: Vec3, half: Vec3) -> Self {
        Self {
            min: Vec3::new(pos.x - half.x, pos.y, pos.z - half.z),
            max: Vec3::new(pos.x + half.x, pos.y + 2.0 * half.y, pos.z + half.z),
        }
    }

    /// ¿La caja ocupa la celda de bloque `(bx, by, bz)`?
    pub fn intersects_block(&self, bx: i32, by: i32, bz: i32) -> bool {
        let (x, y, z) = (bx as f32, by as f32, bz as f32);
        self.min.x < x + 1.0
            && self.max.x > x
            && self.min.y < y + 1.0
            && self.max.y > y
            && self.min.z < z + 1.0
            && self.max.z > z
    }

    /// ¿Solapan dos cajas?
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }
}

/// Resultado de mover una entidad un tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveResult {
    pub pos: Vec3,
    pub vel: Vec3,
    /// ¿Toco el suelo al caer en este tick?
    pub on_ground: bool,
}

/// Flotabilidad escalada por la **fraccion sumergida** (0..1). El motor de agua
/// reporta el nivel (`Fluid`) y la entidad sube mas cuanto mas hundida esta; en
/// la superficie el empuje baja a cero.
pub fn buoyancy_for(fill: f32) -> f32 {
    WATER_BUOYANCY * fill.clamp(0.0, 1.0)
}

/// Mueve una entidad AABB un tick de fisica, resolviendo la colision con el
/// mundo en el orden **X, Z, Y**.
///
/// * `pos`/`half`: posicion de los pies y semitamanos.
/// * `vel`: velocidad actual (entrada/salida).
/// * `dt`: paso de tiempo.
/// * `in_water`: si esta sumergida (flotabilidad + arrastre).
/// * `is_solid`: consulta "hay bloque solido en esta celda".
pub fn move_and_collide(
    pos: Vec3,
    half: Vec3,
    mut vel: Vec3,
    dt: f32,
    in_water: bool,
    is_solid: impl Fn(i32, i32, i32) -> bool,
) -> MoveResult {
    // 1. Gravedad (reducida en el agua) + flotabilidad y arrastre.
    let gravity = if in_water {
        GRAVITY * WATER_GRAVITY_SCALE
    } else {
        GRAVITY
    };
    vel.y -= gravity * dt;
    if in_water {
        vel.y += WATER_BUOYANCY * dt;
        let damp = (1.0 - WATER_DRAG * dt).max(0.0);
        vel = vel * damp;
    }
    vel.y = vel.y.clamp(-MAX_FALL_SPEED, MAX_FALL_SPEED);

    let mut pos = pos;
    let mut on_ground = false;
    for axis in [0usize, 2, 1] {
        let delta = match axis {
            0 => vel.x,
            2 => vel.z,
            _ => vel.y,
        } * dt;
        if delta == 0.0 {
            continue;
        }
        let (coord, hit) = resolve_axis(pos, half, axis, delta, &is_solid);
        match axis {
            0 => pos.x = coord,
            2 => pos.z = coord,
            _ => pos.y = coord,
        }
        if hit {
            match axis {
                0 => vel.x = 0.0,
                2 => vel.z = 0.0,
                _ => {
                    if delta < 0.0 {
                        on_ground = true;
                    }
                    vel.y = 0.0;
                }
            }
        }
    }

    // Friccion de suelo: frena el deslizamiento horizontal al estar apoyado.
    if on_ground {
        let f = (1.0 - GROUND_FRICTION * dt).max(0.0);
        vel.x *= f;
        vel.z *= f;
    }

    MoveResult {
        pos,
        vel,
        on_ground,
    }
}

/// Resuelve el movimiento en un eje. Devuelve la coordenada final y si choco.
///
/// Se recorre **todo el barrido** (union de la posicion vieja y la nueva) para
/// no tunelar a alta velocidad, no solo la posicion final.
fn resolve_axis(
    pos: Vec3,
    half: Vec3,
    axis: usize,
    delta: f32,
    is_solid: &impl Fn(i32, i32, i32) -> bool,
) -> (f32, bool) {
    // Intervalo de la caja en el eje que se mueve (antes y despues).
    let (old_lo, old_hi, new_lo, new_hi, coord) = match axis {
        0 => (
            pos.x - half.x,
            pos.x + half.x,
            pos.x + delta - half.x,
            pos.x + delta + half.x,
            pos.x + delta,
        ),
        2 => (
            pos.z - half.z,
            pos.z + half.z,
            pos.z + delta - half.z,
            pos.z + delta + half.z,
            pos.z + delta,
        ),
        _ => (
            pos.y,
            pos.y + 2.0 * half.y,
            pos.y + delta,
            pos.y + delta + 2.0 * half.y,
            pos.y + delta,
        ),
    };
    let swept_lo = old_lo.min(new_lo);
    let swept_hi = old_hi.max(new_hi);

    // Rangos perpendiculares (no cambian al mover este eje).
    let (y0, y1) = (pos.y, pos.y + 2.0 * half.y);
    let (x0, x1) = (pos.x - half.x, pos.x + half.x);
    let (z0, z1) = (pos.z - half.z, pos.z + half.z);

    let i0 = swept_lo.floor() as i32;
    let i1 = swept_hi.floor() as i32;

    // Rangos de bloques que cubre la caja en cada eje (una vez).
    let (px0, px1) = (x0.floor() as i32, x1.floor() as i32);
    let (py0, py1) = (y0.floor() as i32, y1.floor() as i32);
    let (pz0, pz1) = (z0.floor() as i32, z1.floor() as i32);

    let mut best: Option<f32> = None;
    // Candidato de correccion segun el eje que se mueve.
    let candidate = |p: [i32; 3]| -> f32 {
        match axis {
            0 => {
                if delta > 0.0 {
                    p[0] as f32 - half.x - EPS
                } else {
                    p[0] as f32 + 1.0 + half.x + EPS
                }
            }
            2 => {
                if delta > 0.0 {
                    p[2] as f32 - half.z - EPS
                } else {
                    p[2] as f32 + 1.0 + half.z + EPS
                }
            }
            _ => {
                if delta > 0.0 {
                    p[1] as f32 - 2.0 * half.y - EPS
                } else {
                    p[1] as f32 + 1.0 + EPS
                }
            }
        }
    };

    // Marca un bloque como obstaculo si es solido (acumula la correccion).
    let mut consider = |p: [i32; 3]| {
        if !is_solid(p[0], p[1], p[2]) {
            return;
        }
        let c = candidate(p);
        best = Some(match best {
            None => c,
            Some(cur) => {
                if delta > 0.0 {
                    cur.min(c)
                } else {
                    cur.max(c)
                }
            }
        });
    };

    for i in i0..=i1 {
        // Bloque `i` en el eje que se mueve; los otros dos ejes se recorren.
        match axis {
            0 => {
                for by in py0..=py1 {
                    for bz in pz0..=pz1 {
                        consider([i, by, bz]);
                    }
                }
            }
            2 => {
                for bx in px0..=px1 {
                    for by in py0..=py1 {
                        consider([bx, by, i]);
                    }
                }
            }
            _ => {
                for bx in px0..=px1 {
                    for bz in pz0..=pz1 {
                        consider([bx, i, bz]);
                    }
                }
            }
        }
    }

    match best {
        Some(c) => (c, true),
        None => (coord, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caja de "jugador": 0.3 x 0.9 x 0.3 de semitamanos.
    fn half() -> Vec3 {
        Vec3::new(0.3, 0.9, 0.3)
    }

    fn flat_floor(x: i32, y: i32, z: i32) -> bool {
        let _ = (x, z);
        y < 4
    }

    #[test]
    fn la_aabb_de_pies_tiene_la_altura_correcta() {
        let a = Aabb::from_feet(Vec3::new(0.5, 10.0, 0.5), half());
        assert!((a.min.y - 10.0).abs() < 1e-6);
        assert!((a.max.y - 11.8).abs() < 1e-6);
        assert!((a.min.x - 0.2).abs() < 1e-6);
        assert!((a.max.x - 0.8).abs() < 1e-6);
    }

    #[test]
    fn cae_y_se_posa_en_el_suelo() {
        let mut pos = Vec3::new(0.5, 30.0, 0.5);
        let mut vel = Vec3::ZERO;
        let mut grounded = false;
        for _ in 0..600 {
            let r = move_and_collide(pos, half(), vel, 1.0 / 60.0, false, flat_floor);
            pos = r.pos;
            vel = r.vel;
            grounded = r.on_ground;
        }
        assert!(grounded, "deberia tocar el suelo");
        assert!((pos.y - 4.0).abs() < 0.1, "posado en y={}", pos.y);
        assert_eq!(vel.y, 0.0, "velocidad vertical anulada");
    }

    #[test]
    fn no_atraviesa_una_pared_horizontal() {
        // Pared solida en x >= 5 (todo lo demas aire).
        let is_solid = |x: i32, _y: i32, _z: i32| x >= 5;
        let mut pos = Vec3::new(4.5, 10.0, 0.5);
        let mut vel = Vec3::new(20.0, 0.0, 0.0);
        for _ in 0..120 {
            let r = move_and_collide(pos, half(), vel, 1.0 / 60.0, false, is_solid);
            pos = r.pos;
            vel = r.vel;
        }
        assert!(pos.x + 0.3 < 5.0 + 1e-3, "atraveso la pared, x={}", pos.x);
        assert_eq!(vel.x, 0.0, "la velocidad en X se anula");
    }

    #[test]
    fn no_tunela_a_alta_velocidad() {
        // Un unico bloque de 1 m de grosor en x=10, y una entidad que va a 50 m/s:
        // en un frame avanza ~0.83 m < 1, pero probamos varios dt grandes.
        let is_solid = |x: i32, _y: i32, _z: i32| x == 10;
        let mut pos = Vec3::new(5.0, 10.0, 0.5);
        let mut vel = Vec3::new(50.0, 0.0, 0.0);
        for _ in 0..60 {
            let r = move_and_collide(pos, half(), vel, 1.0 / 30.0, false, is_solid);
            pos = r.pos;
            vel = r.vel;
            assert!(pos.x + 0.3 <= 10.0 + 1e-3, "tunelo: x={}", pos.x);
        }
    }

    #[test]
    fn flota_en_el_agua() {
        // Sin suelo: solo flotabilidad. La entidad debe ganar velocidad hacia
        // arriba y estabilizarse (arrastre).
        let mut pos = Vec3::new(0.5, 10.0, 0.5);
        let mut vel = Vec3::ZERO;
        for _ in 0..240 {
            let r = move_and_collide(pos, half(), vel, 1.0 / 60.0, true, |_, _, _| false);
            pos = r.pos;
            vel = r.vel;
        }
        assert!(vel.y > 0.0, "deberia flotar, vel.y={}", vel.y);
        assert!(pos.y > 10.0, "deberia haber subido, y={}", pos.y);
    }

    #[test]
    fn overlaps_detecta_interseccion() {
        let a = Aabb::from_feet(Vec3::new(0.5, 0.0, 0.5), half());
        let b = Aabb::from_feet(Vec3::new(0.9, 0.0, 0.5), half());
        let c = Aabb::from_feet(Vec3::new(3.0, 0.0, 0.5), half());
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn la_flotabilidad_escala_con_la_inmersion() {
        assert_eq!(buoyancy_for(0.0), 0.0);
        assert!(buoyancy_for(0.5) > 0.0);
        assert!(buoyancy_for(1.0) > buoyancy_for(0.5));
        // Se satura por encima de 1 (no sube sin limite si el dato se pasa).
        assert_eq!(buoyancy_for(2.0), buoyancy_for(1.0));
    }
}
