//! El bucle principal de la aplicacion.
//!
//! winit 0.30 usa el patron `ApplicationHandler`: nosotros implementamos una
//! serie de callbacks y winit nos avisa cuando pasa algo (la app arranca, llega
//! un evento de ventana, toca repintar...). No hay un `while` visible: el bucle
//! vive dentro de `event_loop.run_app`.
//!
//! Reparto de responsabilidades:
//! * [`App`] une las piezas: ventana + renderer + escena + input.
//! * [`Input`] acumula el estado de teclado y raton.
//! * [`Renderer`] dibuja. [`Camera`] dice desde donde miramos y se mueve.
//!
//! v0.1.1 anadio la camara FPS (WASD + pointer lock); v0.1.2, el primer cubo;
//! v0.2.0, el primer chunk de voxeles. Cada frame le pasamos al renderer la
//! matriz de la camara.

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::engine::demo;
use crate::engine::input::Input;
use crate::engine::window;
use crate::math::Vec3;
use crate::player::PlayerController;
use crate::render::Renderer;
use crate::scene::{Camera, DayCycle};

/// Estado global de la aplicacion.
///
/// Los campos que dependen de la plataforma son `Option` porque en winit 0.30
/// la ventana (y con ella el renderer) no existen hasta el callback `resumed`.
#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Option<Camera>,
    player: PlayerController,
    input: Input,
    /// ¿Tenemos el cursor capturado (pointer lock)?
    mouse_locked: bool,
    /// Modo vuelo (F): sin gravedad, para explorar.
    flying: bool,
    /// Bloque apuntado por la camara en el ultimo frame (y su cara).
    selection: Option<crate::world::RayHit>,
    /// Barra rapida: 9 ranuras con un bloque cada una.
    hotbar: [crate::world::Block; 9],
    /// Ranura seleccionada de la barra (0..9).
    hotbar_sel: usize,
    /// ¿Esta abierto el inventario? (`E`).
    inventory_open: bool,
    /// ¿Esta abierta la mesa de crafteo? (click derecho sobre una mesa).
    crafting_open: bool,
    /// Rejilla 3x3 de la mesa (fila a fila). Sin conteos: creativo.
    craft_grid: [Option<crate::world::Block>; 9],
    /// Resultado actual de la receta (`None` si no casa ninguna).
    craft_result: Option<crate::world::Block>,
    /// Ultima posicion del cursor en pixels (para el inventario).
    cursor: (f32, f32),
    /// Semilla del mundo (de la partida o cargada de disco).
    seed: u32,
    /// Ficha del mundo con su versionado, para actualizarla al guardar.
    world_header: crate::world::WorldHeader,
    /// Evita guardar dos veces (CloseRequested + exiting).
    world_saved: bool,
    /// Marca de tiempo del frame anterior, para calcular el `dt`.
    last_frame: Option<Instant>,
    /// Acumuladores para mostrar los FPS en el titulo de la ventana.
    fps_frames: u32,
    fps_accum: f32,
    /// Hora del mundo y como afecta a la luz y al cielo.
    day_cycle: DayCycle,
    /// Modo demo (`SOLARIA_DEMO`): congela la camara y elige la escena de la
    /// captura. La fisica y el resaltado se desactivan para que la vista no se
    /// desplace antes de la foto.
    demo: bool,
}

/// Ruta del archivo de mundo por defecto (junto al ejecutable de trabajo).
fn world_path() -> std::path::PathBuf {
    std::path::PathBuf::from("world.vf")
}

/// Segundos desde el epoch de UNIX (para la fecha del header).
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Bloques disponibles en la barra rapida y en el inventario.
///
/// Los tablones NO estan aqui: se obtienen crafteando madera (1) en la mesa.
/// La mesa SI esta: es la puerta de entrada al crafteo.
const ITEMS: [crate::world::Block; 9] = [
    crate::world::Block::Stone,
    crate::world::Block::Dirt,
    crate::world::Block::Grass,
    crate::world::Block::Sand,
    crate::world::Block::Wood,
    crate::world::Block::CraftingTable,
    crate::world::Block::Leaves,
    crate::world::Block::Snow,
    crate::world::Block::Torch,
];

/// Escala de la interfaz (pixels de mundo -> pixels de pantalla).
const UI_SCALE: f32 = 2.0;

/// Indice de ranura para las teclas `1`..`9`.
fn digit_slot(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 0,
        KeyCode::Digit2 => 1,
        KeyCode::Digit3 => 2,
        KeyCode::Digit4 => 3,
        KeyCode::Digit5 => 4,
        KeyCode::Digit6 => 5,
        KeyCode::Digit7 => 6,
        KeyCode::Digit8 => 7,
        KeyCode::Digit9 => 8,
        _ => return None,
    })
}

/// Arranca el motor: crea el bucle de eventos y lo ejecuta.
///
/// `ControlFlow::Poll` hace que el bucle no duerma esperando eventos, lo que
/// es lo que queremos para un juego que repinta continuamente.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl App {
    /// Captura el cursor dentro de la ventana (pointer lock) y lo oculta.
    ///
    /// En Windows esto solo funciona con la ventana enfocada, por eso lo
    /// hacemos al hacer click. Si falla, avisamos pero no reventamos: el
    /// usuario puede volver a intentarlo.
    fn lock_mouse(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        match window.set_cursor_grab(CursorGrabMode::Locked) {
            Ok(()) => {
                window.set_cursor_visible(false);
                self.mouse_locked = true;
                println!("[input] cursor capturado (Escape para liberar)");
            }
            Err(e) => eprintln!("[input] no se pudo capturar el cursor: {e}"),
        }
    }

    /// Libera el cursor y lo vuelve a mostrar.
    fn unlock_mouse(&mut self) {
        if let Some(window) = self.window.as_ref() {
            // Ignoramos el error: si nunca se capturo, liberar puede fallar.
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
        self.mouse_locked = false;
    }

    /// Un tick de simulacion: giro del raton, desplazamiento horizontal y
    /// fisica vertical (gravedad/suelo) del jugador.
    fn update(&mut self, dt: f32) {
        // El tiempo del mundo avanza siempre (salvo en demo, que lo congela).
        if !self.demo {
            self.day_cycle.advance(dt);
        }

        // Leemos TODO el input primero, para no mezclar prestamos.
        let (dx, dy) = self.input.take_mouse_delta();
        let forward = self.input.forward_axis();
        let right = self.input.right_axis();
        let jump = self.input.jump_axis();
        let jump_held = self.input.jump_held();
        let flying = self.flying;

        // Necesitamos el renderer (para consultar bloques) y la camara a la vez.
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let Some(camera) = self.camera.as_mut() else {
            return;
        };

        // Giro.
        if self.mouse_locked && (dx != 0.0 || dy != 0.0) {
            camera.add_look(dx, dy);
        }

        // En modo demo la camara queda fija: no aplicamos la fisica, para que la
        // vista de la captura no se desplace antes de la foto.
        if self.demo {
            return;
        }

        // Fisica. La consulta de solido mira el mundo en coordenadas de bloque
        // (cualquier columna cargada).
        let world = renderer;
        let is_solid = move |point: Vec3| -> bool { world.is_solid_at(point) };

        // Movimiento horizontal CON colision (no atravesamos paredes).
        if forward != 0.0 || right != 0.0 {
            let mut player = self.player;
            player.move_horizontal(camera, is_solid, forward, right, dt);
            self.player = player;
        }

        // En modo vuelo, Espacio/Shift suben/bajan; en modo normal Space salta.
        let shift = self.input.is_pressed(winit::keyboard::KeyCode::ShiftLeft)
            || self.input.is_pressed(winit::keyboard::KeyCode::ShiftRight);
        let fly_up = if flying {
            (jump_held as i32 - shift as i32) as f32
        } else {
            0.0
        };
        // ¿Esta el jugador en el agua? (cabeza o pies dentro de agua) -> flota.
        let feet = Vec3::new(
            camera.position.x,
            camera.position.y - crate::player::EYE_HEIGHT,
            camera.position.z,
        );
        let in_water = world.is_water_at(camera.position) || world.is_water_at(feet);
        let mut player = self.player;
        player.update(
            camera,
            is_solid,
            fly_up,
            flying,
            jump && !flying,
            in_water,
            dt,
        );
        self.player = player;
    }

    /// Raycast desde el ojo del jugador en la direccion en que mira y actualiza
    /// el resaltado del bloque apuntado.
    fn update_selection(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(camera) = self.camera.as_ref() else {
            return;
        };
        let hit = renderer.raycast(camera.position, camera.forward(), 6.0);
        renderer.set_highlight(hit);
        self.selection = hit;
    }

    /// Rompe el bloque apuntado (si lo hay).
    fn break_block(&mut self) {
        let Some(hit) = self.selection else {
            return;
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_block(hit.block, crate::world::Block::Air);
            println!("[edit] bloque roto en {:?}", hit.block);
        }
        self.update_selection();
    }

    /// Guarda el mundo a disco (semilla + TODOS los chunks editados). Solo una
    /// vez por ejecucion.
    fn save_world(&mut self) {
        if self.world_saved {
            return;
        }
        self.world_saved = true;
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let mut save = crate::world::WorldSave::new(self.seed, self.world_header.created_at);
        // Conservamos las versiones del header original.
        save.header = self.world_header.clone();
        // Volcamos todas las columnas que el jugador ha modificado.
        for (pos, record) in renderer.snapshot_modified() {
            save.set_chunk(pos, record);
        }
        // Guardado completo: la posicion del jugador.
        if let Some(camera) = self.camera.as_ref() {
            save.player_pos = [camera.position.x, camera.position.y, camera.position.z];
        }
        // Ratio de compresion medio (raw / comprimido) de los chunks.
        let ratio = if save.chunks.is_empty() {
            1.0
        } else {
            let sum: f32 = save.chunks.values().map(|r| r.compression_ratio()).sum();
            sum / save.chunks.len() as f32
        };
        match save.save_to(&world_path()) {
            Ok(()) => {
                let size = std::fs::metadata(world_path())
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!(
                    "[world] guardado en {:?}: {} chunks editados, {} bytes, LZ4 x{:.1}",
                    world_path(),
                    save.chunks.len(),
                    size,
                    ratio
                );
            }
            Err(e) => eprintln!("[world] no se pudo guardar: {e}"),
        }
    }

    /// Coloca un bloque en el aire contiguo al apuntado (si lo hay y no choca
    /// con el jugador).
    fn place_block(&mut self) {
        let Some(hit) = self.selection else {
            return;
        };
        let (ox, oy, oz) = hit.face.offset();
        let target = [hit.block[0] + ox, hit.block[1] + oy, hit.block[2] + oz];

        // Evitamos colocar un bloque dentro del propio jugador.
        if let Some(camera) = self.camera.as_ref() {
            if crate::player::block_overlaps_player(target, camera.position) {
                return;
            }
        }
        if let Some(renderer) = self.renderer.as_mut() {
            let block = self.hotbar[self.hotbar_sel];
            renderer.set_block(target, block);
            println!("[edit] colocado {block:?} en {target:?}");
        }
        self.update_selection();
    }

    /// Tamano de la ventana en pixels logicos (o (1,1) si aun no hay ventana).
    fn window_size_f(&self) -> (f32, f32) {
        self.window
            .as_ref()
            .map(|w| {
                let s = w.inner_size();
                (s.width as f32, s.height as f32)
            })
            .unwrap_or((1.0, 1.0))
    }

    /// Celdas (rectangulos) del inventario 3x3, centradas en la ventana.
    fn inventory_cells(&self, win_w: f32, win_h: f32) -> Vec<[f32; 4]> {
        use crate::render::gui;
        let slot = gui::SLOT as f32 * UI_SCALE;
        let gap = 6.0;
        let (cols, rows) = (3usize, 3usize);
        let grid_w = cols as f32 * slot + (cols as f32 - 1.0) * gap;
        let grid_h = rows as f32 * slot + (rows as f32 - 1.0) * gap;
        let x0 = (win_w - grid_w) * 0.5;
        let y0 = (win_h - grid_h) * 0.5;
        let mut out = Vec::with_capacity(cols * rows);
        for i in 0..(cols * rows) {
            let cx = x0 + (i % cols) as f32 * (slot + gap);
            let cy = y0 + (i / cols) as f32 * (slot + gap);
            out.push([cx, cy, slot, slot]);
        }
        out
    }

    /// Construye los quads de la interfaz (hotbar + inventario).
    fn build_ui(&self, win_w: f32, win_h: f32) -> Vec<crate::render::UiQuad> {
        use crate::render::{UiQuad, gui, region_uv};
        use crate::world::Face;
        let mut quads: Vec<UiQuad> = Vec::new();
        let slot = gui::SLOT as f32 * UI_SCALE;
        let inset = 3.0 * UI_SCALE;

        // Hotbar centrada abajo.
        let bar_w = gui::HOTBAR.w as f32 * UI_SCALE;
        let bar_h = gui::HOTBAR.h as f32 * UI_SCALE;
        let bar_x = ((win_w - bar_w) * 0.5).floor();
        let bar_y = (win_h - bar_h - 8.0).floor();
        quads.push(UiQuad {
            rect: [bar_x, bar_y, bar_w, bar_h],
            uv: region_uv(gui::HOTBAR),
            layer: -1,
        });
        for (i, (cell, item)) in self
            .hotbar_cells(win_w, win_h)
            .iter()
            .zip(self.hotbar.iter())
            .enumerate()
        {
            let [sx, sy, _, _] = *cell;
            if i == self.hotbar_sel {
                quads.push(UiQuad {
                    rect: [sx, sy, slot, slot],
                    uv: region_uv(gui::SELECTION),
                    layer: -1,
                });
            }
            quads.push(UiQuad {
                rect: [
                    sx + inset,
                    sy + inset,
                    slot - 2.0 * inset,
                    slot - 2.0 * inset,
                ],
                uv: [0.0, 0.0, 1.0, 1.0],
                layer: item.face_tile(Face::PosY) as i32,
            });
        }

        // Mesa de crafteo: rejilla 3x3 + flecha + resultado (nuestra hotbar
        // sigue abajo; el inventario tambien se muestra como fuente).
        if self.crafting_open {
            let (cells, arrow, result) = self.crafting_layout(win_w, win_h);
            for (j, cell) in cells.iter().enumerate() {
                let [cx, cy, cw, ch] = *cell;
                quads.push(UiQuad {
                    rect: [cx, cy, cw, ch],
                    uv: region_uv(gui::SLOT_REGION),
                    layer: -1,
                });
                if let Some(b) = self.craft_grid[j] {
                    quads.push(UiQuad {
                        rect: [cx + inset, cy + inset, cw - 2.0 * inset, ch - 2.0 * inset],
                        uv: [0.0, 0.0, 1.0, 1.0],
                        layer: b.face_tile(Face::PosY) as i32,
                    });
                }
            }
            quads.push(UiQuad {
                rect: arrow,
                uv: region_uv(gui::ARROW),
                layer: -1,
            });
            {
                let [rx, ry, rw, rh] = result;
                quads.push(UiQuad {
                    rect: [rx, ry, rw, rh],
                    uv: region_uv(gui::SLOT_REGION),
                    layer: -1,
                });
                if let Some(b) = self.craft_result {
                    quads.push(UiQuad {
                        rect: [rx + inset, ry + inset, rw - 2.0 * inset, rh - 2.0 * inset],
                        uv: [0.0, 0.0, 1.0, 1.0],
                        layer: b.face_tile(Face::PosY) as i32,
                    });
                    quads.push(UiQuad {
                        rect: [rx, ry, rw, rh],
                        uv: region_uv(gui::SELECTION),
                        layer: -1,
                    });
                }
            }
        }

        // Inventario: rejilla 3x3 con todos los bloques disponibles.
        if self.inventory_open {
            for (cell, item) in self.inventory_cells(win_w, win_h).iter().zip(ITEMS.iter()) {
                let [cx, cy, cw, ch] = *cell;
                quads.push(UiQuad {
                    rect: [cx, cy, cw, ch],
                    uv: region_uv(gui::SLOT_REGION),
                    layer: -1,
                });
                quads.push(UiQuad {
                    rect: [cx + inset, cy + inset, cw - 2.0 * inset, ch - 2.0 * inset],
                    uv: [0.0, 0.0, 1.0, 1.0],
                    layer: item.face_tile(Face::PosY) as i32,
                });
            }
        }
        quads
    }

    /// Un click en el inventario: elige el bloque de la celda pulsada.
    fn inventory_click(&mut self) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        for (cell, item) in self.inventory_cells(win_w, win_h).iter().zip(ITEMS.iter()) {
            let [x, y, w, h] = *cell;
            if mx >= x && mx < x + w && my >= y && my < y + h {
                self.hotbar[self.hotbar_sel] = *item;
                println!("[engine] ranura {} = {item:?}", self.hotbar_sel + 1);
                return;
            }
        }
    }

    /// Rectangulos de las 9 ranuras de la hotbar (misma matematica que
    /// `build_ui`, para que clic y dibujo coincidan).
    fn hotbar_cells(&self, win_w: f32, win_h: f32) -> Vec<[f32; 4]> {
        use crate::render::gui;
        let slot = gui::SLOT as f32 * UI_SCALE;
        let bar_w = gui::HOTBAR.w as f32 * UI_SCALE;
        let bar_h = gui::HOTBAR.h as f32 * UI_SCALE;
        let bar_x = ((win_w - bar_w) * 0.5).floor();
        let bar_y = (win_h - bar_h - 8.0).floor();
        (0..9)
            .map(|i| {
                let sx = bar_x + (1.0 + i as f32 * gui::SLOT as f32) * UI_SCALE;
                let sy = bar_y + UI_SCALE;
                [sx, sy, slot, slot]
            })
            .collect()
    }

    /// Layout de la ventana de crafteo: 9 celdas 3x3, flecha y resultado.
    /// Devuelve `(celdas, flecha, resultado)`, siempre encima del inventario.
    fn crafting_layout(&self, win_w: f32, win_h: f32) -> (Vec<[f32; 4]>, [f32; 4], [f32; 4]) {
        use crate::render::gui;
        let slot = gui::SLOT as f32 * UI_SCALE;
        let gap = 6.0;
        let grid = 3.0 * slot + 2.0 * gap;
        let arrow_w = gui::ARROW.w as f32 * UI_SCALE;
        let arrow_h = gui::ARROW.h as f32 * UI_SCALE;
        let row_w = grid + 12.0 + arrow_w + 12.0 + slot;
        let x0 = (win_w - row_w) * 0.5;
        let inv_top = self.inventory_cells(win_w, win_h)[0][1];
        let gy = (inv_top - grid - 24.0).max(8.0);
        let mut cells = Vec::with_capacity(9);
        for i in 0..9 {
            cells.push([
                x0 + (i % 3) as f32 * (slot + gap),
                gy + (i / 3) as f32 * (slot + gap),
                slot,
                slot,
            ]);
        }
        let ax = x0 + grid + 12.0;
        let arrow = [ax, gy + (grid - arrow_h) * 0.5, arrow_w, arrow_h];
        let rx = ax + arrow_w + 12.0;
        let result = [rx, gy + (grid - slot) * 0.5, slot, slot];
        (cells, arrow, result)
    }

    /// Recalcula el resultado segun la rejilla.
    fn refresh_craft_result(&mut self) {
        self.craft_result = crate::world::match_recipe(&self.craft_grid);
    }

    /// Cierra la mesa (limpia rejilla y resultado).
    fn close_crafting(&mut self) {
        self.crafting_open = false;
        self.craft_grid = [None; 9];
        self.craft_result = None;
    }

    /// Un click con la mesa abierta: resultado, rejilla, inventario u hotbar.
    ///
    /// Sin "mano": el click en el inventario pone el bloque en la primera celda
    /// libre, el click en una celda la limpia, y el click en el resultado lo
    /// asigna a la ranura activa y consume la rejilla.
    fn crafting_click(&mut self) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        let inside =
            |r: &[f32; 4]| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3];

        let (cells, _arrow, result) = self.crafting_layout(win_w, win_h);

        // 1. Resultado: asigna a la ranura activa y consume la rejilla.
        if inside(&result) {
            if let Some(out) = self.craft_result {
                self.hotbar[self.hotbar_sel] = out;
                println!("[crafteo] {out:?} -> ranura {}", self.hotbar_sel + 1);
                self.craft_grid = [None; 9];
                self.craft_result = None;
            }
            return;
        }
        // 2. Rejilla: limpia la celda pulsada.
        for (j, cell) in cells.iter().enumerate() {
            if inside(cell) {
                self.craft_grid[j] = None;
                self.refresh_craft_result();
                return;
            }
        }
        // 3. Inventario: pone el bloque en la primera celda libre.
        for (cell, item) in self.inventory_cells(win_w, win_h).iter().zip(ITEMS.iter()) {
            if inside(cell) {
                if let Some(j) = self.craft_grid.iter().position(|c| c.is_none()) {
                    self.craft_grid[j] = Some(*item);
                    self.refresh_craft_result();
                    println!("[crafteo] {item:?} -> celda {}", j + 1);
                }
                return;
            }
        }
        // 4. Hotbar: elige la ranura destino del resultado.
        for (i, cell) in self.hotbar_cells(win_w, win_h).iter().enumerate() {
            if inside(cell) {
                self.hotbar_sel = i;
                return;
            }
        }
    }
}

impl ApplicationHandler for App {
    /// Se llama cuando la plataforma esta lista. En escritorio, una vez al
    /// arrancar. Es el sitio correcto para crear la ventana.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // En movil `resumed` puede llamarse varias veces; si ya hay ventana,
        // no hacemos nada.
        if self.window.is_some() {
            return;
        }

        let attrs = window::default_window_attributes();
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[engine] no se pudo crear la ventana: {e}");
                event_loop.exit();
                return;
            }
        };

        // Cargamos el mundo de disco si existe (semilla + chunks editados + pos).
        let path = world_path();
        let (seed, restored, header, player_pos) = match crate::world::load_and_migrate(&path) {
            Ok(save) => {
                let restored: Vec<_> = save
                    .chunks
                    .iter()
                    .map(|(pos, rec)| (*pos, rec.clone()))
                    .collect();
                println!(
                    "[world] mundo cargado: semilla {} | formato v{} | {} chunks | jugador en {:?}",
                    save.header.seed,
                    save.header.format_version,
                    save.chunks.len(),
                    save.player_pos
                );
                (save.header.seed, restored, save.header, save.player_pos)
            }
            Err(e) => {
                println!("[world] sin mundo previo ({e}); se crea uno nuevo (semilla 13371)");
                let seed = 13_371;
                (
                    seed,
                    Vec::new(),
                    crate::world::WorldHeader::new(seed, now_unix()),
                    crate::world::save::DEFAULT_PLAYER_POS,
                )
            }
        };

        match Renderer::new(window.clone(), seed, restored) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[engine] no se pudo iniciar el renderer: {e}");
                event_loop.exit();
                return;
            }
        }
        self.seed = seed;
        self.world_header = header;
        // Barra rapida por defecto.
        self.hotbar = ITEMS;

        // Camara FPS: donde la dejo el jugador (guardado completo), o el spawn
        // por defecto. La fisica la posara sobre el terreno antes del primer frame.
        let mut camera = Camera::new(Vec3::new(player_pos[0], player_pos[1], player_pos[2]));
        camera.pitch_deg = -12.0;
        let size = window.inner_size();
        camera.update_projection(size.width as f32 / size.height.max(1) as f32);
        camera.update_view();
        // Coloca la camara sobre el primer bloque solido bajo ella.
        {
            let world = self.renderer.as_ref().unwrap();
            let is_solid = |p: Vec3| world.is_solid_at(p);
            self.player.settle(&mut camera, is_solid);
        }
        println!("[engine] jugador posado en y={:.2}", camera.position.y);
        self.camera = Some(camera);

        // Modo demo (SOLARIA_DEMO=1): escena fija para las capturas. La camara
        // queda congelada (ver `Self::demo`), asi la vista no se mueve antes de
        // la foto. El montaje vive en `engine::demo`.
        self.demo = demo::is_active();
        if self.demo {
            self.day_cycle = DayCycle::new(demo::time_of_day());
            if let (Some(renderer), Some(camera)) = (self.renderer.as_mut(), self.camera.as_mut()) {
                if demo::ocean_active() {
                    demo::build_ocean_overview(renderer, camera);
                } else if demo::biomes_active() {
                    demo::build_overview(camera);
                } else if demo::collide_active() {
                    demo::build_collision(renderer, camera);
                } else if demo::craft_active() {
                    let (table, grid, result) = demo::build_crafting(renderer, camera);
                    self.crafting_open = true;
                    self.craft_grid = grid;
                    self.craft_result = result;
                    println!("[engine] demo: mesa en {table:?}, resultado={result:?}");
                } else {
                    let torch = demo::build(renderer, camera);
                    println!("[engine] demo: escena lista (antorcha en {torch:?})");
                }
            }
        }

        self.last_frame = Some(Instant::now());
        self.window = Some(window);

        println!("[engine] click = capturar raton | WASD = andar | Espacio = saltar");
        println!("[engine] 1-9/rueda = ranura | E = inventario | F = volar | Escape = salir");
    }

    /// Eventos de la ventana (foco, teclado, botones, resize...).
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("[engine] cerrando");
                self.save_world();
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Guardamos el estado (necesario para el movimiento continuo).
                    self.input.on_key(code, event.state);

                    match code {
                        // Escape: cierra mesa; si no, cierra el inventario; si no,
                        // libera el raton; si ya esta libre, sale.
                        KeyCode::Escape if event.state == ElementState::Pressed => {
                            if self.crafting_open {
                                self.close_crafting();
                            } else if self.inventory_open {
                                self.inventory_open = false;
                            } else if self.mouse_locked {
                                self.unlock_mouse();
                            } else {
                                self.save_world();
                                event_loop.exit();
                            }
                        }
                        // E: cierra la mesa si esta abierta; si no, abre/cierra el
                        // inventario (libera el raton al abrir).
                        KeyCode::KeyE if event.state == ElementState::Pressed => {
                            if self.crafting_open {
                                self.close_crafting();
                                println!("[crafteo] mesa cerrada");
                                return;
                            }
                            self.inventory_open = !self.inventory_open;
                            if self.inventory_open {
                                self.unlock_mouse();
                            }
                            println!(
                                "[engine] inventario {}",
                                if self.inventory_open {
                                    "abierto"
                                } else {
                                    "cerrado"
                                }
                            );
                        }
                        // F: alterna modo vuelo.
                        KeyCode::KeyF if event.state == ElementState::Pressed => {
                            self.flying = !self.flying;
                            println!(
                                "[engine] modo vuelo: {}",
                                if self.flying { "ON" } else { "OFF" }
                            );
                        }
                        // 1..9: selecciona la ranura de la hotbar.
                        _ if event.state == ElementState::Pressed => {
                            if let Some(slot) = digit_slot(code) {
                                self.hotbar_sel = slot;
                                println!("[engine] ranura {} ({:?})", slot + 1, self.hotbar[slot]);
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Botones del raton.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => match button {
                // Click izquierdo: con mesa abierta va a la mesa; en el
                // inventario elige bloque; capturado, rompe; si no, captura.
                MouseButton::Left => {
                    if self.crafting_open {
                        self.crafting_click();
                    } else if self.inventory_open {
                        self.inventory_click();
                    } else if self.mouse_locked {
                        self.break_block();
                    } else {
                        self.lock_mouse();
                    }
                }
                // Click derecho: sobre una mesa la abre; si no, coloca (solo
                // con el cursor capturado y sin ventanas abiertas).
                MouseButton::Right
                    if self.mouse_locked && !self.inventory_open && !self.crafting_open =>
                {
                    // ¿Apuntamos a una mesa de crafteo? Se abre en vez de colocar.
                    let aimed_table = self
                        .selection
                        .zip(self.renderer.as_ref())
                        .map(|(hit, r)| r.block_at(hit.block) == crate::world::Block::CraftingTable)
                        .unwrap_or(false);
                    if aimed_table {
                        self.crafting_open = true;
                        self.inventory_open = false;
                        self.unlock_mouse();
                        println!("[crafteo] mesa abierta (E o Escape para cerrar)");
                    } else {
                        self.place_block();
                    }
                }
                _ => {}
            },

            // Rueda del raton: cambia de ranura en la hotbar.
            WindowEvent::MouseWheel { delta, .. } => {
                if !self.inventory_open {
                    use winit::event::MouseScrollDelta;
                    let step = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y.signum(),
                        MouseScrollDelta::PixelDelta(p) => p.y.signum() as f32,
                    };
                    if step > 0.0 {
                        self.hotbar_sel = (self.hotbar_sel + 8) % 9;
                    } else if step < 0.0 {
                        self.hotbar_sel = (self.hotbar_sel + 1) % 9;
                    }
                }
            }

            // Posicion del cursor (para el inventario).
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            }

            // Si perdemos el foco (alt-tab), liberamos el cursor para no
            // dejarlo atrapado.
            WindowEvent::Focused(false) => {
                if self.mouse_locked {
                    self.unlock_mouse();
                }
            }

            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
                if let Some(camera) = self.camera.as_mut() {
                    camera.update_projection(size.width as f32 / size.height.max(1) as f32);
                }
            }

            WindowEvent::RedrawRequested => {
                // Tiempo real transcurrido desde el frame anterior.
                let now = Instant::now();
                let raw_dt = self
                    .last_frame
                    .map(|last| (now - last).as_secs_f32())
                    .unwrap_or(0.0);
                self.last_frame = Some(now);
                // Limitamos el dt: si el proceso se quedo parado (arrastrando
                // la ventana, breakpoint...) no queremos "teletransportarnos".
                let dt = raw_dt.clamp(0.0, 0.1);

                self.update(dt);
                // En modo demo no resaltamos (queremos ver el modelo limpio).
                if !self.demo {
                    self.update_selection();
                }

                // Dibujamos con la matriz de la camara actual (proyeccion * vista).
                let day_factor = self.day_cycle.day_factor();
                let sky = self.day_cycle.sky_color();
                // Interfaz (hotbar/inventario) construida antes de prestar el
                // renderer para no mezclar prestamos.
                let (win_w, win_h) = self.window_size_f();
                let ui = self.build_ui(win_w, win_h);
                if let (Some(renderer), Some(camera)) =
                    (self.renderer.as_mut(), self.camera.as_ref())
                {
                    renderer.set_environment(day_factor, sky);
                    let view_projection = camera.view_projection();
                    let position = camera.position;
                    renderer.sync_streaming(position);
                    renderer.render(&view_projection, position, &ui);
                }

                // FPS en el titulo: se actualiza cada ~0.5 s con el tiempo real.
                self.fps_frames += 1;
                self.fps_accum += raw_dt;
                if self.fps_accum >= 0.5 {
                    let fps = self.fps_frames as f32 / self.fps_accum;
                    if let Some(window) = self.window.as_ref() {
                        window.set_title(&format!("{}  |  {fps:.0} fps", window::TITLE));
                    }
                    self.fps_frames = 0;
                    self.fps_accum = 0.0;
                }
            }

            _ => {}
        }
    }

    /// Eventos crudos de dispositivos. Aqui llega el movimiento del raton SIN
    /// las limitaciones del borde de la pantalla, que es justo lo que necesita
    /// una camara FPS capturada.
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.mouse_locked
        {
            self.input.on_mouse_motion(delta.0, delta.1);
        }
    }

    /// Se llama cuando no quedan eventos pendientes. Aprovechamos para pedir
    /// otro repintado: asi tenemos refresh continuo (necesario para que la
    /// camara se mueva de forma fluida).
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Ultimo callback antes de cerrar. Guardamos por si no se paso por
    /// `CloseRequested`/Escape (cierre desde el sistema).
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_world();
    }
}
