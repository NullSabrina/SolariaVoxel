//! Fisica vertical del jugador: gravedad y deteccion de suelo.
//!
//! En v0.3.2 solo hay movimiento vertical: el jugador cae hasta posarse sobre el
//! primer bloque solido. La logica esta **desacoplada** del mundo y del renderer:
//! `update` recibe una funcion de consulta (`is_solid`), asi que se puede testear
//! sin GPU ni chunks.
//!
//! Nota: en v0.3.1 el mundo se regenera "de golpe" al cruzar de chunk, y aquello
//! era incompatible con un jugador a ras de suelo. Al subir el radio de streaming
//! a 7x7 (112x112 bloques), el jugador puede caminar sin salir del area cargada
//! en una partida normal.

use crate::math::Vec3;
use crate::scene::Camera;

/// Aceleracion de la gravedad, en bloques/s^2.
pub const GRAVITY: f32 = 28.0;

/// Velocidad vertical maxima de caida, en bloques/s (evita atravesar el suelo).
pub const MAX_FALL_SPEED: f32 = 50.0;

/// Velocidad de salto al despegar.
pub const JUMP_SPEED: f32 = 9.0;

/// Altura del jugador: la camara va a `pies + EYE_HEIGHT`.
pub const EYE_HEIGHT: f32 = 1.62;

/// El jugador resuelve la fisica vertical contra el mundo.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerController {
    /// Velocidad vertical actual (positiva = subiendo).
    pub vertical_velocity: f32,
    /// ¿Esta el jugador tocando el suelo?
    pub on_ground: bool,
}

impl PlayerController {
    /// Crea un controlador en reposo.
    pub fn new() -> Self {
        Self::default()
    }

    /// Aplica un frame de fisica vertical a la camara.
    ///
    /// * `is_solid`: consulta "hay bloque solido en este punto del mundo".
    /// * `fly_up`: en modo vuelo, `+1` subir / `-1` bajar.
    /// * `flying`: si es `true`, ignora la gravedad.
    /// * `jump`: si esta en el suelo, da un salto.
    pub fn update(
        &mut self,
        camera: &mut Camera,
        is_solid: impl Fn(Vec3) -> bool,
        fly_up: f32,
        flying: bool,
        jump: bool,
        dt: f32,
    ) {
        let feet_y = camera.position.y - EYE_HEIGHT;

        if flying {
            // Vuelo: movimiento vertical directo, sin gravedad ni suelo.
            camera.position.y += fly_up * camera.speed * dt;
            self.vertical_velocity = 0.0;
            self.on_ground = false;
            camera.update_view();
            return;
        }

        // 1. Salto (solo si estamos apoyados).
        if jump && self.on_ground {
            self.vertical_velocity = JUMP_SPEED;
            self.on_ground = false;
        }

        // 2. Gravedad, limitada en ambos sentidos.
        self.vertical_velocity =
            (self.vertical_velocity - GRAVITY * dt).clamp(-MAX_FALL_SPEED, JUMP_SPEED);

        // 3. Integracion vertical. Subdividimos el paso si cae muy rapido, para
        //    no atravesar un bloque fino entre dos frames.
        let mut new_feet = feet_y;
        let total = self.vertical_velocity * dt;
        let substeps = ((total.abs() / 0.5).ceil() as i32).max(1);
        let step = total / substeps as f32;

        for _ in 0..substeps {
            let candidate = new_feet + step;
            if self.vertical_velocity <= 0.0 {
                // Cayendo: miramos si los pies entran en un bloque solido.
                let probe = Vec3::new(camera.position.x, candidate, camera.position.z);
                if is_solid(probe) {
                    // Nos posamos justo encima del bloque.
                    new_feet = candidate.floor() + 1.0;
                    self.vertical_velocity = 0.0;
                    self.on_ground = true;
                    break;
                }
                self.on_ground = false;
                new_feet = candidate;
            } else {
                // Subiendo: miramos si la cabeza choca (techo).
                let head = Vec3::new(camera.position.x, candidate + EYE_HEIGHT, camera.position.z);
                if is_solid(head) {
                    self.vertical_velocity = 0.0;
                    break;
                }
                self.on_ground = false;
                new_feet = candidate;
            }
        }

        camera.position.y = new_feet + EYE_HEIGHT;
        camera.update_view();
    }

    /// Teleporta la camara a la primera superficie solida bajo ella (evita quedar
    /// atrapado bajo tierra al arrancar).
    pub fn settle(&mut self, camera: &mut Camera, is_solid: impl Fn(Vec3) -> bool) {
        let mut y = 0.0f32;
        while y < crate::world::WORLD_HEIGHT as f32 {
            let feet = Vec3::new(camera.position.x, y, camera.position.z);
            if is_solid(feet) {
                camera.position.y = (y + 1.0) + EYE_HEIGHT;
                self.vertical_velocity = 0.0;
                self.on_ground = true;
                camera.update_view();
                return;
            }
            y += 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Suelo plano en y < 4; aire por encima.
    fn flat_solid(point: Vec3) -> bool {
        point.y < 4.0
    }

    fn test_camera() -> Camera {
        let mut camera = Camera::new(Vec3::new(0.5, 12.0, 0.5));
        camera.update_view();
        camera
    }

    #[test]
    fn cae_hasta_posarse_en_el_suelo() {
        let mut camera = test_camera();
        let mut player = PlayerController::new();
        for _ in 0..120 {
            player.update(&mut camera, flat_solid, 0.0, false, false, 1.0 / 60.0);
        }
        assert!(player.on_ground, "deberia estar en el suelo");
        assert!((camera.position.y - (4.0 + EYE_HEIGHT)).abs() < 0.1);
    }

    #[test]
    fn no_atraviesa_el_suelo_a_alta_velocidad() {
        let mut camera = Camera::new(Vec3::new(0.5, 300.0, 0.5));
        camera.update_view();
        let mut player = PlayerController::new();
        for _ in 0..1200 {
            player.update(&mut camera, flat_solid, 0.0, false, false, 1.0 / 30.0);
        }
        assert!(camera.position.y >= 4.0 + EYE_HEIGHT - 0.1);
        assert!(player.on_ground);
    }

    #[test]
    fn vuela_ignora_la_gravedad() {
        let mut camera = test_camera();
        let mut player = PlayerController::new();
        let start = camera.position.y;
        player.update(&mut camera, flat_solid, 1.0, true, false, 0.5);
        assert!(camera.position.y > start);
        assert!(!player.on_ground);
    }

    #[test]
    fn salta_solo_desde_el_suelo() {
        let mut camera = test_camera();
        let mut player = PlayerController::new();
        // Sin estar en el suelo, un salto no hace nada.
        player.update(&mut camera, flat_solid, 0.0, false, true, 1.0 / 60.0);
        assert!(player.vertical_velocity <= 0.0);
        // Dejamos que aterrice.
        for _ in 0..120 {
            player.update(&mut camera, flat_solid, 0.0, false, false, 1.0 / 60.0);
        }
        assert!(player.on_ground);
        player.update(&mut camera, flat_solid, 0.0, false, true, 1.0 / 60.0);
        assert!(player.vertical_velocity > 5.0, "deberia haber saltado");
    }

    #[test]
    fn la_consulta_de_bloque_usa_is_solid() {
        use crate::world::Block;
        assert!(Block::Stone.is_solid());
        assert!(!Block::Air.is_solid());
    }
}
