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

/// Altura total del jugador (para saber donde colisiona un bloque).
pub const PLAYER_HEIGHT: f32 = 1.8;

/// Radio del jugador en el plano horizontal (lo tratamos como un cilindro).
pub const PLAYER_RADIUS: f32 = 0.3;

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

    /// Posa la camara sobre la **superficie** del terreno en su columna `(x, z)`
    /// (evita quedar atrapado bajo tierra al arrancar).
    ///
    /// Buscamos **de arriba hacia abajo** el primer bloque con aire justo encima:
    /// ese es el techo del terreno. Empezar desde el fondo seria un error, porque
    /// lo primero que aparece es piedra en lo profundo y el jugador acabaria
    /// enterrado dentro del suelo.
    pub fn settle(&mut self, camera: &mut Camera, is_solid: impl Fn(Vec3) -> bool) {
        let x = camera.position.x;
        let z = camera.position.z;
        let top = crate::world::WORLD_HEIGHT as f32 - 1.0;

        let mut y = top;
        while y >= 0.0 {
            let here = is_solid(Vec3::new(x, y, z));
            // Superficie = bloque solido con aire encima (o el borde superior).
            if here {
                let above = y + 1.0 >= crate::world::WORLD_HEIGHT as f32;
                if above || !is_solid(Vec3::new(x, y + 1.0, z)) {
                    camera.position.y = (y + 1.0) + EYE_HEIGHT;
                    self.vertical_velocity = 0.0;
                    self.on_ground = true;
                    camera.update_view();
                    return;
                }
            }
            y -= 1.0;
        }

        // Sin terreno solido en esta columna: dejamos la camara alta y que caiga.
        camera.position.y = top + EYE_HEIGHT;
        self.vertical_velocity = 0.0;
        self.on_ground = false;
        camera.update_view();
    }
}

/// ¿El bloque `voxel` (coordenadas de mundo) ocupa el espacio del jugador?
///
/// El jugador se aproxima por un **cilindro vertical**: un radio `PLAYER_RADIUS`
/// en el plano X-Z y desde los pies (`ojo - EYE_HEIGHT`) hasta `PLAYER_HEIGHT`.
/// Se usa para no colocar un bloque dentro del propio jugador.
pub fn block_overlaps_player(voxel: [i32; 3], eye: Vec3) -> bool {
    let (bx, by, bz) = (voxel[0] as f32, voxel[1] as f32, voxel[2] as f32);
    let feet = eye.y - EYE_HEIGHT;
    let overlaps_xz = (bx + 1.0 > eye.x - PLAYER_RADIUS)
        && (bx < eye.x + PLAYER_RADIUS)
        && (bz + 1.0 > eye.z - PLAYER_RADIUS)
        && (bz < eye.z + PLAYER_RADIUS);
    let overlaps_y = (by + 1.0 > feet) && (by < feet + PLAYER_HEIGHT);
    overlaps_xz && overlaps_y
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

    // Terreno tipo columna: solido desde y=0 hasta y=9, aire por encima.
    fn column_solid(point: Vec3) -> bool {
        point.y >= 0.0 && point.y < 10.0
    }

    #[test]
    fn settle_pone_al_jugador_sobre_la_superficie_no_dentro() {
        // Este era el bug: el jugador aparecia enterrado en la piedra.
        let mut camera = Camera::new(Vec3::new(0.5, 200.0, 0.5));
        camera.update_view();
        let mut player = PlayerController::new();
        player.settle(&mut camera, column_solid);
        // La superficie esta en y=10; los pies deben quedar en y=10.
        assert!(player.on_ground);
        assert!(
            (camera.position.y - (10.0 + EYE_HEIGHT)).abs() < 0.01,
            "camara en y={} (esperado {})",
            camera.position.y,
            10.0 + EYE_HEIGHT
        );
    }

    #[test]
    fn settle_sin_terreno_deja_caer_al_jugador() {
        let mut camera = Camera::new(Vec3::new(0.5, 5.0, 0.5));
        camera.update_view();
        let mut player = PlayerController::new();
        player.settle(&mut camera, |_| false);
        assert!(!player.on_ground);
    }

    // --- block_overlaps_player -----------------------------------------------
    // El ojo se pone a `EYE_HEIGHT` sobre los pies; usamos pies en y=0 -> ojo 1.62.
    fn eye_at(x: f32, z: f32) -> Vec3 {
        Vec3::new(x, EYE_HEIGHT, z)
    }

    #[test]
    fn el_bloque_bajo_los_pies_solapa() {
        // El jugador esta en (0.5, *, 0.5); su cuerpo ocupa x/z 0.2..0.8 y los
        // pies justo en y=0. El bloque [0,0,0] contiene ese espacio.
        assert!(block_overlaps_player([0, 0, 0], eye_at(0.5, 0.5)));
    }

    #[test]
    fn un_bloque_lejano_no_solapa() {
        // A 2 bloques en X no toca al jugador (radio 0.3).
        assert!(!block_overlaps_player([2, 0, 0], eye_at(0.5, 0.5)));
    }

    #[test]
    fn un_bloque_bajo_el_suelo_no_solapa() {
        // y=-1 esta por debajo de los pies (0) -> fuera.
        assert!(!block_overlaps_player([0, -1, 0], eye_at(0.5, 0.5)));
    }

    #[test]
    fn un_bloque_encima_de_la_cabeza_no_solapa() {
        // La cabeza queda en 1.8; un bloque a y=2 empieza en 2.0 -> fuera.
        assert!(!block_overlaps_player([0, 2, 0], eye_at(0.5, 0.5)));
    }

    #[test]
    fn el_bloque_de_la_cabeza_solapa() {
        // y=1 ocupa 1..2, solapa con el cuerpo (0..1.8). El jugador de 1.8 de
        // alto ocupa parte del bloque de arriba.
        assert!(block_overlaps_player([0, 1, 0], eye_at(0.5, 0.5)));
    }

    #[test]
    fn el_borde_del_radio_cuenta_o_no_segun_el_caso() {
        // Radio 0.3: el jugador en x=0.5 cubre 0.2..0.8 en X.
        // Un bloque en x=1 empieza en 1.0 > 0.8 -> NO solapa.
        assert!(!block_overlaps_player([1, 0, 0], eye_at(0.5, 0.5)));
        // Pero uno en x=0 cubre 0..1 y si solapa.
        assert!(block_overlaps_player([0, 0, 0], eye_at(0.5, 0.5)));
    }
}
