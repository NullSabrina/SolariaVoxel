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
use crate::world::{Block, SEA_LEVEL};

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

/// Orientacion de la camara para capturas (`SOLARIA_LOOK=yaw,pitch`, grados).
/// Permite apuntar al sol o a la luna sin tocar el codigo de cada demo.
pub fn apply_look(camera: &mut Camera) {
    if let Ok(s) = std::env::var("SOLARIA_LOOK") {
        let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if parts.len() == 2 {
            camera.yaw_deg = parts[0];
            camera.pitch_deg = parts[1];
            camera.update_view();
        }
    }
}

/// Dia de juego inicial (`SOLARIA_DAY`), para probar las fases lunares.
pub fn day_count() -> Option<u64> {
    std::env::var("SOLARIA_DAY")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
}

/// ¿Mostrar el test de colision horizontal? (`SOLARIA_COLLIDE`).
pub fn collide_active() -> bool {
    std::env::var("SOLARIA_COLLIDE").is_ok()
}

/// ¿Mostrar la vista aerea de biomas? (`SOLARIA_BIOMES`).
pub fn biomes_active() -> bool {
    std::env::var("SOLARIA_BIOMES").is_ok()
}

/// ¿Mostrar una vista de oceano? (`SOLARIA_OCEAN`).
pub fn ocean_active() -> bool {
    std::env::var("SOLARIA_OCEAN").is_ok()
}

/// Vista de **oceano** (`SOLARIA_OCEAN=1`): busca una columna cercana cuyo fondo
/// este por debajo del nivel del mar y coloca la camara elevada mirandola, para
/// ver el agua translucida y la playa.
pub fn build_ocean_overview(renderer: &Renderer, camera: &mut Camera) {
    let sea = SEA_LEVEL;
    let mut target: Option<(i32, i32, i32)> = None;
    'search: for cz in -6..6 {
        for cx in -6..6 {
            let x = cx * 16 + 8;
            let z = cz * 16 + 8;
            let mut y = crate::world::WORLD_HEIGHT as i32 - 1;
            while y > 0 {
                if renderer.is_solid_at(Vec3::new(x as f32, y as f32, z as f32)) {
                    if y < sea - 2 {
                        target = Some((x, y, z));
                        break 'search;
                    }
                    break;
                }
                y -= 1;
            }
        }
    }
    let (x, s, z) = target.unwrap_or((8, sea - 4, 20));
    camera.position = Vec3::new(x as f32 + 0.5, (s + 28) as f32, z as f32 + 0.5);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -45.0;
    camera.update_view();
    println!("[engine] demo: vista de oceano sobre ({x},{s},{z})");
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

    let applied = renderer.set_blocks(&edits);
    // 3. Camara: a 2.3 del suelo y 5.0 al frente, mirando a la antorcha.
    camera.position = Vec3::new(cx as f32 + 0.5, plateau as f32 + 2.3, cz as f32 - 0.5);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -13.7;
    camera.update_view();
    println!(
        "[demo] aplicados {applied}/{} edits, antorcha en {torch:?}, camara=({:.1},{:.1},{:.1})",
        edits.len(),
        camera.position.x,
        camera.position.y,
        camera.position.z
    );

    // Sin resaltado: queremos ver el modelo limpio.
    renderer.set_highlight(None);

    torch
}

/// ¿Mostrar la mesa de crafteo abierta? (`SOLARIA_CRAFT`).
pub fn craft_active() -> bool {
    std::env::var("SOLARIA_CRAFT").is_ok()
}

/// ¿Mostrar una poza de lava subterranea? (`SOLARIA_CAVE`).
pub fn cave_active() -> bool {
    std::env::var("SOLARIA_CAVE").is_ok()
}

/// ¿Mostrar un rio? (`SOLARIA_RIVER`).
pub fn river_active() -> bool {
    std::env::var("SOLARIA_RIVER").is_ok()
}

/// Escena de **rio** (`SOLARIA_RIVER=1`): busca agua por encima del nivel del mar
/// (un cauce generado) en el area cargada y coloca la camara elevada mirandolo.
/// Si no encuentra ninguno, cae a una vista generica.
pub fn build_river(renderer: &mut Renderer, camera: &mut Camera) {
    let mut found: Option<[i32; 3]> = None;
    'scan: for z in (-60..60).step_by(2) {
        for x in (-60..60).step_by(2) {
            for y in (SEA_LEVEL + 1..SEA_LEVEL + 30).rev() {
                if renderer.block_at([x, y, z]) == Block::Water {
                    found = Some([x, y, z]);
                    break 'scan;
                }
            }
        }
    }
    let [x, y, z] = found.unwrap_or([8, SEA_LEVEL + 1, 20]);
    camera.position = Vec3::new(x as f32 + 0.5, (y + 12) as f32, z as f32 + 18.0);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -30.0;
    camera.update_view();
    renderer.set_highlight(None);
    println!("[engine] demo: rio en ({x},{y},{z})");
}

/// Escena de cueva (`SOLARIA_CAVE=1`): busca una poza de lava real generada
/// por el terreno, vacia una sala a su alrededor y coloca la camara dentro
/// mirandola. Si no hay ninguna cerca, talla una de muestra y lo avisa.
pub fn build_cave(renderer: &mut Renderer, camera: &mut Camera) {
    let cx = camera.position.x.floor() as i32;
    let cz = camera.position.z.floor() as i32;

    // 1. Buscar lava generada en un radio de 48 (da igual que este tapada:
    // la sala la deja al descubierto).
    let mut found: Option<[i32; 3]> = None;
    'scan: for dz in -48..=48 {
        for dx in -48..=48 {
            for y in 6..12 {
                if renderer.block_at([cx + dx, y, cz + dz]) == Block::Lava {
                    found = Some([cx + dx, y, cz + dz]);
                    break 'scan;
                }
            }
        }
    }

    let (lx, ly, lz, natural) = match found {
        Some(p) => (p[0], p[1], p[2], true),
        // Plan B: sala de muestra tallada a mano (el generador se verifica
        // con tests; aqui importa el render y la luz).
        None => (cx, 8, cz, false),
    };

    // 2. Vaciar una sala de 7x5x7 alrededor (se conserva el suelo y la lava:
    // lo segundo es justo lo que venimos a ver).
    let mut edits: Vec<([i32; 3], Block)> = Vec::new();
    for dy in 0..5 {
        for dz in -3..=3 {
            for dx in -3..=3 {
                let p = [lx + dx, ly + dy, lz + dz];
                if renderer.block_at(p) != Block::Lava {
                    edits.push((p, Block::Air));
                }
            }
        }
    }
    if !natural {
        // Poza de muestra 3x3 con suelo de obsidiana.
        for dz in -1..=1 {
            for dx in -1..=1 {
                edits.push(([lx + dx, ly, lz + dz], Block::Lava));
                edits.push(([lx + dx, ly - 1, lz + dz], Block::Obsidian));
            }
        }
    }
    renderer.set_blocks(&edits);

    // 3. Camara dentro de la sala, mirando a la lava.
    camera.position = Vec3::new(lx as f32 - 2.0, ly as f32 + 2.0, lz as f32 + 2.0);
    camera.yaw_deg = 59.0;
    camera.pitch_deg = -27.0;
    camera.update_view();
    renderer.set_highlight(None);
    println!("[engine] demo: cueva en [{lx},{ly},{lz}] (natural: {natural})");
}

/// Escena de la mesa de crafteo (`SOLARIA_CRAFT=1`): parcela plana, una mesa
/// delante y la interfaz abierta con 2x2 de tablones (resultado: mesa).
/// Devuelve `(mesa, rejilla, resultado)` para que `App` abra la UI igual que
/// con un click derecho real.
pub fn build_crafting(
    renderer: &mut Renderer,
    camera: &mut Camera,
) -> ([i32; 3], [Option<Block>; 9], Option<Block>) {
    let cx = camera.position.x.floor() as i32;
    let cz = camera.position.z.floor() as i32;
    let feet = (camera.position.y - EYE_HEIGHT).floor() as i32;
    let plateau = feet + 1;

    let mut edits: Vec<([i32; 3], Block)> = Vec::new();
    for x in (cx - 3)..=(cx + 3) {
        for z in (cz - 6)..=(cz + 3) {
            edits.push(([x, plateau, z], Block::Grass));
            for y in (plateau + 1)..(plateau + 8) {
                edits.push(([x, y, z], Block::Air));
            }
        }
    }

    // Mesa a 4 bloques al frente (-Z).
    let table = [cx, plateau + 1, cz - 4];
    edits.push((table, Block::CraftingTable));
    renderer.set_blocks(&edits);

    // Rejilla con la receta 2x2 de tablones ya puesta.
    let mut grid = [None; 9];
    for i in [0usize, 1, 3, 4] {
        grid[i] = Some(Block::Planks);
    }
    let result = crate::world::match_recipe(&grid);

    camera.position = Vec3::new(cx as f32 + 0.5, plateau as f32 + 2.3, cz as f32 - 0.5);
    camera.yaw_deg = 0.0;
    camera.pitch_deg = -13.7;
    camera.update_view();
    renderer.set_highlight(None);

    (table, grid, result)
}
