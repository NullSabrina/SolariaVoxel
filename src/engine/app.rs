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
use crate::ui::{Effect, InputMode, Mode, Overlay, Screen, ScreenStack};
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
    /// Maquina de estados del **modo de input** (Esc/cursor). Logica pura en
    /// `ui::input_mode`; aqui solo se aplican sus efectos.
    input_mode: InputMode,
    /// Captura de cursor **pedida** pero aun no aceptada por el SO (se reintenta
    /// al enfocar la ventana o al hacer click). El estado logico es `mouse_locked`.
    want_capture: bool,
    /// Modo vuelo (F): sin gravedad, para explorar.
    flying: bool,
    /// Bloque apuntado por la camara en el ultimo frame (y su cara).
    selection: Option<crate::world::RayHit>,
    /// Inventario y hotbar (stacks, cursor, arrastre). Logica en `ui`.
    inventory: crate::ui::InventoryState,
    /// Foco de teclado en los menus (indice de boton visible).
    menu_focus: usize,
    /// Boton de menu pulsado con el raton (estado "pulsado").
    menu_pressed: Option<usize>,
    /// Rebote de la ranura seleccionada de la hotbar (1.0 -> 0.0 en ~150 ms).
    hotbar_bounce: f32,
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
    /// Idioma de la interfaz (es/en).
    lang: crate::ui::Lang,
    /// Categoria activa del inventario creativo (indice en `CreativeCategory::ALL`).
    inv_category: usize,
    /// Texto de busqueda del inventario creativo.
    inv_search: String,
    /// Desplazamiento vertical del inventario (en filas).
    inv_scroll: i32,
    /// Nombre del bloque sobre la hotbar: `(bloque, segundos restantes)`.
    hotbar_toast: Option<(crate::world::Block, f32)>,
    /// Pila de pantallas (titulo, selector de mundos, jugar, pausa...).
    screens: crate::ui::ScreenStack,
    /// Directorio base de los mundos (`saves/` cuelga de aqui).
    world_base: std::path::PathBuf,
    /// Slug del mundo activo (su carpeta).
    world_slug: String,
    /// Mundos en disco (cache para el selector).
    worlds: Vec<crate::world::WorldEntry>,
    /// Texto del campo "nombre" al crear mundo.
    create_name: String,
    /// Texto del campo "semilla" al crear mundo.
    create_seed: String,
    /// Mundo pendiente de confirmar para eliminar.
    confirm_delete: Option<String>,
    /// Indice seleccionado en el selector de mundos.
    world_sel: usize,
    /// Campo enfocado en "crear mundo" (0 = nombre, 1 = semilla).
    create_focus: usize,
    /// Accion esperando una tecla nueva en la pantalla de Controles.
    rebinding: Option<String>,
    /// Slug del mundo que se esta renombrando (si `Some`, "Crear" renombra).
    rename_target: Option<String>,
    /// Peticion de salida (se atiende tras procesar el evento).
    exit_requested: bool,
    /// Opciones persistentes (video, juego, teclas).
    options: crate::ui::Options,
    /// Intervalo de autoguardado en segundos (de las opciones).
    autosave_period: f32,
}

/// Distancia y altura de la camara en tercera persona.
const TP_DISTANCE: f32 = 3.6;
const TP_HEIGHT: f32 = 0.35;

/// Ruta del `world.vf` del mundo en `base/<slug>/`.
fn active_world_path(base: &std::path::Path, slug: &str) -> std::path::PathBuf {
    crate::world::library::saves_dir(base)
        .join(slug)
        .join(crate::world::library::WORLD_FILE)
}

/// Tipo de generador para mundos **nuevos** (`SOLARIA_GENERATOR=graph`), o legacy.
fn generator_kind_from_env() -> crate::world::GeneratorKind {
    std::env::var("SOLARIA_GENERATOR")
        .map(|s| crate::world::GeneratorKind::from_name(&s))
        .unwrap_or_default()
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

/// Rejilla visible del inventario creativo (columnas x filas).
const INV_COLS: usize = 9;
const INV_ROWS: usize = 5;

/// Escala de la interfaz (pixels de mundo -> pixels de pantalla).
const UI_SCALE: f32 = 2.0;

/// Periodo del tick de **agua**, en segundos (10 Hz). Va aparte de la fisica y
/// del render. Es mas rapido que Minecraft (que usa 5 ticks = 0.25 s por paso)
/// pero sin llegar a verse nervioso: cada paso mueve el agua un bloque.
const WATER_PERIOD: f32 = 0.1;

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

    /// Modo de input derivado del estado real (pantallas y flags). No se guarda
    /// aparte: la maquina de estados se sincroniza con esto antes de decidir.
    fn current_mode(&self) -> Mode {
        if self.inventory_open {
            Mode::Overlay(Overlay::Inventory)
        } else if self.crafting_open {
            Mode::Overlay(Overlay::Crafting)
        } else {
            match self.screens.top() {
                Screen::Playing => Mode::Playing,
                Screen::Pause => Mode::Overlay(Overlay::Pause),
                _ => Mode::Menu,
            }
        }
    }

    /// Bloque de la ranura `i` de la hotbar (aire si esta vacia).
    fn hotbar_block(&self, i: usize) -> crate::world::Block {
        self.inventory.hotbar_block(i)
    }

    /// Bloque de la ranura activa de la hotbar.
    fn hotbar_block_sel(&self) -> crate::world::Block {
        self.inventory.hotbar_block(self.hotbar_sel)
    }

    /// Pone un stack lleno del bloque dado en la ranura activa.
    fn set_hotbar_sel(&mut self, block: crate::world::Block) {
        self.inventory.set(
            crate::ui::SlotRef::hotbar(self.hotbar_sel),
            Some(crate::world::ItemStack::full(block)),
        );
    }

    /// Restaura la hotbar desde el guardado (`(id, cantidad)`).
    fn set_hotbar_from_save(&mut self, saved: &[(u8, u8)]) {
        self.inventory.set_hotbar_from_save(saved);
    }

    /// Intenta capturar el cursor. Si el SO lo rechaza (ventana sin foco) deja
    /// `want_capture` puesto para reintentar; `mouse_locked` refleja lo aceptado.
    fn try_capture(&mut self) {
        if self.demo {
            return;
        }
        self.lock_mouse();
        self.want_capture = !self.mouse_locked;
    }

    /// Aplica los efectos que devuelve la maquina de estados de input (`ui`).
    fn apply_input_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::CaptureCursor => {
                    self.want_capture = true;
                    self.try_capture();
                }
                Effect::ReleaseCursor => {
                    self.want_capture = false;
                    self.unlock_mouse();
                }
                Effect::OpenPause => self.screens.push(Screen::Pause),
                Effect::ClosePause => {
                    self.rebinding = None;
                    self.screens.pop();
                }
                Effect::OpenInventory => self.inventory_open = true,
                Effect::CloseInventory => self.inventory_open = false,
                Effect::CloseCrafting => self.close_crafting(),
                Effect::MenuBack => self.menu_back(),
            }
        }
    }

    /// Un frame: avanza el tiempo del mundo (dia, agua a 10 Hz, autosave), aplica
    /// el giro de camara (por frame) y corre la fisica del jugador a **timestep
    /// fijo** (acumulador).
    fn update(&mut self, frame_dt: f32) {
        // Reloj del inventario (ventana de doble click).
        self.inventory.tick(frame_dt);
        // Rebote de la hotbar: decae con el tiempo de frame (o se apaga con
        // "reducir movimiento").
        if self.options.reduce_motion {
            self.hotbar_bounce = 0.0;
        } else {
            self.hotbar_bounce = (self.hotbar_bounce - frame_dt / 0.15).max(0.0);
        }
        // El tiempo del mundo avanza solo si se esta jugando (no en demo/menus,
        // que lo congelan).
        let playing = self.screens.is_playing();
        if !self.demo && playing {
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
            if self.autosave_timer >= self.autosave_period {
                self.autosave_timer = 0.0;
                self.save_world();
            }

            // Animacion de la mano: balanceo al andar y decaimiento del golpe.
            let moving = self.move_forward_axis() != 0.0 || self.move_right_axis() != 0.0;
            if moving && !self.flying {
                self.bob += frame_dt * 7.0;
            }
            self.swing = (self.swing - frame_dt / 0.28).max(0.0);
        }

        // Recoge los guardados terminados SIEMPRE (tambien si se pidio desde la
        // pausa): si no, `save_requested` se queda atascado y no se vuelve a guardar.
        self.poll_save();

        // Nombre del bloque sobre la hotbar: aparece al cambiar de ranura y se
        // desvanece (a los ~2 s desaparece).
        if let Some((b, t)) = self.hotbar_toast {
            let t = t - frame_dt;
            self.hotbar_toast = (t > 0.0).then_some((b, t));
        }

        // El giro es **por frame**: el delta del raton es de este frame, no de un
        // paso de simulacion. Se consume una sola vez.
        let (dx, dy) = self.input.take_mouse_delta();
        if self.mouse_locked && playing && (dx != 0.0 || dy != 0.0) {
            if let Some(camera) = self.camera.as_mut() {
                camera.add_look(dx, dy);
            }
        }

        // En demo, con un menu abierto, o con el inventario/mesa abiertos la
        // camara y el jugador quedan fijos (como en Minecraft: no se anda).
        if self.demo || !playing || self.inventory_open || self.crafting_open {
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

    /// Codigo de tecla asignado a una accion (o `default` si falta).
    fn binding_code(&self, action: &str, default: KeyCode) -> KeyCode {
        self.options
            .key_for(action)
            .and_then(super::input::parse_key)
            .unwrap_or(default)
    }

    /// Eje adelante/atras con las teclas asignadas.
    fn move_forward_axis(&self) -> f32 {
        self.input.axis_with(
            self.binding_code("forward", KeyCode::KeyW),
            self.binding_code("back", KeyCode::KeyS),
        )
    }

    /// Eje derecha/izquierda con las teclas asignadas.
    fn move_right_axis(&self) -> f32 {
        self.input.axis_with(
            self.binding_code("right", KeyCode::KeyD),
            self.binding_code("left", KeyCode::KeyA),
        )
    }

    /// Un paso de fisica del jugador de duracion `dt` (fija).
    fn simulate_player(&mut self, dt: f32) {
        // Movimiento con las teclas **asignadas** (reasignables en Controles).
        let forward = self.move_forward_axis();
        let right = self.move_right_axis();
        let jump = self
            .input
            .is_pressed(self.binding_code("jump", KeyCode::Space));
        let jump_held = jump;
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
        // Hotbar como stacks `(id, cantidad)`; ranuras vacias = `(0, 0)`.
        save.hotbar = self.inventory.hotbar_save();
        let chunks = save.chunks.len();
        let path = active_world_path(&self.world_base, &self.world_slug);
        let requested = match self.save_worker.as_ref() {
            Some(worker) => worker.request(save, path),
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
        let block = self.hotbar_block_sel();
        if let Some(renderer) = self.renderer.as_mut() {
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

    /// Bloques que muestra el inventario creativo: la categoria activa, o los
    /// resultados si hay busqueda.
    fn inventory_items(&self) -> Vec<crate::world::Block> {
        let cat = crate::world::CreativeCategory::ALL[self.inv_category.min(3)];
        crate::ui::inventory::view(self.lang, cat, &self.inv_search)
    }

    /// Rectangulos de la rejilla **visible** del inventario (9x5) y su esquina.
    fn inventory_cells(&self, win_w: f32, win_h: f32) -> Vec<[f32; 4]> {
        use crate::render::gui;
        let slot = gui::SLOT as f32 * UI_SCALE;
        let gap = 4.0;
        let cols = INV_COLS as f32;
        let rows = INV_ROWS as f32;
        let grid_w = cols * slot + (cols - 1.0) * gap;
        let grid_h = rows * slot + (rows - 1.0) * gap;
        let bar_h = gui::HOTBAR.h as f32 * UI_SCALE;
        let x0 = ((win_w - grid_w) * 0.5).floor();
        let y0 = ((win_h - bar_h - 8.0) - 12.0 - grid_h).floor();
        let mut out = Vec::with_capacity((cols * rows) as usize);
        for r in 0..INV_ROWS {
            for c in 0..INV_COLS {
                out.push([
                    x0 + c as f32 * (slot + gap),
                    y0 + r as f32 * (slot + gap),
                    slot,
                    slot,
                ]);
            }
        }
        out
    }

    /// Maximo desplazamiento (en filas) del inventario.
    fn inventory_max_scroll(&self) -> i32 {
        let items = self.inventory_items().len() as i32;
        let full_rows = (items + INV_COLS as i32 - 1) / INV_COLS as i32;
        (full_rows - INV_ROWS as i32).max(0)
    }

    /// Celdas visibles emparejadas con su bloque (para clic y dibujo).
    fn inventory_slots(&self, win_w: f32, win_h: f32) -> Vec<([f32; 4], crate::world::Block)> {
        let items = self.inventory_items();
        let start = (self.inv_scroll.max(0) as usize) * INV_COLS;
        self.inventory_cells(win_w, win_h)
            .into_iter()
            .enumerate()
            .filter_map(|(i, cell)| items.get(start + i).map(|b| (cell, *b)))
            .collect()
    }

    /// Rectangulos de las pestanas de categoria (una por categoria), centradas
    /// sobre la rejilla y con ancho segun el texto (para que no se solapen).
    fn inventory_tabs(&self, win_w: f32, win_h: f32) -> Vec<[f32; 4]> {
        use crate::render::font;
        let cells = self.inventory_cells(win_w, win_h);
        let Some(first) = cells.first() else {
            return Vec::new();
        };
        let cats = crate::world::CreativeCategory::ALL;
        let tab_h = 16.0;
        let gap = 4.0;
        let tab_w = cats
            .iter()
            .map(|c| font::text_width(crate::ui::translate(self.lang, c.key()), UI_SCALE))
            .fold(0.0, f32::max)
            + 12.0;
        let grid_w = cells
            .get(INV_COLS - 1)
            .map(|c| c[0] + c[2] - first[0])
            .unwrap_or(0.0);
        let total = cats.len() as f32 * (tab_w + gap) - gap;
        let x0 = first[0] + (grid_w - total).max(0.0) * 0.5;
        let y = (first[1] - tab_h - 6.0).max(2.0);
        (0..cats.len())
            .map(|i| [x0 + i as f32 * (tab_w + gap), y, tab_w, tab_h])
            .collect()
    }

    /// Construye los quads de la interfaz (hotbar + inventario).
    fn build_ui(&self, win_w: f32, win_h: f32) -> Vec<crate::render::UiQuad> {
        use crate::render::{UiQuad, font, gui, region_uv};
        use crate::world::Face;
        // En menus solo se dibuja el propio menu (sin HUD).
        if !self.screens.is_playing() {
            return self.build_menu_ui(win_w, win_h);
        }
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
        for (i, cell) in self.hotbar_cells(win_w, win_h).iter().enumerate() {
            let [sx, sy, _, _] = *cell;
            let stack = self.inventory.get(crate::ui::SlotRef::hotbar(i));
            if i == self.hotbar_sel {
                quads.push(UiQuad {
                    rect: [sx, sy, slot, slot],
                    uv: region_uv(gui::SELECTION),
                    layer: -1,
                });
            }
            // Hover: la ranura bajo el cursor se resalta.
            if (self.inventory_open || self.crafting_open)
                && self.cursor.0 >= sx
                && self.cursor.0 < sx + slot
                && self.cursor.1 >= sy
                && self.cursor.1 < sy + slot
            {
                quads.push(UiQuad {
                    rect: [sx, sy, slot, slot],
                    uv: region_uv(gui::SELECTION),
                    layer: -1,
                });
            }
            // Rebote de la ranura seleccionada (animacion breve, ~150 ms).
            let bounce = if i == self.hotbar_sel {
                4.0 * self.hotbar_bounce * (self.hotbar_bounce * std::f32::consts::PI).sin()
            } else {
                0.0
            };
            if let Some(stack) = stack
                && !stack.is_empty()
            {
                quads.push(UiQuad {
                    rect: [
                        sx + inset,
                        sy + inset - bounce,
                        slot - 2.0 * inset,
                        slot - 2.0 * inset,
                    ],
                    uv: [0.0, 0.0, 1.0, 1.0],
                    layer: stack.block.face_tile(Face::PosY) as i32,
                });
                if stack.count > 1 {
                    let txt = stack.count.to_string();
                    let tw = font::text_width(&txt, UI_SCALE);
                    quads.extend(font::text_quads(
                        &txt,
                        sx + slot - tw - 2.0,
                        sy + slot - 10.0 - bounce,
                        UI_SCALE,
                    ));
                }
            }
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

        // Inventario creativo: pestanas por categoria, busqueda y rejilla con
        // scroll (9x5 visibles).
        if self.inventory_open {
            // Pestanas de categoria (texto centrado).
            for (i, tab) in self.inventory_tabs(win_w, win_h).iter().enumerate() {
                let selected = i == self.inv_category;
                quads.push(UiQuad {
                    rect: *tab,
                    uv: region_uv(if selected {
                        gui::SELECTION
                    } else {
                        gui::SLOT_REGION
                    }),
                    layer: -1,
                });
                let cat = crate::world::CreativeCategory::ALL[i];
                let label = crate::ui::translate(self.lang, cat.key());
                let tw = font::text_width(label, UI_SCALE);
                quads.extend(font::text_quads(
                    label,
                    tab[0] + (tab[2] - tw) * 0.5,
                    tab[1] + 4.0,
                    UI_SCALE,
                ));
            }
            // Campo de busqueda.
            let cells = self.inventory_cells(win_w, win_h);
            if let Some(first) = cells.first() {
                let grid_w = cells
                    .get(INV_COLS - 1)
                    .map(|c| c[0] + c[2] - first[0])
                    .unwrap_or(9.0 * gui::SLOT as f32 * UI_SCALE);
                let sx = first[0];
                let sy = (first[1] - 40.0).max(2.0);
                quads.push(UiQuad {
                    rect: [sx, sy, grid_w, 16.0],
                    uv: region_uv(gui::DIM),
                    layer: -1,
                });
                let label = format!(
                    "{}: {}",
                    crate::ui::translate(self.lang, "ui.search"),
                    self.inv_search
                );
                quads.extend(font::text_quads(&label, sx + 4.0, sy + 4.0, UI_SCALE));
            }
            // Rejilla con los bloques visibles.
            for (cell, item) in self.inventory_slots(win_w, win_h) {
                quads.push(UiQuad {
                    rect: cell,
                    uv: region_uv(gui::SLOT_REGION),
                    layer: -1,
                });
                quads.push(UiQuad {
                    rect: [
                        cell[0] + inset,
                        cell[1] + inset,
                        cell[2] - 2.0 * inset,
                        cell[3] - 2.0 * inset,
                    ],
                    uv: [0.0, 0.0, 1.0, 1.0],
                    layer: item.face_tile(Face::PosY) as i32,
                });
            }
        }

        // Nombre del bloque sobre la hotbar (aparece al cambiar de ranura).
        if let Some((b, _)) = self.hotbar_toast {
            let name = crate::ui::lang::block_name(self.lang, b);
            let tw = font::text_width(name, UI_SCALE);
            let tx = ((win_w - tw) * 0.5).floor();
            let ty = (bar_y - 22.0).floor();
            quads.push(UiQuad {
                rect: [
                    tx - 4.0,
                    ty - 3.0,
                    tw + 8.0,
                    font::GLYPH_H as f32 * UI_SCALE + 6.0,
                ],
                uv: region_uv(gui::DIM),
                layer: -1,
            });
            quads.extend(font::text_quads(name, tx, ty, UI_SCALE));
        }

        // Stack "en el cursor" (inventario/mesa): sigue al raton.
        if (self.inventory_open || self.crafting_open)
            && let Some(stack) = self.inventory.cursor()
        {
            let s = gui::SLOT as f32 * UI_SCALE;
            let (mx, my) = self.cursor;
            let (x, y) = (mx - s * 0.5, my - s * 0.5);
            quads.push(UiQuad {
                rect: [x + inset, y + inset, s - 2.0 * inset, s - 2.0 * inset],
                uv: [0.0, 0.0, 1.0, 1.0],
                layer: stack.block.face_tile(Face::PosY) as i32,
            });
            if stack.count > 1 {
                let txt = stack.count.to_string();
                let tw = font::text_width(&txt, UI_SCALE);
                quads.extend(font::text_quads(
                    &txt,
                    x + s - tw - 2.0,
                    y + s - 10.0,
                    UI_SCALE,
                ));
            }
        }

        // Tooltip: nombre del bloque de la ranura bajo el cursor.
        if (self.inventory_open || self.crafting_open)
            && let Some(name) = self.hovered_block_name(win_w, win_h)
        {
            let tw = font::text_width(name, UI_SCALE);
            let (mx, my) = self.cursor;
            let tx = (mx + 12.0).min(win_w - tw - 8.0).max(2.0);
            let ty = my + 12.0;
            quads.push(UiQuad {
                rect: [
                    tx - 4.0,
                    ty - 3.0,
                    tw + 8.0,
                    font::GLYPH_H as f32 * UI_SCALE + 6.0,
                ],
                uv: region_uv(gui::DIM),
                layer: -1,
            });
            quads.extend(font::text_quads(name, tx, ty, UI_SCALE));
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

    /// Nombre del bloque de la ranura bajo el cursor (hotbar o catalogo), para
    /// el tooltip. `None` si no hay bloque.
    fn hovered_block_name(&self, win_w: f32, win_h: f32) -> Option<&'static str> {
        let (mx, my) = self.cursor;
        let inside =
            |r: &[f32; 4]| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3];
        for (i, cell) in self.hotbar_cells(win_w, win_h).iter().enumerate() {
            if inside(cell) {
                let b = self.inventory.hotbar_block(i);
                return (b != crate::world::Block::Air)
                    .then(|| crate::ui::lang::block_name(self.lang, b));
            }
        }
        for (cell, item) in self.inventory_slots(win_w, win_h) {
            if inside(&cell) {
                return Some(crate::ui::lang::block_name(self.lang, item));
            }
        }
        None
    }

    /// Pulsa en el inventario: pestanas, catalogo (copia infinita al cursor) o
    /// una ranura de la hotbar (click/arrastre).
    fn inventory_press(&mut self, button: crate::ui::Button, shift: bool) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        let inside =
            |r: &[f32; 4]| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3];
        for (i, tab) in self.inventory_tabs(win_w, win_h).iter().enumerate() {
            if inside(tab) {
                self.inv_category = i;
                self.inv_search.clear();
                self.inv_scroll = 0;
                return;
            }
        }
        for (cell, item) in self.inventory_slots(win_w, win_h) {
            if inside(&cell) {
                self.inventory
                    .set_cursor(Some(crate::world::ItemStack::full(item)));
                return;
            }
        }
        for (i, cell) in self.hotbar_cells(win_w, win_h).iter().enumerate() {
            if inside(cell) {
                self.inventory.press(
                    Some(crate::ui::SlotRef::hotbar(i)),
                    button,
                    shift,
                );
                return;
            }
        }
        self.inventory.press(None, button, false);
    }

    /// Suelta el boton en el inventario: aplica click o reparto.
    fn inventory_release(&mut self) {
        self.inventory.release();
    }

    /// Durante un arrastre, entra en la ranura de la hotbar bajo el cursor.
    fn inventory_drag_to(&mut self) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        for (i, cell) in self.hotbar_cells(win_w, win_h).iter().enumerate() {
            let [sx, sy, w, h] = *cell;
            if mx >= sx && mx < sx + w && my >= sy && my < sy + h {
                self.inventory
                    .drag_enter(crate::ui::SlotRef::hotbar(i));
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

    // --- Pantallas / menus ---------------------------------------------------

    /// Vuelve atras desde un menu (Esc). `Title` es la base.
    fn menu_back(&mut self) {
        match self.screens.top() {
            Screen::Pause | Screen::Options | Screen::Controls => {
                self.rebinding = None;
                self.screens.pop();
                // Al cerrar la pausa (y no quedar menus) se recaptura el raton.
                if self.screens.is_playing() && !self.demo {
                    self.try_capture();
                }
            }
            Screen::WorldSelect | Screen::CreateWorld => {
                self.rename_target = None;
                self.confirm_delete = None;
                self.screens.replace(Screen::Title);
            }
            Screen::Title | Screen::Playing => {}
        }
    }

    /// Vuelve a jugar (cierra menus) y recaptura el raton.
    fn resume_play(&mut self) {
        self.screens = ScreenStack::with_playing();
        if !self.demo {
            self.try_capture();
        }
    }

    /// Refresca la lista de mundos desde disco.
    fn reload_worlds(&mut self) {
        self.worlds = crate::world::library::list_worlds(&self.world_base);
        if self.world_sel >= self.worlds.len() {
            self.world_sel = self.worlds.len().saturating_sub(1);
        }
    }

    /// Carga y juega el mundo `slug`. Si ya era el activo, solo cierra los menus.
    fn enter_world(&mut self, slug: String) {
        if slug == self.world_slug && self.renderer.is_some() {
            self.resume_play();
            return;
        }
        self.load_world(slug);
    }

    /// Carga `slug` reconstruyendo el renderer (aunque sea el activo).
    fn load_world(&mut self, slug: String) {
        // Guarda el mundo actual en segundo plano (con su ruta) antes de cambiar;
        // el worker seguira vivo y escribira el mundo anterior mientras cargamos.
        if self.renderer.is_some() {
            self.save_world();
        }
        let base = self.world_base.clone();
        let path = active_world_path(&base, &slug);
        let (seed, restored, header, player_pos, hotbar) = match crate::world::load_and_migrate(&path) {
            Ok(save) => {
                let restored = save
                    .chunks
                    .iter()
                    .map(|(pos, rec)| (*pos, rec.clone()))
                    .collect();
                (save.header.seed, restored, save.header, save.player_pos, save.hotbar)
            }
            Err(e) => {
                eprintln!("[world] no se pudo cargar '{slug}': {e}");
                return;
            }
        };
        let player_pos =
            if player_pos[1] < 0.0 || player_pos[1] >= crate::world::WORLD_HEIGHT as f32 {
                crate::world::save::DEFAULT_PLAYER_POS
            } else {
                player_pos
            };
        let Some(window) = self.window.clone() else {
            return;
        };
        // Tipo de generador del mundo que se va a cargar (de sus metadatos).
        let kind =
            crate::world::library::load_meta(&crate::world::library::saves_dir(&base).join(&slug))
                .map(|m| crate::world::GeneratorKind::from_name(&m.generator_kind))
                .unwrap_or_default();
        match Renderer::new(
            window,
            seed,
            restored,
            self.options.view_settings().with_env_overrides(),
            kind,
        ) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[world] no se pudo crear el renderer: {e}");
                return;
            }
        }
        self.seed = seed;
        self.world_header = header;
        self.set_hotbar_from_save(&hotbar);
        self.world_slug = slug;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.warm_streaming(Vec3::new(player_pos[0], player_pos[1], player_pos[2]));
        }
        let mut camera = Camera::new(Vec3::new(player_pos[0], player_pos[1], player_pos[2]));
        camera.pitch_deg = -12.0;
        camera.fov_y_deg = self.options.fov_deg;
        camera.sensitivity_deg_per_px = self.options.mouse_sensitivity;
        if let Some(window) = self.window.as_ref() {
            let s = window.inner_size();
            camera.update_projection(s.width as f32 / s.height.max(1) as f32);
        }
        camera.update_view();
        if let Some(renderer) = self.renderer.as_ref() {
            let is_solid = |p: Vec3| renderer.is_solid_loaded_at(p);
            self.player.settle(&mut camera, is_solid);
        }
        self.player_pos = camera.position;
        self.prev_player_pos = camera.position;
        self.camera = Some(camera);
        self.world_saved = false;
        self.autosave_timer = 0.0;
        self.resume_play();
        println!("[world] jugando '{}'", self.world_slug);
    }

    /// Etiquetas de los botones de la pantalla actual.
    fn menu_labels(&self) -> Vec<String> {
        if self.confirm_delete.is_some() {
            return vec!["Si, eliminar".into(), "Cancelar".into()];
        }
        match self.screens.top() {
            Screen::Title => vec!["Un jugador".into(), "Opciones".into(), "Salir".into()],
            Screen::WorldSelect => vec![
                "Jugar".into(),
                "Crear mundo".into(),
                "Renombrar".into(),
                "Duplicar".into(),
                "Eliminar".into(),
                "Volver".into(),
            ],
            Screen::CreateWorld => {
                if self.rename_target.is_some() {
                    vec!["Renombrar".into(), "Volver".into()]
                } else {
                    vec!["Crear".into(), "Volver".into()]
                }
            }
            Screen::Options => {
                let o = &self.options;
                vec![
                    format!("Distancia: {} chunks", o.render_radius),
                    format!("Simulación: {} chunks", o.sim_radius),
                    format!("Niebla: {}", o.fog),
                    format!("FOV: {:.0}", o.fov_deg),
                    format!("Sensibilidad: {:.2}", o.mouse_sensitivity),
                    format!("Idioma: {}", o.lang),
                    format!("Autoguardado: {:.0} s", o.autosave_secs),
                    format!("F3 al iniciar: {}", if o.show_f3 { "sí" } else { "no" }),
                    format!(
                        "Reducir movimiento: {}",
                        if o.reduce_motion { "sí" } else { "no" }
                    ),
                    "Hecho".into(),
                ]
            }
            Screen::Controls => {
                let mut rows: Vec<String> = crate::ui::options::BINDABLE_ACTIONS
                    .iter()
                    .map(|(action, key_key)| {
                        let label = crate::ui::translate(self.lang, key_key);
                        if self.rebinding.as_deref() == Some(*action) {
                            format!("{label}: <pulsa una tecla>")
                        } else {
                            let k = self
                                .options
                                .key_for(action)
                                .map(super::input::key_display)
                                .unwrap_or_else(|| "?".to_string());
                            format!("{label}: {k}")
                        }
                    })
                    .collect();
                rows.push("Restablecer".into());
                rows.push("Hecho".into());
                rows
            }
            Screen::Pause => vec![
                "Volver al juego".into(),
                "Opciones".into(),
                "Controles".into(),
                "Guardar mundo ahora".into(),
                "Guardar y salir al titulo".into(),
            ],
            Screen::Playing => Vec::new(),
        }
    }

    /// Rectangulos de los botones (240x22, apilados y centrados).
    fn menu_button_rects(&self, win_w: f32, win_h: f32, n: usize) -> Vec<[f32; 4]> {
        let w = 240.0;
        let h = 22.0;
        let gap = 6.0;
        let total = n as f32 * h + (n as f32 - 1.0).max(0.0) * gap;
        let x = ((win_w - w) * 0.5).floor();
        let y0 = ((win_h - total) * 0.5).floor();
        (0..n)
            .map(|i| [x, y0 + i as f32 * (h + gap), w, h])
            .collect()
    }

    /// Navegacion por teclado en menus con botones: flechas mueven el foco
    /// (visible), Enter/Espacio activan el boton enfocado.
    fn menu_nav_key(&mut self, code: KeyCode) {
        let n = self.menu_labels().len();
        if n == 0 {
            return;
        }
        self.menu_focus = self.menu_focus.min(n - 1);
        match code {
            KeyCode::ArrowUp => self.menu_focus = self.menu_focus.saturating_sub(1),
            KeyCode::ArrowDown => {
                if self.menu_focus + 1 < n {
                    self.menu_focus += 1;
                }
            }
            KeyCode::Enter | KeyCode::Space => self.menu_action(self.menu_focus),
            _ => {}
        }
    }

    /// Indice del boton de menu bajo el cursor, si lo hay.
    fn menu_button_at(&self) -> Option<usize> {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        let labels = self.menu_labels();
        self.menu_button_rects(win_w, win_h, labels.len())
            .iter()
            .position(|r| mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3])
    }

    /// Ejecuta la accion del boton `i` de la pantalla actual.
    fn menu_action(&mut self, i: usize) {
        if let Some(slug) = self.confirm_delete.clone() {
            if i == 0 {
                let _ = crate::world::library::delete_world(&self.world_base, &slug);
            }
            self.confirm_delete = None;
            self.reload_worlds();
            return;
        }
        match self.screens.top() {
            Screen::Title => match i {
                0 => {
                    self.reload_worlds();
                    self.screens.replace(Screen::WorldSelect);
                }
                1 => self.screens.push(Screen::Options),
                2 => self.exit_requested = true,
                _ => {}
            },
            Screen::WorldSelect => {
                let sel = self.worlds.get(self.world_sel).cloned();
                match i {
                    0 => {
                        if let Some(w) = sel {
                            self.enter_world(w.slug);
                        }
                    }
                    1 => {
                        self.rename_target = None;
                        self.create_name = "Mundo nuevo".into();
                        self.create_seed = String::new();
                        self.create_focus = 0;
                        self.screens.replace(Screen::CreateWorld);
                    }
                    2 => {
                        if let Some(w) = sel {
                            self.rename_target = Some(w.slug);
                            self.create_name = w.meta.display_name;
                            self.create_focus = 0;
                            self.screens.replace(Screen::CreateWorld);
                        }
                    }
                    3 => {
                        if let Some(w) = sel {
                            let _ = crate::world::library::duplicate_world(
                                &self.world_base,
                                &w.slug,
                                now_unix(),
                            );
                            self.reload_worlds();
                        }
                    }
                    4 => {
                        if let Some(w) = sel {
                            self.confirm_delete = Some(w.slug);
                        }
                    }
                    5 => self.screens.replace(Screen::Title),
                    _ => {}
                }
            }
            Screen::CreateWorld => match i {
                0 => self.submit_create_or_rename(),
                1 => {
                    self.rename_target = None;
                    self.screens.replace(Screen::WorldSelect);
                }
                _ => {}
            },
            Screen::Pause => match i {
                0 => self.resume_play(),
                1 => self.screens.push(Screen::Options),
                2 => {
                    self.rebinding = None;
                    self.screens.push(Screen::Controls);
                }
                3 => self.save_world(),
                4 => {
                    self.save_world();
                    self.screens.to_title();
                }
                _ => {}
            },
            Screen::Controls => self.controls_action(i),
            Screen::Options => self.options_action(i),
            Screen::Playing => {}
        }
    }

    /// Accion de una fila de Controles: iniciar rebind, restablecer o salir.
    fn controls_action(&mut self, i: usize) {
        let n = crate::ui::options::BINDABLE_ACTIONS.len();
        if i < n {
            self.rebinding = Some(crate::ui::options::BINDABLE_ACTIONS[i].0.to_string());
        } else if i == n {
            self.options.reset_bindings();
            self.save_options();
        } else {
            self.rebinding = None;
            self.menu_back();
        }
    }

    /// Guarda `options.json` (sin aplicar).
    fn save_options(&self) {
        let path = self.world_base.join(crate::ui::options::OPTIONS_FILE);
        if let Err(e) = self.options.save(&path) {
            eprintln!("[options] no se pudo guardar: {e}");
        }
    }

    /// Accion de una fila de opciones (cicla el valor) o "Hecho".
    fn options_action(&mut self, i: usize) {
        let prev_radius = self.options.render_radius;
        let prev_sim = self.options.sim_radius;
        match i {
            0 => {
                self.options.render_radius = if self.options.render_radius >= 32 {
                    2
                } else {
                    self.options.render_radius + 2
                }
            }
            1 => {
                self.options.sim_radius = if self.options.sim_radius >= 12 {
                    1
                } else {
                    self.options.sim_radius + 1
                }
            }
            2 => self.options.cycle_fog(),
            3 => {
                self.options.fov_deg = if self.options.fov_deg >= 110.0 {
                    30.0
                } else {
                    self.options.fov_deg + 10.0
                }
            }
            4 => {
                self.options.mouse_sensitivity = if self.options.mouse_sensitivity >= 0.5 {
                    0.02
                } else {
                    self.options.mouse_sensitivity + 0.04
                }
            }
            5 => self.options.cycle_lang(),
            6 => {
                self.options.autosave_secs = if self.options.autosave_secs >= 900.0 {
                    30.0
                } else {
                    self.options.autosave_secs + 60.0
                }
            }
            7 => self.options.show_f3 = !self.options.show_f3,
            8 => self.options.reduce_motion = !self.options.reduce_motion,
            9 => {
                // "Hecho": vuelve a la pantalla anterior.
                self.menu_back();
                return;
            }
            _ => return,
        }
        self.options.clamp();
        self.apply_options();
        // Distancia de vista/simulacion requiere reconstruir el mundo.
        if self.options.render_radius != prev_radius || self.options.sim_radius != prev_sim {
            let slug = self.world_slug.clone();
            self.load_world(slug);
            self.screens.push(Screen::Options);
        }
    }

    /// Aplica las opciones a la camara/idioma/autoguardado y las persiste.
    fn apply_options(&mut self) {
        self.lang = if self.options.lang == "en" {
            crate::ui::Lang::En
        } else {
            crate::ui::Lang::Es
        };
        self.autosave_period = self.options.autosave_secs;
        let (fov, sens) = (self.options.fov_deg, self.options.mouse_sensitivity);
        if let Some(camera) = self.camera.as_mut() {
            camera.fov_y_deg = fov;
            camera.sensitivity_deg_per_px = sens;
            if let Some(window) = self.window.as_ref() {
                let s = window.inner_size();
                camera.update_projection(s.width as f32 / s.height.max(1) as f32);
            }
            camera.update_view();
        }
        let path = self.world_base.join(crate::ui::options::OPTIONS_FILE);
        if let Err(e) = self.options.save(&path) {
            eprintln!("[options] no se pudo guardar: {e}");
        }
    }

    /// Crea un mundo nuevo o renombra el seleccionado, segun `rename_target`.
    fn submit_create_or_rename(&mut self) {
        let name = if self.create_name.trim().is_empty() {
            "Mundo nuevo".to_string()
        } else {
            self.create_name.trim().to_string()
        };
        if let Some(slug) = self.rename_target.take() {
            if let Err(e) = crate::world::library::rename_world(&self.world_base, &slug, &name) {
                eprintln!("[world] no se pudo renombrar: {e}");
            }
            self.reload_worlds();
            self.screens.replace(Screen::WorldSelect);
            return;
        }
        let seed = crate::world::seed_from_text(&self.create_seed);
        match crate::world::library::create_world_kind(
            &self.world_base,
            &name,
            seed,
            now_unix(),
            generator_kind_from_env().name(),
        ) {
            Ok(entry) => {
                self.reload_worlds();
                self.enter_world(entry.slug);
            }
            Err(e) => eprintln!("[world] no se pudo crear el mundo: {e}"),
        }
    }

    /// Clic en un menu: activa el boton bajo el cursor.
    fn menu_click(&mut self) {
        let (win_w, win_h) = self.window_size_f();
        let (mx, my) = self.cursor;
        let labels = self.menu_labels();
        for (i, r) in self
            .menu_button_rects(win_w, win_h, labels.len())
            .iter()
            .enumerate()
        {
            if mx >= r[0] && mx < r[0] + r[2] && my >= r[1] && my < r[1] + r[3] {
                self.menu_action(i);
                return;
            }
        }
    }

    /// Tecla en un menu: escribe en campos o activa botones.
    fn menu_key(&mut self, code: KeyCode, text: Option<&str>) {
        match self.screens.top() {
            Screen::CreateWorld => match code {
                KeyCode::Tab => self.create_focus = 1 - self.create_focus.min(1),
                KeyCode::Backspace => {
                    if self.create_focus == 0 {
                        self.create_name.pop();
                    } else {
                        self.create_seed.pop();
                    }
                }
                KeyCode::Enter => self.menu_action(0),
                _ => {
                    if let Some(t) = text {
                        for ch in t.chars() {
                            if !(ch.is_alphanumeric() || ch == ' ' || ch == '_' || ch == '-') {
                                continue;
                            }
                            let field = if self.create_focus == 0 {
                                &mut self.create_name
                            } else {
                                &mut self.create_seed
                            };
                            let max = if self.create_focus == 0 { 32 } else { 12 };
                            if field.chars().count() < max {
                                field.push(ch);
                            }
                        }
                    }
                }
            },
            Screen::WorldSelect => match code {
                KeyCode::ArrowUp => {
                    self.world_sel = self.world_sel.saturating_sub(1);
                }
                KeyCode::ArrowDown => {
                    if self.world_sel + 1 < self.worlds.len() {
                        self.world_sel += 1;
                    }
                }
                KeyCode::Enter => self.menu_action(0),
                _ => {}
            },
            Screen::Title | Screen::Pause | Screen::Options => {
                self.menu_nav_key(code);
            }
            Screen::Controls => {
                // Si hay una accion esperando tecla, la captura y reasigna.
                if let Some(action) = self.rebinding.clone()
                    && let Some(name) = super::input::key_name(code)
                {
                    if let Some(other) = self.options.rebind(&action, name) {
                        println!("[controles] '{action}' intercambiada con '{other}'");
                    }
                    self.save_options();
                    self.rebinding = None;
                } else {
                    self.menu_nav_key(code);
                }
            }
            Screen::Playing => {}
        }
    }

    /// Quads del **menu/titulo** (fondo, logo, botones, listas) y su leyenda.
    fn build_menu_ui(&self, win_w: f32, win_h: f32) -> Vec<crate::render::UiQuad> {
        use crate::render::{UiQuad, font, gui, region_uv};
        let top = self.screens.top();
        if top == Screen::Playing {
            return Vec::new();
        }
        let mut quads: Vec<UiQuad> = Vec::new();
        // Atenua el mundo detras.
        quads.push(UiQuad {
            rect: [0.0, 0.0, win_w, win_h],
            uv: region_uv(gui::DIM),
            layer: -1,
        });

        // Titulo grande (logo textual) arriba.
        let title = match top {
            Screen::Title => "SOLARIA VOXEL".to_string(),
            Screen::WorldSelect => "SELECCIONAR MUNDO".to_string(),
            Screen::CreateWorld => {
                if self.rename_target.is_some() {
                    "RENOMBRAR MUNDO".to_string()
                } else {
                    "CREAR MUNDO".to_string()
                }
            }
            Screen::Pause => "PAUSA".to_string(),
            Screen::Options => "OPCIONES".to_string(),
            Screen::Controls => "CONTROLES".to_string(),
            Screen::Playing => String::new(),
        };
        let scale = UI_SCALE * 2.0;
        let tw = font::text_width(&title, scale);
        quads.extend(font::text_quads(
            &title,
            ((win_w - tw) * 0.5).floor(),
            40.0,
            scale,
        ));

        // Lista de mundos (selector).
        if top == Screen::WorldSelect {
            let list_x = ((win_w - 360.0) * 0.5).floor();
            let mut ly = 96.0;
            for (i, w) in self.worlds.iter().take(6).enumerate() {
                let selected = i == self.world_sel;
                let color = if selected { gui::SELECTION } else { gui::DIM };
                quads.push(UiQuad {
                    rect: [list_x, ly, 360.0, 18.0],
                    uv: region_uv(color),
                    layer: -1,
                });
                let tag = if w.corrupt { " (corrupto)" } else { "" };
                let line = format!(
                    "{}{}  semilla {}  [{}]",
                    w.meta.display_name, tag, w.meta.seed, w.meta.generator_kind
                );
                quads.extend(font::text_quads(&line, list_x + 4.0, ly + 4.0, UI_SCALE));
                ly += 20.0;
            }
            if self.worlds.is_empty() {
                quads.extend(font::text_quads(
                    "No hay mundos. Crea uno.",
                    list_x + 4.0,
                    ly,
                    UI_SCALE,
                ));
            }
        }

        // Campos de texto (crear/renombrar).
        if top == Screen::CreateWorld {
            let fx = ((win_w - 360.0) * 0.5).floor();
            let mut fy = 96.0;
            for field in 0..2 {
                let focused = field == self.create_focus;
                if field == 0 {
                    quads.extend(font::text_quads("Nombre:", fx, fy, UI_SCALE));
                } else if self.rename_target.is_none() {
                    quads.extend(font::text_quads("Semilla:", fx, fy, UI_SCALE));
                } else {
                    // Renombrar no usa semilla; deja hueco.
                    fy += 18.0;
                    continue;
                }
                let value = if field == 0 {
                    self.create_name.clone()
                } else {
                    self.create_seed.clone()
                };
                let box_x = fx + 90.0;
                quads.push(UiQuad {
                    rect: [box_x, fy - 2.0, 260.0, 16.0],
                    uv: region_uv(if focused { gui::SELECTION } else { gui::DIM }),
                    layer: -1,
                });
                quads.extend(font::text_quads(&value, box_x + 3.0, fy, UI_SCALE));
                fy += 18.0;
            }
        }

        // Botones con estado (normal/hover/pulsado) + foco de teclado.
        let labels = self.menu_labels();
        let n = labels.len();
        let focus = self.menu_focus.min(n.saturating_sub(1));
        let (mx, my) = self.cursor;
        for (i, (label, rect)) in labels
            .iter()
            .zip(self.menu_button_rects(win_w, win_h, n))
            .enumerate()
        {
            let hover =
                mx >= rect[0] && mx < rect[0] + rect[2] && my >= rect[1] && my < rect[1] + rect[3];
            let region = if self.menu_pressed == Some(i) {
                gui::BUTTON_PRESSED
            } else if hover || i == focus {
                gui::BUTTON_HOVER
            } else {
                gui::BUTTON
            };
            quads.push(UiQuad {
                rect,
                uv: region_uv(region),
                layer: -1,
            });
            if i == focus {
                // Marco de foco (1 px por dentro).
                let border = region_uv(gui::SELECTION);
                quads.push(UiQuad { rect: [rect[0], rect[1], rect[2], 1.0], uv: border, layer: -1 });
                quads.push(UiQuad { rect: [rect[0], rect[1] + rect[3] - 1.0, rect[2], 1.0], uv: border, layer: -1 });
                quads.push(UiQuad { rect: [rect[0], rect[1], 1.0, rect[3]], uv: border, layer: -1 });
                quads.push(UiQuad { rect: [rect[0] + rect[2] - 1.0, rect[1], 1.0, rect[3]], uv: border, layer: -1 });
            }
            let lw = font::text_width(label, UI_SCALE);
            quads.extend(font::text_quads(
                label,
                rect[0] + (rect[2] - lw) * 0.5,
                rect[1] + 4.0,
                UI_SCALE,
            ));
        }

        // Version del motor abajo a la izquierda.
        let v = concat!("v", env!("CARGO_PKG_VERSION"));
        quads.extend(font::text_quads(v, 6.0, win_h - 14.0, UI_SCALE));
        quads
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
                self.set_hotbar_sel(out);
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
        // 3. Inventario: pone el bloque en la primera celda libre. Se usa la
        // **misma lista filtrada** que se dibuja (categoria + busqueda), no
        // `items()` completo: si no, se insertaba un bloque distinto al visible.
        for (cell, item) in self.inventory_slots(win_w, win_h) {
            if inside(&cell) {
                if let Some(j) = self.craft_grid.iter().position(|c| c.is_none()) {
                    self.craft_grid[j] = Some(item);
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

        // Libreria de mundos: base (`saves/`), importar el `world.vf` antiguo y
        // elegir el mundo mas reciente. Si no hay ninguno, se crea uno.
        let base = crate::world::library::base_dir_from_env();
        let now = now_unix();
        let _ = crate::world::library::import_legacy(&base, now);
        let mut worlds = crate::world::library::list_worlds(&base);
        if worlds.is_empty() {
            match crate::world::library::create_world_kind(
                &base,
                "Mundo nuevo",
                13_371,
                now,
                generator_kind_from_env().name(),
            ) {
                Ok(w) => worlds.push(w),
                Err(e) => eprintln!("[world] no se pudo crear el mundo inicial: {e}"),
            }
        }
        self.world_base = base.clone();
        self.worlds = worlds;
        // Opciones persistentes (video/juego/teclas) del archivo global.
        self.options = crate::ui::Options::load(&base.join(crate::ui::options::OPTIONS_FILE));
        self.autosave_period = self.options.autosave_secs;
        self.lang = if self.options.lang == "en" {
            crate::ui::Lang::En
        } else {
            crate::ui::Lang::Es
        };
        if let Some(first) = self.worlds.first() {
            self.world_slug = first.slug.clone();
        }
        let path = active_world_path(&base, &self.world_slug);

        // Cargamos el mundo de disco si existe (semilla + chunks editados + pos).
        let (seed, restored, header, player_pos, hotbar) = match crate::world::load_and_migrate(&path) {
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
                (save.header.seed, restored, save.header, save.player_pos, save.hotbar)
            }
            Err(e) => {
                println!("[world] sin mundo previo ({e}); se crea uno nuevo (semilla 13371)");
                let seed = 13_371;
                (
                    seed,
                    Vec::new(),
                    crate::world::WorldHeader::new(seed, now_unix()),
                    crate::world::save::DEFAULT_PLAYER_POS,
                    crate::world::save::default_hotbar(),
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

        // Tipo de generador del mundo activo (de sus metadatos `level.json`).
        let kind = self
            .worlds
            .first()
            .map(|w| crate::world::GeneratorKind::from_name(&w.meta.generator_kind))
            .unwrap_or_default();
        match Renderer::new(
            window.clone(),
            seed,
            restored,
            self.options.view_settings().with_env_overrides(),
            kind,
        ) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[engine] no se pudo iniciar el renderer: {e}");
                event_loop.exit();
                return;
            }
        }
        self.seed = seed;
        self.world_header = header;
        self.set_hotbar_from_save(&hotbar);
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
        // Barra rapida por defecto: los primeros `HOTBAR_SLOTS` items.
        self.inventory
            .set_hotbar_from_save(&crate::world::save::default_hotbar());

        // Demo de interfaz: abrir el inventario, una busqueda y el nombre del
        // bloque sobre la hotbar (para capturas sin interaccion).
        if let Ok(q) = std::env::var("SOLARIA_SEARCH") {
            self.inv_search = q;
            self.inventory_open = true;
        }
        if std::env::var("SOLARIA_INVENTORY").is_ok() {
            self.inventory_open = true;
        }
        if let Ok(v) = std::env::var("SOLARIA_TOAST") {
            let idx = v.parse::<usize>().unwrap_or(2).min(HOTBAR_SLOTS - 1);
            self.hotbar_sel = idx;
            self.hotbar_toast = Some((self.hotbar_block(idx), 9999.0));
        }

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
        camera.fov_y_deg = self.options.fov_deg;
        camera.sensitivity_deg_per_px = self.options.mouse_sensitivity;
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
        self.show_stats = std::env::var("SOLARIA_STATS").is_ok() || self.options.show_f3;
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

        // Pantallas: en demo se juega directo; si no, se empieza en el titulo.
        self.screens = if self.demo {
            ScreenStack::with_playing()
        } else {
            ScreenStack::with_title()
        };
        if let Ok(scr) = std::env::var("SOLARIA_SCREEN") {
            self.screens = match scr.as_str() {
                "title" => ScreenStack::with_title(),
                "worlds" => {
                    let mut s = ScreenStack::with_title();
                    s.replace(Screen::WorldSelect);
                    s
                }
                "create" => {
                    let mut s = ScreenStack::with_title();
                    s.replace(Screen::CreateWorld);
                    s
                }
                "pause" => {
                    let mut s = ScreenStack::with_playing();
                    s.push(Screen::Pause);
                    s
                }
                "options" => {
                    let mut s = ScreenStack::with_title();
                    s.push(Screen::Options);
                    s
                }
                "controls" => {
                    let mut s = ScreenStack::with_title();
                    s.push(Screen::Controls);
                    s
                }
                _ => self.screens.clone(),
            };
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
                    // `event.repeat` filtra el auto-repeat del SO: si no, mantener
                    // Escape/F/E invertiria el estado ~30 veces/s.
                    let pressed = event.state == ElementState::Pressed && !event.repeat;
                    let playing = self.screens.is_playing();

                    if code == KeyCode::Escape && pressed {
                        // Un solo Esc por capa: la maquina de estados decide
                        // (abre pausa y libera cursor, o cierra el overlay y
                        // recaptura). Ver `ui::input_mode`.
                        self.input_mode.set(self.current_mode());
                        let effects = self.input_mode.on_escape();
                        self.apply_input_effects(effects);
                    } else if !playing {
                        // En menus: el teclado escribe en campos o activa botones.
                        if pressed {
                            self.menu_key(code, event.text.as_deref());
                        }
                    } else if pressed {
                        match code {
                            // E (o la tecla asignada a "inventario"): abre/cierra
                            // el inventario o la mesa; cierra recapturando cursor.
                            c if c == self.binding_code("inventory", KeyCode::KeyE) => {
                                self.input_mode.set(self.current_mode());
                                let effects = self.input_mode.on_inventory_key();
                                self.apply_input_effects(effects);
                            }
                            // F (o la tecla asignada a "volar"): alterna modo vuelo.
                            c if c == self.binding_code("fly", KeyCode::KeyF) => {
                                self.flying = !self.flying;
                                println!(
                                    "[engine] modo vuelo: {}",
                                    if self.flying { "ON" } else { "OFF" }
                                );
                            }
                            // F3: overlay de diagnostico.
                            KeyCode::F3 => {
                                self.show_stats = !self.show_stats;
                                println!(
                                    "[engine] overlay F3: {}",
                                    if self.show_stats { "ON" } else { "OFF" }
                                );
                            }
                            // F5: primera/tercera persona.
                            KeyCode::F5 => {
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
                            // Con el inventario abierto, escribe en la busqueda; si
                            // no, `1`-`9` seleccionan ranura.
                            _ => {
                                if self.inventory_open {
                                    if code == KeyCode::Backspace {
                                        self.inv_search.pop();
                                        self.inv_scroll = 0;
                                    } else if let Some(text) = event.text.as_deref() {
                                        for ch in text.chars() {
                                            if (ch.is_alphanumeric() || ch == ' ')
                                                && self.inv_search.chars().count() < 24
                                            {
                                                self.inv_search.push(ch);
                                            }
                                        }
                                        self.inv_scroll = 0;
                                    }
                                } else if let Some(slot) = digit_slot(code) {
                                    self.hotbar_sel = slot;
                                    self.hotbar_bounce = 1.0;
                                    self.hotbar_toast = Some((self.hotbar_block(slot), 2.0));
                                    println!(
                                        "[engine] ranura {} ({:?})",
                                        slot + 1,
                                        self.hotbar_block(slot)
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Botones del raton.
            WindowEvent::MouseInput { state, button, .. } => {
                if !self.screens.is_playing() {
                    // En menus, el clic izquierdo activa el boton bajo el cursor;
                    // se marca "pulsado" mientras el boton esta hundido.
                    match state {
                        ElementState::Pressed if button == MouseButton::Left => {
                            self.menu_pressed = self.menu_button_at();
                            self.menu_click();
                        }
                        ElementState::Released => self.menu_pressed = None,
                        _ => {}
                    }
                    return;
                }
                // Inventario: press/release (click y arrastre de stacks).
                if self.inventory_open {
                    let our = match button {
                        MouseButton::Left => Some(crate::ui::Button::Left),
                        MouseButton::Right => Some(crate::ui::Button::Right),
                        _ => None,
                    };
                    if let Some(b) = our {
                        let shift = self.input.is_pressed(KeyCode::ShiftLeft)
                            || self.input.is_pressed(KeyCode::ShiftRight);
                        match state {
                            ElementState::Pressed => self.inventory_press(b, shift),
                            ElementState::Released => self.inventory_release(),
                        }
                    }
                    return;
                }
                if state != ElementState::Pressed {
                    return;
                }
                match button {
                    // Click izquierdo: con mesa abierta va a la mesa; capturado,
                    // rompe; si no, captura el cursor.
                    MouseButton::Left => {
                        if self.crafting_open {
                            self.crafting_click();
                        } else if self.mouse_locked {
                            self.break_block();
                        } else {
                            self.want_capture = true;
                            self.try_capture();
                        }
                    }
                    // Click derecho: sobre una mesa la abre; si no, coloca.
                    MouseButton::Right if self.mouse_locked => {
                        let aimed_table = self
                            .selection
                            .zip(self.renderer.as_ref())
                            .map(|(hit, r)| {
                                r.block_at(hit.block) == crate::world::Block::CraftingTable
                            })
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
                }
            }

            // Rueda del raton: cambia de ranura en la hotbar.
            WindowEvent::MouseWheel { delta, .. } => {
                if !self.screens.is_playing() {
                    // En menus no hace nada (los botones se pulsan).
                } else if !self.inventory_open {
                    use winit::event::MouseScrollDelta;
                    let step = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y.signum(),
                        MouseScrollDelta::PixelDelta(p) => p.y.signum() as f32,
                    };
                    if step > 0.0 {
                        self.hotbar_sel = (self.hotbar_sel + 8) % 9;
                        self.hotbar_bounce = 1.0;
                    } else if step < 0.0 {
                        self.hotbar_sel = (self.hotbar_sel + 1) % 9;
                        self.hotbar_bounce = 1.0;
                    }
                    self.hotbar_toast = Some((self.hotbar_block_sel(), 2.0));
                } else {
                    // Con el inventario abierto, la rueda hace scroll.
                    use winit::event::MouseScrollDelta;
                    let step = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y.signum(),
                        MouseScrollDelta::PixelDelta(p) => p.y.signum() as f32,
                    };
                    let max = self.inventory_max_scroll();
                    if step > 0.0 {
                        self.inv_scroll = (self.inv_scroll - 1).clamp(0, max);
                    } else if step < 0.0 {
                        self.inv_scroll = (self.inv_scroll + 1).clamp(0, max);
                    }
                }
            }

            // Posicion del cursor (para el inventario).
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                // Arrastre de stacks: registra la ranura bajo el cursor.
                if self.inventory_open {
                    self.inventory_drag_to();
                }
            }

            // Si perdemos el foco (alt-tab), liberamos el cursor y **olvidamos las
            // teclas**: si el SO no entrega los `Released`, el jugador seguiria
            // andando solo al volver.
            WindowEvent::Focused(false) => {
                if self.mouse_locked {
                    self.unlock_mouse();
                }
                self.input.clear();
            }

            // Al recuperar el foco, si quedaba una captura pedida (el SO la
            // rechazo por falta de foco) la reintentamos.
            WindowEvent::Focused(true) => {
                if self.want_capture {
                    self.try_capture();
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
                // En modo demo o en menus no resaltamos (vista limpia).
                if !self.demo && self.screens.is_playing() {
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
                let held_block = self.hotbar_block_sel();
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
                    // Mano solo en primera persona y jugando (no en demo/menus).
                    let hand = (!self.demo && !third && self.screens.is_playing()).then(|| {
                        crate::render::HandView {
                            projection: camera.projection(),
                            swing: self.swing,
                            bob: self.bob,
                        }
                    });
                    // Personaje solo en tercera persona.
                    let character = third.then(|| crate::render::CharacterView {
                        view_projection,
                        world: crate::scene::player::character_matrix(feet, camera.yaw_deg),
                        walk: self.bob,
                    });
                    renderer.set_hand_item(held_block);
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
        if self.exit_requested {
            self.finalize_save();
            _event_loop.exit();
            return;
        }
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
        let mut app = App {
            screens: ScreenStack::with_playing(),
            ..Default::default()
        };
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
