//! Fisica del jugador: gravedad, suelo y **colision horizontal**.
//!
//! La fisica esta **desacoplada** del mundo y del renderer: cada metodo recibe
//! una funcion de consulta (`is_solid`), asi que se puede testear sin GPU ni
//! chunks.
//!
//! * [`PlayerController::update`] resuelve la **vertical** (gravedad, suelo,
//!   salto, vuelo).
//! * [`PlayerController::move_horizontal`] mueve en el plano X-Z resolviendo la
//!   **colision horizontal**: el jugador es una caja (radio `PLAYER_RADIUS`, alto
//!   `PLAYER_HEIGHT`) y se mueve eje a eje, de modo que si choca con una pared se
//!   **desliza** a lo largo de ella en lugar de quedarse clavado.
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

/// Altura maxima que el jugador **sube automaticamente** al caminar contra un
/// escalon (auto-step). Con terreno por bloque hay escalones de 1 bloque por
/// todos lados; sin esto el jugador se quedaria clavado en cada subida.
pub const STEP_HEIGHT: f32 = 1.0;

/// Margen para no "chocar" con el bloque sobre el que estamos de pie: la caja de
/// colision no incluye exactamente los extremos (pies y cabeza).
const SKIN: f32 = 1e-3;

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
                // Cayendo: miramos la **huella completa**, no solo el centro.
                // Si el jugador esta apoyado sobre un escalon, su centro puede
                // quedar sobre la columna vecina (mas baja) y empezar a caer
                // mientras su caja todavia solapa el bloque del escalon; eso lo
                // dejaba embebido. Sondear la huella lo posa sobre la superficie
                // mas alta que sus pies atraviesan.
                if let Some(surface) = Self::landing_surface(
                    camera.position.x,
                    camera.position.z,
                    candidate,
                    &is_solid,
                ) {
                    new_feet = surface;
                    self.vertical_velocity = 0.0;
                    self.on_ground = true;
                    break;
                }
                self.on_ground = false;
                new_feet = candidate;
            } else {
                // Subiendo: miramos si la cabeza choca (techo), tambien con la
                // huella completa.
                if Self::ceiling_hits(
                    camera.position.x,
                    camera.position.z,
                    candidate + EYE_HEIGHT,
                    &is_solid,
                ) {
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

    /// Mueve al jugador en el plano X-Z con **colision horizontal**.
    ///
    /// `forward` y `right` son los ejes de input en `[-1, 1]`. El movimiento se
    /// resuelve **eje a eje**: primero X y luego Z. Si un eje choca contra un
    /// bloque solido, ese eje se cancela y el otro sigue, de modo que el jugador
    /// se **desliza** a lo largo de la pared en vez de quedarse clavado.
    pub fn move_horizontal(
        &mut self,
        camera: &mut Camera,
        is_solid: impl Fn(Vec3) -> bool,
        forward: f32,
        right: f32,
        dt: f32,
    ) {
        let yaw = camera.yaw_deg.to_radians();
        // "Adelante" horizontal (sin componente vertical), como `Camera::walk`.
        let fwd = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        let right_v = fwd.cross(Vec3::Y);
        let mut dir = fwd * forward + right_v * right;
        if dir.length_squared() > 0.0 {
            dir = dir.normalize();
        }
        let delta = dir * (camera.speed * dt);

        // Eje X.
        if delta.x != 0.0 {
            let candidate = Vec3::new(
                camera.position.x + delta.x,
                camera.position.y,
                camera.position.z,
            );
            if !Self::collides(candidate, &is_solid) {
                camera.position.x = candidate.x;
            } else if let Some(raised) = Self::try_step_up(candidate, &is_solid) {
                // Escalon de 1 bloque: subimos ademas de avanzar.
                camera.position.x = candidate.x;
                camera.position.y = raised;
                self.on_ground = true;
            }
        }
        // Eje Z.
        if delta.z != 0.0 {
            let candidate = Vec3::new(
                camera.position.x,
                camera.position.y,
                camera.position.z + delta.z,
            );
            if !Self::collides(candidate, &is_solid) {
                camera.position.z = candidate.z;
            } else if let Some(raised) = Self::try_step_up(candidate, &is_solid) {
                camera.position.z = candidate.z;
                camera.position.y = raised;
                self.on_ground = true;
            }
        }
        camera.update_view();
    }

    /// Si el movimiento se bloquea por un escalon bajo (`<= STEP_HEIGHT`), devuelve
    /// la nueva altura del ojo tras subirlo; `None` si no hay hueco arriba.
    fn try_step_up(pos: Vec3, is_solid: &impl Fn(Vec3) -> bool) -> Option<f32> {
        let raised = Vec3::new(pos.x, pos.y + STEP_HEIGHT, pos.z);
        (!Self::collides(raised, is_solid)).then_some(raised.y)
    }

    /// ¿La caja del jugador (radio `PLAYER_RADIUS`, alto `PLAYER_HEIGHT`, pies en
    /// `pos.y - EYE_HEIGHT`) solapa algun bloque solido en `pos`?
    fn collides(pos: Vec3, is_solid: &impl Fn(Vec3) -> bool) -> bool {
        let feet = pos.y - EYE_HEIGHT;
        let (x0, x1) = (pos.x - PLAYER_RADIUS, pos.x + PLAYER_RADIUS);
        let (z0, z1) = (pos.z - PLAYER_RADIUS, pos.z + PLAYER_RADIUS);
        // La caja no llega exactamente a los pies ni a la cabeza (`SKIN`), para
        // no colisionar con el bloque del suelo ni con el techo por rozarlos.
        let (y0, y1) = (feet + SKIN, feet + PLAYER_HEIGHT - SKIN);

        let (ix0, ix1) = (x0.floor() as i32, x1.floor() as i32);
        let (iy0, iy1) = (y0.floor() as i32, y1.floor() as i32);
        let (iz0, iz1) = (z0.floor() as i32, z1.floor() as i32);

        for x in ix0..=ix1 {
            for y in iy0..=iy1 {
                for z in iz0..=iz1 {
                    // Centro del voxel, como espera la consulta del mundo.
                    if is_solid(Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Columnas (x0, x1, z0, z1) que cubre la huella del jugador en `(x, z)`.
    fn footprint_columns(x: f32, z: f32) -> (i32, i32, i32, i32) {
        let (x0, x1) = (x - PLAYER_RADIUS, x + PLAYER_RADIUS);
        let (z0, z1) = (z - PLAYER_RADIUS, z + PLAYER_RADIUS);
        (
            x0.floor() as i32,
            x1.floor() as i32,
            z0.floor() as i32,
            z1.floor() as i32,
        )
    }

    /// Si los pies (candidatos) entran en un bloque solido, devuelve la superficie
    /// a la que posarlos: el techo del bloque mas alto que la huella atraviesa en
    /// esa capa. Si no hay nada solido bajo la huella, `None` (sigue cayendo).
    fn landing_surface(x: f32, z: f32, feet: f32, is_solid: &impl Fn(Vec3) -> bool) -> Option<f32> {
        let by = feet.floor() as i32;
        if by < 0 || by >= crate::world::WORLD_HEIGHT as i32 {
            return None;
        }
        let (ix0, ix1, iz0, iz1) = Self::footprint_columns(x, z);
        let mut encontrado = false;
        for ix in ix0..=ix1 {
            for iz in iz0..=iz1 {
                if is_solid(Vec3::new(ix as f32 + 0.5, by as f32 + 0.5, iz as f32 + 0.5)) {
                    encontrado = true;
                }
            }
        }
        encontrado.then(|| (by + 1) as f32)
    }

    /// ¿Alguna columna de la huella tiene un bloque solido en la capa de `head`?
    fn ceiling_hits(x: f32, z: f32, head: f32, is_solid: &impl Fn(Vec3) -> bool) -> bool {
        let by = head.floor() as i32;
        if by < 0 || by >= crate::world::WORLD_HEIGHT as i32 {
            return false;
        }
        let (ix0, ix1, iz0, iz1) = Self::footprint_columns(x, z);
        for ix in ix0..=ix1 {
            for iz in iz0..=iz1 {
                if is_solid(Vec3::new(ix as f32 + 0.5, by as f32 + 0.5, iz as f32 + 0.5)) {
                    return true;
                }
            }
        }
        false
    }

    /// Superficie mas alta (techo del bloque solido mas alto) bajo la **huella**
    /// del jugador, buscando de arriba hacia abajo. `None` si no hay terreno.
    fn top_surface(x: f32, z: f32, is_solid: &impl Fn(Vec3) -> bool) -> Option<f32> {
        let (ix0, ix1, iz0, iz1) = Self::footprint_columns(x, z);
        let mut best: Option<f32> = None;
        for ix in ix0..=ix1 {
            for iz in iz0..=iz1 {
                let mut y = crate::world::WORLD_HEIGHT as i32 - 1;
                while y >= 0 {
                    if is_solid(Vec3::new(ix as f32 + 0.5, y as f32 + 0.5, iz as f32 + 0.5)) {
                        let top = (y + 1) as f32;
                        best = Some(best.map_or(top, |b: f32| b.max(top)));
                        break;
                    }
                    y -= 1;
                }
            }
        }
        best
    }

    /// Posa la camara sobre la **superficie** del terreno bajo su huella `(x, z)`
    /// (evita quedar atrapado bajo tierra o con parte del cuerpo dentro de un
    /// escalon vecino al arrancar).
    ///
    /// Buscamos de arriba hacia abajo la superficie mas alta que cubre la huella
    /// del jugador. Mirar solo la columna del centro seria un error: con terreno
    /// por bloque, el jugador puede aparecer sobre un borde y su caja solaparia
    /// el bloque del escalon de al lado.
    pub fn settle(&mut self, camera: &mut Camera, is_solid: impl Fn(Vec3) -> bool) {
        let x = camera.position.x;
        let z = camera.position.z;

        if let Some(surface) = Self::top_surface(x, z, &is_solid) {
            camera.position.y = surface + EYE_HEIGHT;
            self.vertical_velocity = 0.0;
            self.on_ground = true;
            camera.update_view();
            return;
        }

        // Sin terreno solido bajo la huella: dejamos la camara alta y que caiga.
        camera.position.y = crate::world::WORLD_HEIGHT as f32 - 1.0 + EYE_HEIGHT;
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

    // --- colision horizontal (move_horizontal) --------------------------------
    // El jugador camina a `camera.speed` (8 u/s); con dt=1/60 avanza ~0.133.

    fn test_camera_at(x: f32, z: f32) -> Camera {
        let mut camera = Camera::new(Vec3::new(x, EYE_HEIGHT, z));
        camera.yaw_deg = 0.0; // fwd = -Z, right = +X
        camera.update_view();
        camera
    }

    #[test]
    fn en_espacio_abierto_se_mueve_libre() {
        let mut camera = test_camera_at(0.5, 0.5);
        let mut player = PlayerController::new();
        let open = |_: Vec3| false;
        player.move_horizontal(&mut camera, open, 0.0, 1.0, 1.0 / 60.0);
        // Se movio ~0.133 en +X (right).
        assert!((camera.position.x - (0.5 + 8.0 / 60.0)).abs() < 1e-3);
        assert!((camera.position.z - 0.5).abs() < 1e-4);
    }

    #[test]
    fn no_atraviesa_una_pared() {
        // Pared solida en x >= 2. El jugador parte en x=1.5 y empuja +X 2 s.
        let mut camera = test_camera_at(1.5, 0.5);
        let mut player = PlayerController::new();
        let wall = |p: Vec3| p.x >= 2.0;
        for _ in 0..120 {
            player.move_horizontal(&mut camera, wall, 0.0, 1.0, 1.0 / 60.0);
        }
        // No entra en la pared (x + radio debe quedar por debajo de 2).
        assert!(
            camera.position.x + PLAYER_RADIUS < 2.0 + 1e-3,
            "x={} (entro en la pared)",
            camera.position.x
        );
        // Pero si se acerco (no se quedo clavado al primer paso).
        assert!(camera.position.x > 1.5, "no se acerco a la pared");
    }

    #[test]
    fn se_desliza_a_lo_largo_de_la_pared() {
        // Pared en x >= 2. Empujamos en diagonal (+X bloqueado, -Z libre): el
        // jugador debe deslizarse por Z aunque X se cancele.
        let mut camera = test_camera_at(1.5, 0.5);
        let mut player = PlayerController::new();
        let wall = |p: Vec3| p.x >= 2.0;
        for _ in 0..60 {
            player.move_horizontal(&mut camera, wall, 1.0, 1.0, 1.0 / 60.0);
        }
        assert!(camera.position.z < 0.5 - 0.5, "no se deslizo por Z");
        assert!(
            camera.position.x + PLAYER_RADIUS < 2.0 + 1e-3,
            "X atraveso la pared"
        );
    }

    #[test]
    fn no_colisiona_con_el_bloque_del_suelo() {
        // Suelo solido en y < 4; el jugador esta de pie con los pies en y=4.
        // Moverse en horizontal no debe chocar con el suelo.
        let mut camera = test_camera_at(0.5, 0.5);
        camera.position.y = 4.0 + EYE_HEIGHT;
        camera.update_view();
        let mut player = PlayerController::new();
        let floor = |p: Vec3| p.y < 4.0;
        player.move_horizontal(&mut camera, floor, 0.0, 1.0, 1.0 / 60.0);
        assert!(
            camera.position.x > 0.5 + 0.1,
            "deberia moverse sobre el suelo, x={}",
            camera.position.x
        );
    }

    #[test]
    fn sube_un_escalon_de_un_bloque() {
        // Suelo en y<4 y un escalon (capa y=4, techo 5) en x>=1.
        let mut camera = test_camera_at(0.5, 0.5);
        camera.position.y = 4.0 + EYE_HEIGHT;
        camera.update_view();
        let mut player = PlayerController::new();
        let step = |p: Vec3| p.y < 4.0 || (p.x >= 1.0 && (4.0..5.0).contains(&p.y));
        for _ in 0..120 {
            player.move_horizontal(&mut camera, step, 0.0, 1.0, 1.0 / 60.0);
        }
        assert!(
            camera.position.x > 1.0,
            "no subio el escalon, x={}",
            camera.position.x
        );
        assert!(
            (camera.position.y - (5.0 + EYE_HEIGHT)).abs() < 0.05,
            "quedo a la altura equivocada, y={}",
            camera.position.y
        );
    }

    #[test]
    fn no_sube_un_muro_de_dos_bloques() {
        // Muro de dos bloques (capa y=4 y y=5) en x>=1: hay que saltarlo, no
        // debe subirse caminando.
        let mut camera = test_camera_at(0.5, 0.5);
        camera.position.y = 4.0 + EYE_HEIGHT;
        camera.update_view();
        let mut player = PlayerController::new();
        let wall = |p: Vec3| p.y < 4.0 || (p.x >= 1.0 && (4.0..6.0).contains(&p.y));
        for _ in 0..120 {
            player.move_horizontal(&mut camera, wall, 0.0, 1.0, 1.0 / 60.0);
        }
        assert!(
            camera.position.x + PLAYER_RADIUS < 1.0 + 1e-3,
            "atraveso el muro, x={}",
            camera.position.x
        );
        assert!(
            (camera.position.y - (4.0 + EYE_HEIGHT)).abs() < 1e-3,
            "subio de mas, y={}",
            camera.position.y
        );
    }

    // --- simulacion sobre terreno real (busca embebido) -----------------------

    fn world_solid(world: &crate::world::World, p: Vec3) -> bool {
        world.is_solid([p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32])
    }

    fn box_overlaps_solid(pos: Vec3, world: &crate::world::World) -> bool {
        PlayerController::collides(pos, &|p| world_solid(world, p))
    }

    #[test]
    fn caminata_por_terreno_real_no_queda_embebido() {
        let seed = 13_371;
        let mut world = crate::world::World::new(seed, 4, vec![]);
        world.update_streaming([8.0, 74.0, 20.0]);

        let mut camera = Camera::new(Vec3::new(8.0, 76.0, 20.0));
        camera.update_view();
        let mut player = PlayerController::new();
        player.settle(&mut camera, |p| world_solid(&world, p));

        assert!(
            !box_overlaps_solid(camera.position, &world),
            "el spawn ya nace embebido: {:?}",
            camera.position
        );

        let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut fwd = 1.0f32;
        let mut right = 0.0f32;
        for frame in 0..20_000 {
            if frame % 17 == 0 {
                rng = rng
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let a = ((rng >> 40) as f32 / (1u64 << 24) as f32) * std::f32::consts::TAU;
                fwd = a.cos();
                right = a.sin();
                camera.yaw_deg = ((rng >> 24) % 360) as f32;
            }
            player.move_horizontal(
                &mut camera,
                |p| world_solid(&world, p),
                fwd,
                right,
                1.0 / 60.0,
            );
            player.update(
                &mut camera,
                |p| world_solid(&world, p),
                0.0,
                false,
                false,
                1.0 / 60.0,
            );
            world.update_streaming([camera.position.x, camera.position.y, camera.position.z]);
            assert!(
                !box_overlaps_solid(camera.position, &world),
                "frame {frame}: jugador embebido en el terreno en {:?}",
                camera.position
            );
        }
    }
}
