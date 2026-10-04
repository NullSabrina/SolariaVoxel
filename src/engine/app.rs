//! El bucle principal de la aplicacion.
//!
//! winit 0.30 usa el patron `ApplicationHandler`: nosotros implementamos una
//! serie de callbacks y winit nos avisa cuando pasa algo (la app arranca, llega
//! un evento de ventana, toca repintar...). No hay un `while` visible: el bucle
//! vive dentro de `event_loop.run_app`.
//!
//! Reparto de responsabilidades:
//! * [`App`] une las piezas: ventana + renderer + escena.
//! * `window` (modulo hermano) describe la ventana.
//! * [`Renderer`] dibuja. [`Camera`] dice desde donde miramos.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::engine::window;
use crate::math::Vec3;
use crate::render::Renderer;
use crate::scene::Camera;

/// Estado global de la aplicacion.
///
/// Los campos son `Option` porque en winit 0.30 la ventana (y con ella el
/// renderer) no existen hasta que llega el callback `resumed`.
#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Option<Camera>,
}

/// Arranca el motor: crea el bucle de eventos y lo ejecuta.
///
/// `ControlFlow::Poll` hace que el bucle no duerma esperando eventos, lo que
/// es lo que queremos para un juego que repinta continuamente. Para una app de
/// escritorio normalizariamos `ControlFlow::Wait`.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
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

        // El renderer necesita la ventana; le pasamos una copia del `Arc`.
        match Renderer::new(window.clone()) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                eprintln!("[engine] no se pudo iniciar el renderer: {e}");
                event_loop.exit();
                return;
            }
        }

        // Camara de v0.1.0: estatica en el origen, mirando al frente (-Z).
        let mut camera = Camera::new(Vec3::ZERO);
        let size = window.inner_size();
        camera.update_projection(size.width as f32 / size.height.max(1) as f32);
        camera.update_view();
        println!(
            "[engine] camara en {:?} | mira hacia {:?}",
            camera.position,
            camera.forward()
        );
        self.camera = Some(camera);

        self.window = Some(window);
    }

    /// Se llama por cada evento de la ventana (teclado, raton, resize...).
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

            // Escape tambien cierra (comodo durante el desarrollo).
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                {
                    event_loop.exit();
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
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.render();
                }
            }

            _ => {}
        }
    }

    /// Se llama cuando no quedan eventos pendientes. Aprovechamos para pedir
    /// otro repintado: asi tenemos animacion/refresh continuo.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
