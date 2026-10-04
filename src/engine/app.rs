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
    input: Input,
    /// ¿Tenemos el cursor capturado (pointer lock)?
    mouse_locked: bool,
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

    /// Un tick de simulacion: aplica el giro del raton y el desplazamiento WASD.
    fn update(&mut self, dt: f32) {
        // Leemos TODO el input primero, para no mezclar los prestamos de la
        // camara con los del input.
        let (dx, dy) = self.input.take_mouse_delta();
        let forward = self.input.forward_axis();
        let right = self.input.right_axis();
        let up = self.input.up_axis();

        let Some(camera) = self.camera.as_mut() else {
            return;
        };

        // Solo giramos si el cursor esta capturado (y nos movimos de verdad).
        if self.mouse_locked && (dx != 0.0 || dy != 0.0) {
            camera.add_look(dx, dy);
        }

        if forward != 0.0 || right != 0.0 || up != 0.0 {
            camera.walk(forward, right, up, dt);
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

        match Renderer::new(window.clone()) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[engine] no se pudo iniciar el renderer: {e}");
                event_loop.exit();
                return;
            }
        }

        // Camara FPS: elevada y cerca del borde para ver la rejilla 3x3.
        let mut camera = Camera::new(Vec3::new(0.0, 100.0, 30.0));
        camera.pitch_deg = -38.0;
        let size = window.inner_size();
        camera.update_projection(size.width as f32 / size.height.max(1) as f32);
        camera.update_view();
        self.camera = Some(camera);

        self.last_frame = Some(Instant::now());
        self.window = Some(window);

        println!("[engine] controles: click = capturar raton | WASD = andar");
        println!("[engine] Espacio/Shift = subir/bajar | Escape = liberar/salir");
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

                    // Escape: libera el raton; si ya esta libre, sale.
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        if self.mouse_locked {
                            self.unlock_mouse();
                        } else {
                            event_loop.exit();
                        }
                    }
                }
            }

            // Click izquierdo: captura el cursor si no lo estaba.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !self.mouse_locked {
                    self.lock_mouse();
                }
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
                let dt = self
                    .last_frame
                    .map(|last| (now - last).as_secs_f32())
                    .unwrap_or(0.0);
                self.last_frame = Some(now);
                // Limitamos el dt: si el proceso se quedo parado (arrastrando
                // la ventana, breakpoint...) no queremos "teletransportarnos".
                let dt = dt.clamp(0.0, 0.1);

                self.update(dt);

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
