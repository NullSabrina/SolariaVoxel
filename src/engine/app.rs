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
use crate::render::{Renderer, SkyBasis};
use crate::scene::{Camera, DayCycle, SkyParams, SkyState};
use crate::world::registry;

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
    /// Posicion **logica** del jugador: la unica fuente de verdad de la fisica
    /// (a timestep fijo). La camara guarda la posicion de **render** interpolada.
    player_pos: Vec3,
    /// Posicion logica al **inicio** del frame, para interpolar el render entre
    /// los dos ultimos pasos de fisica (evita el micro-tiron a alto FPS).
    prev_player_pos: Vec3,
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
    /// Hay un guardado en segundo plano pedido y sin confirmar.
    save_requested: bool,
    /// El guardado final ya se hizo (evita el doble cierre CloseRequested+exiting).
    save_finalized: bool,
    /// Hilo de guardado en segundo plano.
    save_worker: Option<super::save_worker::SaveWorker>,
    /// Acumulador para el autoguardado periodico.
    autosave_timer: f32,
    /// Marca de tiempo del frame anterior, para calcular el `dt`.
    last_frame: Option<Instant>,
    /// Acumuladores para mostrar los FPS en el titulo de la ventana.
    fps_frames: u32,
    fps_accum: f32,
    /// Hora del mundo y como afecta a la luz y al cielo.
    day_cycle: DayCycle,
    /// Parametros artisticos del cielo (paleta, tilt, bruma...).
    sky_params: SkyParams,
    /// Acumulador para el tick de agua (10 Hz), separado de la fisica y el render.
    water_timer: f32,
    /// Acumulador del **timestep fijo** de la fisica del jugador (segundos).
    accumulator: f32,
    /// Presupuesto del tick de fluidos (celdas y ms). Configurable por entorno.
    fluid_budget: crate::world::FluidBudget,
    /// Modo demo (`SOLARIA_DEMO`): congela la camara y elige la escena de la
    /// captura. La fisica y el resaltado se desactivan para que la vista no se
    /// desplace antes de la foto.
    demo: bool,
    /// Overlay de diagnostico (**F3** o `SOLARIA_STATS=1`): dibuja en pantalla
    /// dos columnas de texto (jugador/mundo a la izquierda, render/sistema a la
    /// derecha) con la fuente bitmap, al estilo de la pantalla de depuracion de
    /// Minecraft. Ademas traza una linea `[stats]` por consola.
    show_stats: bool,
    /// Duracion del ultimo `update` (ms).
    update_ms: f32,
    /// Duracion del ultimo `render` (ms).
    render_ms: f32,
    /// Ultimos FPS calculados (el titulo los refresca cada ~0.5 s; el overlay F3
    /// los reutiliza).
    last_fps: f32,
    /// Animacion de la **mano**: `swing` en `0..1` (golpe al romper/colocar) y
    /// `bob` = fase de balanceo al andar.
    swing: f32,
    bob: f32,
    /// Camara en **tercera persona** (F5): se ve el personaje.
    third_person: bool,
}

/// Distancia y altura de la camara en tercera persona.
const TP_DISTANCE: f32 = 3.6;
const TP_HEIGHT: f32 = 0.35;

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

/// Cuantas ranuras tiene la barra rapida (teclas `1`-`9`).
const HOTBAR_SLOTS: usize = 9;

/// Escala de la interfaz (pixels de mundo -> pixels de pantalla).
const UI_SCALE: f32 = 2.0;

/// Periodo del tick de **agua**, en segundos (10 Hz). Va aparte de la fisica y
/// del render. Es mas rapido que Minecraft (que usa 5 ticks = 0.25 s por paso)
/// pero sin llegar a verse nervioso: cada paso mueve el agua un bloque.
const WATER_PERIOD: f32 = 0.1;

/// Periodo del **autoguardado** en segundo plano (segundos). El mundo se guarda
/// sin bloquear el render.
const AUTOSAVE_PERIOD: f32 = 300.0;

/// Paso fijo de la simulacion del jugador (segundos). 120 Hz da margen a
/// velocidades altas; el render puede ir a otro ritmo.
const FIXED_DT: f32 = 1.0 / 120.0;

/// Maximo de pasos fijos por frame (evita la "espiral de la muerte": cada paso
/// cuesta CPU y un frame lento no debe generar infinitos pasos).
const MAX_FIXED_STEPS: u32 = 8;

/// Tope del acumulador de tiempo de simulacion (segundos).
const MAX_ACCUMULATOR: f32 = 0.25;

/// Avanza el acumulador de simulacion y devuelve cuantos **pasos fijos** hay que
/// dar este frame. Acota a `MAX_FIXED_STEPS` (descarta la deuda si el frame fue
/// demasiado largo). Es puro: se testea sin ventana ni GPU.
fn fixed_steps(accumulator: &mut f32, frame_dt: f32) -> u32 {
    *accumulator = (*accumulator + frame_dt).min(MAX_ACCUMULATOR);
    let mut steps = 0;
    while *accumulator >= FIXED_DT && steps < MAX_FIXED_STEPS {
        *accumulator -= FIXED_DT;
        steps += 1;
    }
    if steps == MAX_FIXED_STEPS {
        *accumulator = 0.0;
    }
    steps
}

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

    /// Un frame: avanza el tiempo del mundo (dia, agua a 10 Hz, autosave), aplica
    /// el giro de camara (por frame) y corre la fisica del jugador a **timestep
    /// fijo** (acumulador).
    fn update(&mut self, frame_dt: f32) {
        // El tiempo del mundo avanza siempre (salvo en demo, que lo congela).
        if !self.demo {
            self.day_cycle.advance(frame_dt);

            // Tick de agua a 10 Hz, independiente del framerate.
            self.water_timer += frame_dt;
            if self.water_timer >= WATER_PERIOD {
                self.water_timer -= WATER_PERIOD;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.tick_water(self.fluid_budget);
                }
            }

            // Autoguardado en segundo plano (no bloquea el render).
            self.autosave_timer += frame_dt;
            if self.autosave_timer >= AUTOSAVE_PERIOD {
                self.autosave_timer = 0.0;
                self.save_world();
            }
            // Recoge los guardados que hayan terminado.
            self.poll_save();

            // Animacion de la mano: balanceo al andar y decaimiento del golpe.
            let moving = self.input.forward_axis() != 0.0 || self.input.right_axis() != 0.0;
            if moving && !self.flying {
                self.bob += frame_dt * 7.0;
            }
            self.swing = (self.swing - frame_dt / 0.28).max(0.0);
        }

        // El giro es **por frame**: el delta del raton es de este frame, no de un
        // paso de simulacion. Se consume una sola vez.
        let (dx, dy) = self.input.take_mouse_delta();
        if self.mouse_locked && (dx != 0.0 || dy != 0.0) {
            if let Some(camera) = self.camera.as_mut() {
                camera.add_look(dx, dy);
            }
        }

        // En demo la camara queda fija: no aplicamos la fisica, para que la vista
        // de la captura no se desplace antes de la foto.
        if self.demo {
            return;
        }

        // Fisica a **timestep fijo**: se acumula el tiempo real y se ejecutan
        // pasos de duracion constante. Desacopla el movimiento del framerate
        // (determinismo y base para entidades/multijugador).
        self.prev_player_pos = self.player_pos;
        let steps = fixed_steps(&mut self.accumulator, frame_dt);
        for _ in 0..steps {
            self.simulate_player(FIXED_DT);
        }
        // Posicion de **render**: interpolada entre los dos ultimos pasos de
        // fisica con la fraccion de tiempo acumulada (`alpha`). Con la fisica a
        // 120 Hz y el render a mas, sin esto la posicion se quedaria un paso
        // atras (micro-tiron al andar).
        let alpha = (self.accumulator / FIXED_DT).clamp(0.0, 1.0);
        if let Some(camera) = self.camera.as_mut() {
            camera.position = self.prev_player_pos.lerp(self.player_pos, alpha);
            camera.update_view();
        }
    }

    /// Un paso de fisica del jugador de duracion `dt` (fija).
    fn simulate_player(&mut self, dt: f32) {
        let forward = self.input.forward_axis();
        let right = self.input.right_axis();
        let jump = self.input.jump_axis();
        let jump_held = self.input.jump_held();
        let flying = self.flying;

        // La consulta de solido mira el mundo (incluye `Unloaded` = muro, para no
        // caer al vacio mientras llega la generacion).
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let Some(camera) = self.camera.as_mut() else {
            return;
        };
        // La fisica parte de la posicion **logica** (no de la interpolada).
        camera.position = self.player_pos;
        let world = renderer;
        let is_solid = move |point: Vec3| -> bool { world.is_solid_at(point) };

        // Movimiento horizontal CON colision (no atravesamos paredes).
        if forward != 0.0 || right != 0.0 {
            let mut player = self.player;
            player.move_horizontal(camera, is_solid, forward, right, dt);
            self.player = player;
        }

        // En modo vuelo, Espacio/Shift suben/bajan; en modo normal Space salta.
        let shift =
            self.input.is_pressed(KeyCode::ShiftLeft) || self.input.is_pressed(KeyCode::ShiftRight);
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
        // Recoge la posicion logica resultante del paso.
        self.player_pos = camera.position;
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
        self.swing = 1.0;
        self.update_selection();
    }

    /// Pide un guardado en **segundo plano** (no bloquea el render). Sirve tanto
    /// para el autoguardado como para el cierre.
    fn save_world(&mut self) {
        if self.save_requested {
            return;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mut save = crate::world::WorldSave::new(self.seed, self.world_header.created_at);
        // Conservamos las versiones del header original.
        save.header = self.world_header.clone();
        // Volcamos todas las columnas que el jugador ha modificado.
        for (pos, record) in renderer.snapshot_modified() {
            save.set_chunk(pos, record);
        }
        // Guardado completo: la posicion **logica** del jugador (no la de render).
        let p = self.player_pos;
        save.player_pos = [p.x, p.y, p.z];
        let chunks = save.chunks.len();
        let requested = match self.save_worker.as_ref() {
            Some(worker) => worker.request(save, world_path()),
            None => false,
        };
        if requested {
            self.save_requested = true;
            println!("[world] guardado en segundo plano ({chunks} chunks editados)");
        } else {
            eprintln!("[world] no se pudo encolar el guardado");
        }
    }

    /// Recoge los resultados de guardados terminados.
    fn poll_save(&mut self) {
        let outcomes: Vec<super::save_worker::SaveOutcome> = match self.save_worker.as_ref() {
            Some(worker) => std::iter::from_fn(|| worker.try_recv()).collect(),
            None => Vec::new(),
        };
        for outcome in outcomes {
            self.save_requested = false;
            match outcome.result {
                Ok(()) => {
                    self.world_saved = true;
                    println!(
                        "[world] guardado: {} chunks, {} bytes",
                        outcome.chunks, outcome.bytes
                    );
                }
                // Si fallo, queda reintentable (otro autosave o el cierre).
                Err(e) => eprintln!("[world] fallo el guardado (se reintentara): {e}"),
            }
        }
    }

    /// Cierre: pide el guardado, **espera** al hilo y recoge el resultado (para
    /// no perder el ultimo estado). Idempotente (`CloseRequested` + `exiting`).
    fn finalize_save(&mut self) {
        if self.save_finalized {
            return;
        }
        self.save_finalized = true;
        self.save_world();
        if let Some(worker) = self.save_worker.as_mut() {
            worker.join();
        }
        self.poll_save();
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
        self.swing = 1.0;
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

    /// Texto de la barra de titulo (solo lo esencial: el detalle va en el overlay
    /// **F3** en pantalla).
    fn title_line(&self, fps: f32, stats: Option<crate::render::FrameStats>) -> String {
        match stats {
            Some(s) => format!(
                "{}  |  {fps:.0} fps  |  {} dc  |  {} tri",
                window::TITLE,
                s.draw_calls,
                s.triangles
            ),
            None => format!("{}  |  {fps:.0} fps", window::TITLE),
        }
    }

    /// Celdas (rectangulos) del inventario: una rejilla que contiene **todos**
    /// los bloques de `BlockRegistry::items()` (8 columnas), centrada.
    fn inventory_cells(&self, win_w: f32, win_h: f32) -> Vec<[f32; 4]> {
        use crate::render::gui;
        let slot = gui::SLOT as f32 * UI_SCALE;
        let gap = 6.0;
        let cols = 8usize;
        let rows = registry::BlockRegistry::items().len().div_ceil(cols).max(1);
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

        // Con una ventana abierta se atenua el mundo detras (legibilidad).
        if self.inventory_open || self.crafting_open {
            quads.push(UiQuad {
                rect: [0.0, 0.0, win_w, win_h],
                uv: region_uv(gui::DIM),
                layer: -1,
            });
        }

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

        // Inventario: rejilla con TODOS los bloques disponibles (ICONOS).
        if self.inventory_open {
            for (cell, item) in self
                .inventory_cells(win_w, win_h)
                .iter()
                .zip(registry::BlockRegistry::items().iter())
            {
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

        // Overlay de diagnostico **F3**: dos columnas con fondo oscuro.
        if self.show_stats {
            quads.extend(self.f3_overlay(win_w));
        }
        quads
    }

    /// Quads del overlay **F3** (estilo pantalla de depuracion de Minecraft):
    /// columna izquierda con jugador/mundo, derecha con render/sistema, cada una
    /// sobre un fondo oscuro translucido.
    fn f3_overlay(&self, win_w: f32) -> Vec<crate::render::UiQuad> {
        use crate::render::{UiQuad, font, gui, region_uv};
        use crate::world::memory::mib;
        let scale = UI_SCALE;
        let advance = (font::GLYPH_H as f32 + 2.0) * scale;
        let pad = 5.0;
        // Ancho minimo reservado a la columna derecha (margen a la derecha).
        const RIGHT_COL_W: f32 = 300.0;

        let p = self.player_pos;
        let feet_y = (p.y - crate::player::EYE_HEIGHT).floor() as i32;
        let (bx, bz) = (p.x.floor() as i32, p.z.floor() as i32);
        let (cx, cz) = (bx.div_euclid(16), bz.div_euclid(16));
        let (rx, rz) = (cx.div_euclid(32), cz.div_euclid(32));
        let (yaw, pitch) = self
            .camera
            .as_ref()
            .map_or((0.0, 0.0), |c| (c.yaw_deg, c.pitch_deg));
        let facing = if yaw.abs() < 45.0 {
            "NORTH"
        } else if yaw.abs() > 135.0 {
            "SOUTH"
        } else if yaw > 0.0 {
            "EAST"
        } else {
            "WEST"
        };
        let (sky, blk) = self.renderer.as_ref().map_or((0, 0), |r| {
            (
                r.sky_light_at([bx, feet_y, bz]),
                r.block_light_at([bx, feet_y, bz]),
            )
        });
        let biome = self.renderer.as_ref().map_or("?", |r| {
            crate::world::worldgen::biomes::definition(r.biome_at(bx, bz)).name
        });
        let tod = self.day_cycle.time_of_day * 24.0;
        let sky_state = SkyState::at(&self.day_cycle, &self.sky_params);
        let stats = self.renderer.as_ref().map(|r| r.frame_stats());
        let memory = self.renderer.as_ref().map(|r| r.world_memory());
        let gpu = self.renderer.as_ref().map_or(0, |r| r.gpu_mesh_bytes());
        let view = self
            .renderer
            .as_ref()
            .map_or(crate::world::ViewSettings::default(), |r| r.view());
        let queued = self
            .renderer
            .as_ref()
            .map_or(0, |r| r.pending_mesh_sections());

        let left = [
            format!("SOLARIA VOXEL {}", env!("CARGO_PKG_VERSION")),
            format!(
                "{:.0} FPS  UP {:.1}MS  RND {:.1}MS",
                self.last_fps, self.update_ms, self.render_ms
            ),
            format!("XYZ: {:.2} / {:.2} / {:.2}", p.x, p.y, p.z),
            format!("BLOCK: {bx} {feet_y} {bz}"),
            format!("CHUNK: {cx} {cz} IN {rx} {rz}"),
            format!("FACING: {facing} ({yaw:.1} / {pitch:.1})"),
            format!("BIOME: {biome}"),
            format!("LIGHT: {} ({sky} SKY, {blk} BLOCK)", sky.max(blk)),
            format!("TIME: {:02}:{:02}", tod as u32, (tod * 60.0) as u32 % 60),
            format!(
                "SKY: EL {:.0} MOON {}",
                sky_state.sun_elevation_deg, sky_state.moon_phase
            ),
            format!("SEED: {}", self.seed),
        ];
        let mut right: Vec<String> = Vec::new();
        if let Some(s) = stats {
            right.push(format!("DRAW CALLS: {}", s.draw_calls));
            right.push(format!("TRIANGLES: {}", s.triangles));
            right.push(format!(
                "CULLED: F{} D{}",
                s.culled_frustum, s.culled_distance
            ));
        }
        if let Some(m) = memory {
            right.push(format!("CHUNKS: {}", m.columns));
            right.push(format!("MEMORY: {:.1} MB", mib(m.total_bytes())));
        }
        right.push(format!("GPU MESH: {:.1} MB", mib(gpu as usize)));
        right.push(format!(
            "VIEW: R{} S{} {}",
            view.render_radius,
            view.simulation_radius,
            view.fog.name()
        ));
        right.push(format!("FLUID QUEUE: {queued}"));
        right.push(format!(
            "SAVE: {}",
            if self.save_requested { "PENDING" } else { "OK" }
        ));

        let mut quads: Vec<UiQuad> = Vec::new();
        // Izquierda: ancho segun el texto.
        let lw = left
            .iter()
            .map(|l| font::text_width(l, scale))
            .fold(0.0, f32::max);
        let lh = left.len() as f32 * advance;
        quads.push(UiQuad {
            rect: [pad - 3.0, pad - 3.0, lw + 6.0, lh + 5.0],
            uv: region_uv(gui::DIM),
            layer: -1,
        });
        for (i, line) in left.iter().enumerate() {
            quads.extend(font::text_quads(line, pad, pad + i as f32 * advance, scale));
        }
        // Derecha: **ancho fijo** con margen holgado a la derecha (no depende del
        // tamano exacto de la ventana, que puede diferir del de la superficie).
        let rw = RIGHT_COL_W.max(
            right
                .iter()
                .map(|l| font::text_width(l, scale))
                .fold(0.0, f32::max),
        );
        let rxx = (win_w - 20.0 - rw).max(win_w * 0.5);
        let rh = right.len() as f32 * advance;
        quads.push(UiQuad {
            rect: [rxx - 3.0, pad - 3.0, rw + 6.0, rh + 5.0],
            uv: region_uv(gui::DIM),
            layer: -1,
        });
        for (i, line) in right.iter().enumerate() {
            quads.extend(font::text_quads(line, rxx, pad + i as f32 * advance, scale));
        }
        quads
    }

    /// Un click en el inventario: elige el bloque de la celda pulsada.
    fn inventory_click(&mut self) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        for (cell, item) in self
            .inventory_cells(win_w, win_h)
            .iter()
            .zip(registry::BlockRegistry::items().iter())
        {
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
        (0..HOTBAR_SLOTS)
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
        for (cell, item) in self
            .inventory_cells(win_w, win_h)
            .iter()
            .zip(registry::BlockRegistry::items().iter())
        {
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
        // Saneamiento: una posicion guardada fuera del mundo (p.ej. de un
        // arranque anterior con columnas sin cargar que poso al jugador en el
        // techo) no debe dejarlo cayendo. Si no es valida, se usa el spawn.
        let player_pos =
            if player_pos[1] < 0.0 || player_pos[1] >= crate::world::WORLD_HEIGHT as f32 {
                crate::world::save::DEFAULT_PLAYER_POS
            } else {
                player_pos
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
        // Hora inicial y velocidad del dia por entorno (fuera del modo demo).
        let start_time = std::env::var("SOLARIA_TIME")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.35);
        self.day_cycle = DayCycle::new(start_time);
        if let Some(speed) = std::env::var("SOLARIA_DAY_SPEED")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            && speed > 0.0
        {
            let base = self.day_cycle.day_length;
            self.day_cycle.day_length = base / speed;
        }
        // Presupuesto de fluidos configurable por entorno.
        self.fluid_budget = crate::world::FluidBudget::from_env();
        println!(
            "[world] presupuesto de fluidos: {} celdas / {:.1} ms por tick",
            self.fluid_budget.cells, self.fluid_budget.ms
        );
        // Barra rapida por defecto.
        // Barra rapida por defecto: los primeros `HOTBAR_SLOTS` items.
        self.hotbar = std::array::from_fn(|i| registry::BlockRegistry::items()[i]);

        // Carga **sincrona** del area inicial antes de posar al jugador (o
        // montar la demo): con streaming async el suelo aun no estaria cargado y
        // el jugador caeria o la demo saldria vacia.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.warm_streaming(Vec3::new(player_pos[0], player_pos[1], player_pos[2]));
        }

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
            let is_solid = |p: Vec3| world.is_solid_loaded_at(p);
            self.player.settle(&mut camera, is_solid);
        }
        println!("[engine] jugador posado en y={:.2}", camera.position.y);
        // Posicion logica inicial (la camara arranca en ella; el render la
        // interpola cada frame).
        self.player_pos = camera.position;
        self.prev_player_pos = camera.position;
        self.camera = Some(camera);

        // Informe de memoria por categorias (FASE 10): medir antes de optimizar.
        if let Some(renderer) = self.renderer.as_ref() {
            use crate::world::memory::mib;
            let m = renderer.world_memory();
            println!(
                "[mem] mundo: {} col | bloques {:.1} MB | cielo {:.1} MB | bloque {:.1} MB | fluido {:.1} MB | struct {:.1} MB | editados {} ({:.1} MB) | cola agua {} celdas",
                m.columns,
                mib(m.blocks_bytes),
                mib(m.skylight_bytes),
                mib(m.blocklight_bytes),
                mib(m.fluid_bytes),
                mib(m.struct_overhead_bytes),
                m.modified_chunks,
                mib(m.modified_bytes),
                m.water_queue_cells,
            );
            println!(
                "[mem] render: {} col con malla | GPU {:.1} MB | {} secciones pendientes",
                renderer.mesh_columns(),
                mib(renderer.gpu_mesh_bytes() as usize),
                renderer.pending_mesh_sections(),
            );
        }

        // Modo demo (SOLARIA_DEMO=1): escena fija para las capturas. La camara
        // queda congelada (ver `Self::demo`), asi la vista no se mueve antes de
        // la foto. El montaje vive en `engine::demo`.
        self.show_stats = std::env::var("SOLARIA_STATS").is_ok();
        self.third_person = std::env::var("SOLARIA_THIRD").is_ok();
        self.demo = demo::is_active();
        if self.demo {
            self.day_cycle = DayCycle::new(demo::time_of_day());
            if let (Some(renderer), Some(camera)) = (self.renderer.as_mut(), self.camera.as_mut()) {
                if demo::ocean_active() {
                    demo::build_ocean_overview(renderer, camera);
                } else if demo::river_active() {
                    demo::build_river(renderer, camera);
                } else if demo::biomes_active() {
                    demo::build_overview(camera);
                } else if demo::collide_active() {
                    demo::build_collision(renderer, camera);
                } else if demo::cave_active() {
                    demo::build_cave(renderer, camera);
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
                demo::apply_look(camera);
            }
        }
        if let Some(day) = demo::day_count() {
            self.day_cycle.day_count = day;
        }

        self.last_frame = Some(Instant::now());
        // Hilo de guardado en segundo plano.
        self.save_worker = Some(super::save_worker::SaveWorker::spawn());
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
                self.finalize_save();
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
                                self.finalize_save();
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
                        // F3: overlay de diagnostico (metricas de frame).
                        KeyCode::F3 if event.state == ElementState::Pressed => {
                            self.show_stats = !self.show_stats;
                            println!(
                                "[engine] overlay F3: {}",
                                if self.show_stats { "ON" } else { "OFF" }
                            );
                        }
                        // F5: primera/tercera persona (ver el personaje).
                        KeyCode::F5 if event.state == ElementState::Pressed => {
                            self.third_person = !self.third_person;
                            println!(
                                "[engine] camara: {}",
                                if self.third_person {
                                    "tercera persona"
                                } else {
                                    "primera persona"
                                }
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

                let t_update = Instant::now();
                self.update(dt);
                // En modo demo no resaltamos (queremos ver el modelo limpio).
                if !self.demo {
                    self.update_selection();
                }

                // Cielo del frame: unica fuente de verdad, resuelta en CPU.
                let sky_state = SkyState::at(&self.day_cycle, &self.sky_params);
                // Interfaz (hotbar/inventario) construida antes de prestar el
                // renderer para no mezclar prestamos.
                // El layout de la interfaz usa el tamano de la **superficie**
                // (el mismo que la conversion a NDC), no el de la ventana.
                let (win_w, win_h) = self
                    .renderer
                    .as_ref()
                    .map(|r| {
                        let (w, h) = r.surface_size();
                        (w as f32, h as f32)
                    })
                    .unwrap_or_else(|| self.window_size_f());
                let ui = self.build_ui(win_w, win_h);
                self.update_ms = t_update.elapsed().as_secs_f32() * 1000.0;
                let t_render = Instant::now();
                if let (Some(renderer), Some(camera)) =
                    (self.renderer.as_mut(), self.camera.as_mut())
                {
                    renderer.set_sky(&sky_state);
                    let player_eye = camera.position;
                    let third = self.third_person && !self.demo;
                    // Tercera persona: la camara se separa del jugador (el
                    // personaje se dibuja en la posicion del jugador).
                    if third {
                        let f = camera.forward();
                        camera.position = Vec3::new(
                            player_eye.x - f.x * TP_DISTANCE,
                            player_eye.y - f.y * TP_DISTANCE + TP_HEIGHT,
                            player_eye.z - f.z * TP_DISTANCE,
                        );
                        camera.update_view();
                    }
                    let view_projection = camera.view_projection();
                    let position = camera.position;
                    // Base de la camara para que el shader del cielo reconstruya
                    // el rayo de vista sin invertir la matriz.
                    let fwd = camera.forward();
                    let right = fwd.cross(Vec3::Y).normalize();
                    let sky_basis = SkyBasis {
                        forward: fwd,
                        right,
                        up: right.cross(fwd).normalize(),
                        tan_half_fov_y: (camera.fov_y_deg.to_radians() * 0.5).tan(),
                        aspect: win_w / win_h.max(1.0),
                    };
                    let feet = Vec3::new(
                        player_eye.x,
                        player_eye.y - crate::player::EYE_HEIGHT,
                        player_eye.z,
                    );
                    // Mano solo en primera persona (no en demo).
                    let hand = (!self.demo && !third).then(|| crate::render::HandView {
                        projection: camera.projection(),
                        swing: self.swing,
                        bob: self.bob,
                    });
                    // Personaje solo en tercera persona.
                    let character = third.then(|| crate::render::CharacterView {
                        view_projection,
                        world: crate::scene::player::character_matrix(feet, camera.yaw_deg),
                        walk: self.bob,
                    });
                    renderer.set_hand_item(self.hotbar[self.hotbar_sel]);
                    renderer.sync_streaming(player_eye);
                    renderer.render(&view_projection, position, &sky_basis, &ui, hand, character);
                    // Restaura la camara del jugador para la fisica del proximo frame.
                    camera.position = player_eye;
                    camera.update_view();
                }
                self.render_ms = t_render.elapsed().as_secs_f32() * 1000.0;

                // FPS en el titulo: se actualiza cada ~0.5 s con el tiempo real.
                self.fps_frames += 1;
                self.fps_accum += raw_dt;
                if self.fps_accum >= 0.5 {
                    let fps = self.fps_frames as f32 / self.fps_accum;
                    self.last_fps = fps;
                    let stats = self.renderer.as_ref().map(|r| r.frame_stats());
                    let queued = self
                        .renderer
                        .as_ref()
                        .map_or(0, |r| r.pending_mesh_sections());
                    let title = self.title_line(fps, stats);
                    if let Some(window) = self.window.as_ref() {
                        window.set_title(&title);
                    }
                    if self.show_stats
                        && let Some(s) = stats
                    {
                        println!(
                            "[stats] up={:.1}ms rnd={:.1}ms dc={} tri={} dib={} cull_f={} cull_d={} col={} cola={} save={}",
                            self.update_ms,
                            self.render_ms,
                            s.draw_calls,
                            s.triangles,
                            s.sections_drawn,
                            s.culled_frustum,
                            s.culled_distance,
                            s.columns,
                            queued,
                            if self.save_requested { "pend" } else { "ok" },
                        );
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
        self.finalize_save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_timestep_fijo_da_pasos_constantes() {
        let mut acc = 0.0;
        // Un frame de 1/60 = 2 pasos de 1/120 y el acumulador vuelve a ~0.
        assert_eq!(fixed_steps(&mut acc, 1.0 / 60.0), 2);
        assert!(acc.abs() < 1e-5, "acc={acc}");
        // Un frame vacio no da pasos.
        assert_eq!(fixed_steps(&mut acc, 0.0), 0);
        // Dos medios pasos acumulan un paso.
        assert_eq!(fixed_steps(&mut acc, FIXED_DT / 2.0), 0);
        assert_eq!(fixed_steps(&mut acc, FIXED_DT / 2.0), 1);
    }

    #[test]
    fn el_timestep_fijo_acota_los_pasos_y_descarta_la_deuda() {
        let mut acc = 0.0;
        // Un frame gigante no dispara infinitos pasos; ademas la deuda se anula.
        assert_eq!(fixed_steps(&mut acc, 5.0), MAX_FIXED_STEPS);
        assert_eq!(acc, 0.0, "la deuda acumulada deberia descartarse");
    }

    #[test]
    fn la_mesa_abierta_atenua_el_fondo_y_dibuja_la_rejilla() {
        let mut app = App::default();
        let base = app.build_ui(1280.0, 720.0).len();
        app.crafting_open = true;
        app.craft_grid = [
            None,
            None,
            None,
            None,
            Some(crate::world::Block::Wood),
            None,
            None,
            None,
            None,
        ];
        app.refresh_craft_result();
        let quads = app.build_ui(1280.0, 720.0);
        assert!(quads.len() > base, "la mesa debe anadir quads");
        assert_eq!(
            quads[0].rect,
            [0.0, 0.0, 1280.0, 720.0],
            "el primer quad es el dim a pantalla completa"
        );
        assert_eq!(app.craft_result, Some(crate::world::Block::Planks));
    }
}
