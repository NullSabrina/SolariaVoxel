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

    // 1. Alisamos una parcela pequena (7x10) alrededor del jugador.
    for x in (cx - 3)..=(cx + 3) {
        for z in (cz - 6)..=(cz + 3) {
            renderer.set_block([x, plateau, z], Block::Grass);
            for y in (plateau + 1)..(plateau + 8) {
                renderer.set_block([x, y, z], Block::Air);
            }
        }
    }

    // 2. UNA antorcha a 5 bloques al frente (-Z).
    let torch = [cx, plateau + 1, cz - 5];
    renderer.set_block(torch, Block::Torch);

    // 3. Camara: a 2.3 del suelo y 5.0 al frente, mirando a la antorcha.
    camera.position = Vec3::new(cx as f32 + 0.5, plateau as f32 + 2.3, cz as f32 - 0.5);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -13.7;
    camera.update_view();

    // Sin resaltado: queremos ver el modelo limpio.
    renderer.set_highlight(None);

    torch
}
