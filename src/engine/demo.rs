//! Escena de demostracion para las capturas (`SOLARIA_DEMO=1`).
//!
//! Yo (el asistente) no puedo hacer click en la app, asi que necesito una forma
//! de montar una escena concreta sin interactuar. Con `SOLARIA_DEMO` en el
//! entorno, al arrancar se alisa una parcela, se planta una antorcha y se apunta
//! la camara a ella, para verificar el modelo (la cruz de dos quads con cutout)
//! y la luz de bloque.
//!
//! La camara queda **fija**: `App::update` no aplica fisica ni resaltado cuando
//! el modo demo esta activo (si no, la vista se desplazaria antes de la foto).

use crate::math::Vec3;
use crate::player::EYE_HEIGHT;
use crate::render::Renderer;
use crate::scene::Camera;
use crate::world::Block;

/// ¿Esta activo el modo demo? (variable de entorno `SOLARIA_DEMO`).
pub fn is_active() -> bool {
    std::env::var("SOLARIA_DEMO").is_ok()
}

/// Hora del dia a la que arranca el demo (`SOLARIA_TIME`, 0..1). Por defecto
/// media manana (0.35). Permite capturar de dia (0.5) o de noche (0.0).
pub fn time_of_day() -> f32 {
    std::env::var("SOLARIA_TIME")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.35)
        .rem_euclid(1.0)
}

/// ¿Mostrar el test de colision horizontal? (`SOLARIA_COLLIDE`).
pub fn collide_active() -> bool {
    std::env::var("SOLARIA_COLLIDE").is_ok()
}

/// ¿Mostrar la vista aerea de biomas? (`SOLARIA_BIOMES`).
pub fn biomes_active() -> bool {
    std::env::var("SOLARIA_BIOMES").is_ok()
}

/// Vista aerea para ver los biomas (`SOLARIA_BIOMES=1`): sube la camara y mira
/// hacia abajo, sin tocar el terreno, para apreciar las manchas de
/// arena/hierba/nieve que reparte el ruido de Worley.
pub fn build_overview(camera: &mut Camera) {
    camera.position = Vec3::new(8.0, 150.0, 20.0);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -52.0;
    camera.update_view();
}

/// Escena para **verificar la colision horizontal**: una pared solida delante y
/// el jugador empujando hacia ella durante 3 s (sin input real: llamamos a
/// `move_horizontal` en un bucle). Al capturar debe verse la pared de cerca y
/// **no su interior**, prueba de que el jugador no la atraviesa.
pub fn build_collision(renderer: &mut Renderer, camera: &mut Camera) {
    let cx = camera.position.x.floor() as i32;
    let cz = camera.position.z.floor() as i32;
    let feet = (camera.position.y - EYE_HEIGHT).floor() as i32;
    let plateau = feet + 1;

    let mut edits: Vec<([i32; 3], Block)> = Vec::new();
    // Plataforma plana.
    for x in (cx - 3)..=(cx + 3) {
        for z in (cz - 6)..=(cz + 3) {
            edits.push(([x, plateau, z], Block::Grass));
            for y in (plateau + 1)..(plateau + 8) {
                edits.push(([x, y, z], Block::Air));
            }
        }
    }
    // Pared solida de 7x3 a 3 bloques al frente (-Z).
    for dx in -3..=3 {
        for dy in 0..3 {
            edits.push(([cx + dx, plateau + 1 + dy, cz - 3], Block::Stone));
        }
    }
    renderer.set_blocks(&edits);

    // Jugador a 2 bloques de la pared, empujando hacia -Z (yaw 0, forward=1).
    camera.position = Vec3::new(
        cx as f32 + 0.5,
        plateau as f32 + 1.0 + EYE_HEIGHT,
        cz as f32 + 0.5,
    );
    camera.yaw_deg = 0.0;
    camera.pitch_deg = 0.0;
    camera.update_view();

    let mut player = crate::player::PlayerController::new();
    let query = |p: Vec3| renderer.is_solid_at(p);
    for _ in 0..180 {
        player.move_horizontal(camera, query, 1.0, 0.0, 1.0 / 60.0);
    }
    camera.pitch_deg = -5.0;
    camera.update_view();
    println!(
        "[engine] demo: colision -> jugador en z={:.2}",
        camera.position.z
    );
}

/// Monta la escena de captura: una parcela plana de hierba con una antorcha
/// delante, y coloca la camara mirandola desde cerca.
///
/// Devuelve las coordenadas de la antorcha (util para depurar/tests).
pub fn build(renderer: &mut Renderer, camera: &mut Camera) -> [i32; 3] {
    let cx = camera.position.x.floor() as i32;
    let cz = camera.position.z.floor() as i32;
    let feet = (camera.position.y - EYE_HEIGHT).floor() as i32;
    // Plataforma: solidos hasta `plateau`, aire por encima.
    let plateau = feet + 1;

    // Reunimos todas las ediciones y las aplicamos en UN lote: cada `set_block`
    // reconstruye mallas, asi que hacerlo de uno en uno tardaria mucho.
    let mut edits: Vec<([i32; 3], Block)> = Vec::new();

    // 1. Alisamos una parcela pequena (7x10) alrededor del jugador.
    for x in (cx - 3)..=(cx + 3) {
        for z in (cz - 6)..=(cz + 3) {
            edits.push(([x, plateau, z], Block::Grass));
            for y in (plateau + 1)..(plateau + 8) {
                edits.push(([x, y, z], Block::Air));
            }
        }
    }

    // 2. UNA antorcha a 5 bloques al frente (-Z).
    let torch = [cx, plateau + 1, cz - 5];
    edits.push((torch, Block::Torch));

    renderer.set_blocks(&edits);

    // 3. Camara: a 2.3 del suelo y 5.0 al frente, mirando a la antorcha.
    camera.position = Vec3::new(cx as f32 + 0.5, plateau as f32 + 2.3, cz as f32 - 0.5);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -13.7;
    camera.update_view();

    // Sin resaltado: queremos ver el modelo limpio.
    renderer.set_highlight(None);

    torch
}
