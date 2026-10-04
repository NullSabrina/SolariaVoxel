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

use crate::engine::input::Input;
use crate::engine::window;
use crate::math::Vec3;
use crate::player::PlayerController;
use crate::render::Renderer;
use crate::scene::Camera;

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
    /// Marca de tiempo del frame anterior, para calcular el `dt`.
    last_frame: Option<Instant>,
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

        // Movimiento horizontal (el vertical lo resuelve la fisica).
        if forward != 0.0 || right != 0.0 {
            camera.walk(forward, right, 0.0, dt);
        }

        // Fisica vertical. La consulta de solido ignora la componente Y del
        // punto (ya la elige el controlador): solo usamos x/z para localizar el
        // bloque dentro del chunk central.
        let world = renderer;
        let is_solid = move |point: Vec3| -> bool {
            world.block_at(point).map(|b| b.is_solid()).unwrap_or(false)
        };

        // En modo vuelo, Espacio/Shift suben/bajan; en modo normal Space salta.
        let shift = self.input.is_pressed(winit::keyboard::KeyCode::ShiftLeft)
            || self.input.is_pressed(winit::keyboard::KeyCode::ShiftRight);
        let fly_up = if flying {
            (jump_held as i32 - shift as i32) as f32
        } else {
            0.0
        };
        let mut player = self.player;
        player.update(camera, is_solid, fly_up, flying, jump && !flying, dt);
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
            if block_overlaps_player(target, camera.position) {
                return;
            }
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_block(target, crate::world::Block::Stone);
            println!("[edit] bloque colocado en {target:?}");
        }
        self.update_selection();
    }
}

/// ¿El bloque `voxel` ocupa el espacio del jugador (pies..cabeza)?
fn block_overlaps_player(voxel: [i32; 3], eye: Vec3) -> bool {
    use crate::player::{EYE_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS};
    let (bx, by, bz) = (voxel[0] as f32, voxel[1] as f32, voxel[2] as f32);
    let feet = eye.y - EYE_HEIGHT;
    // El jugador se aproxima por una columna de radio PLAYER_RADIUS.
    let overlaps_xz = (bx + 1.0 > eye.x - PLAYER_RADIUS)
        && (bx < eye.x + PLAYER_RADIUS)
        && (bz + 1.0 > eye.z - PLAYER_RADIUS)
        && (bz < eye.z + PLAYER_RADIUS);
    let overlaps_y = (by + 1.0 > feet) && (by < feet + PLAYER_HEIGHT);
    overlaps_xz && overlaps_y
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

        match Renderer::new(window.clone()) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[engine] no se pudo iniciar el renderer: {e}");
                event_loop.exit();
                return;
            }
        }

        // Camara FPS: en el centro del chunk, a ras de suelo. La fisica la
        // posara sobre el terreno antes del primer frame.
        let mut camera = Camera::new(Vec3::new(8.0, 100.0, 8.0));
        camera.pitch_deg = -35.0;
        let size = window.inner_size();
        camera.update_projection(size.width as f32 / size.height.max(1) as f32);
        camera.update_view();
        // Coloca la camara sobre el primer bloque solido bajo ella.
        {
            let world = self.renderer.as_ref().unwrap();
            let is_solid = |p: Vec3| world.block_at(p).map(|b| b.is_solid()).unwrap_or(false);
            self.player.settle(&mut camera, is_solid);
        }
        println!("[engine] jugador posado en y={:.2}", camera.position.y);
        self.camera = Some(camera);

        self.last_frame = Some(Instant::now());
        self.window = Some(window);

        println!("[engine] click = capturar raton | WASD = andar | Espacio = saltar");
        println!("[engine] F = volar (Espacio/Shift sube/baja) | Escape = salir");
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
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Guardamos el estado (necesario para el movimiento continuo).
                    self.input.on_key(code, event.state);

                    match code {
                        // Escape: libera el raton; si ya esta libre, sale.
                        KeyCode::Escape if event.state == ElementState::Pressed => {
                            if self.mouse_locked {
                                self.unlock_mouse();
                            } else {
                                event_loop.exit();
                            }
                        }
                        // F: alterna modo vuelo.
                        KeyCode::KeyF if event.state == ElementState::Pressed => {
                            self.flying = !self.flying;
                            println!(
                                "[engine] modo vuelo: {}",
                                if self.flying { "ON" } else { "OFF" }
                            );
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
                // Click izquierdo: captura el cursor; si ya esta capturado, rompe.
                MouseButton::Left => {
                    if self.mouse_locked {
                        self.break_block();
                    } else {
                        self.lock_mouse();
                    }
                }
                // Click derecho: coloca (solo con el cursor capturado).
                MouseButton::Right if self.mouse_locked => self.place_block(),
                _ => {}
            },

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
                let dt = self
                    .last_frame
                    .map(|last| (now - last).as_secs_f32())
                    .unwrap_or(0.0);
                self.last_frame = Some(now);
                // Limitamos el dt: si el proceso se quedo parado (arrastrando
                // la ventana, breakpoint...) no queremos "teletransportarnos".
                let dt = dt.clamp(0.0, 0.1);

                self.update(dt);
                self.update_selection();

                // Dibujamos con la matriz de la camara actual (proyeccion * vista).
                if let (Some(renderer), Some(camera)) =
                    (self.renderer.as_mut(), self.camera.as_ref())
                {
                    let view_projection = camera.view_projection();
                    let position = camera.position;
                    renderer.update_streaming(position);
                    renderer.render(&view_projection);
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
}
