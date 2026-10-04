//! El [`Renderer`]: duena de todos los recursos de GPU.
//!
//! Flujo de un frame en wgpu (el mismo que seguiremos siempre, cada vez con
//! mas pasos entre medias):
//!
//! 1. `surface.get_current_texture()` -> pide a la ventana la textura del frame.
//! 2. `texture.create_view()` -> una "vista" sobre esa textura para poder
//!    usarla como destino de dibujo.
//! 3. `device.create_command_encoder()` -> un cuaderno de ordenes.
//! 4. `encoder.begin_render_pass(...)` -> en v0.1.0 el unico paso limpia.
//! 5. `queue.submit(...)` -> la GPU ejecuta las ordenes.
//! 6. `queue.present(frame)` -> se muestra el frame en la ventana.

use std::fmt;
use std::sync::Arc;

use winit::window::Window;

/// Errores que pueden ocurrir al inicializar el renderer.
#[derive(Debug)]
pub enum RendererError {
    /// No se pudo crear la superficie a partir de la ventana.
    Surface(wgpu::CreateSurfaceError),
    /// No se encontro una GPU compatible con la superficie.
    Adapter(wgpu::RequestAdapterError),
    /// La GPU no pudo crear el dispositivo logico.
    Device(wgpu::RequestDeviceError),
    /// La superficie no ofrece ninguna configuracion valida.
    NoSurfaceConfig,
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::Surface(e) => write!(f, "no se pudo crear la superficie: {e}"),
            RendererError::Adapter(e) => write!(f, "no se encontro adaptador de GPU: {e}"),
            RendererError::Device(e) => write!(f, "no se pudo crear el dispositivo: {e}"),
            RendererError::NoSurfaceConfig => {
                write!(f, "la superficie no ofrece ninguna configuracion valida")
            }
        }
    }
}

impl std::error::Error for RendererError {}

/// Convierte un canal de color de sRGB (como el que elegirias en un editor de
/// imagenes) al espacio lineal que espera la GPU antes de aplicar la correccion
/// de gamma. Formula estandar de la norma sRGB.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Color de cielo por defecto, expresado en sRGB y convertido a lineal.
fn sky_color() -> wgpu::Color {
    // Azul cielo en sRGB 0..1 (como se ve en pantalla).
    let (r, g, b) = (0.47, 0.71, 0.97);
    wgpu::Color {
        r: srgb_to_linear(r) as f64,
        g: srgb_to_linear(g) as f64,
        b: srgb_to_linear(b) as f64,
        a: 1.0,
    }
}

/// Todos los recursos de GPU viven aqui.
pub struct Renderer {
    /// La superficie sobre la que presentamos (esta atada a la ventana).
    surface: wgpu::Surface<'static>,
    /// El dispositivo logico: la "GPU virtual" con la que creamos recursos.
    device: wgpu::Device,
    /// La cola: por donde se envian los comandos y se presentan los frames.
    queue: wgpu::Queue,
    /// Configuracion de la superficie (tamano, formato, modo de presentacion).
    config: wgpu::SurfaceConfiguration,
    /// Color con el que limpiamos cada frame.
    clear_color: wgpu::Color,
}

impl Renderer {
    /// Inicializa wgpu sobre `window`.
    ///
    /// Los `Future` de wgpu (pedir adaptador y dispositivo) se ejecutan de forma
    /// bloqueante con `pollster`, porque en v0.1.0 no tenemos runtime async.
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let size = window.inner_size();

        // 1. El "Instance" es el punto de entrada a wgpu: enumera backends.
        let instance = wgpu::Instance::default();

        // 2. La superficie conecta wgpu con la ventana. Pasamos `window.clone()`
        //    (un `Arc<Window>`), que wgpu acepta como handle de ventana propio.
        let surface = instance
            .create_surface(window)
            .map_err(RendererError::Surface)?;

        // 3. Elegimos un adaptador fisico (la GPU real).
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(RendererError::Adapter)?;

        // 4. Creamos el dispositivo logico y su cola.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("solaria.device"),
            ..Default::default()
        }))
        .map_err(RendererError::Device)?;

        // 5. Configuramos la superficie. `get_default_config` elige un formato
        //    y un modo de presentacion sensatos para esta GPU/ventana.
        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(RendererError::NoSurfaceConfig)?;

        // Preferimos un formato sRGB si esta disponible: asi los colores que
        // calculamos se ven como esperamos, sin lavados ni oscurecidos.
        if let Some(srgb) = surface
            .get_capabilities(&adapter)
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
        {
            config.format = srgb;
        }

        surface.configure(&device, &config);

        let info = adapter.get_info();
        println!("[render] GPU: {} | backend: {:?}", info.name, info.backend);
        println!(
            "[render] superficie: {}x{} | formato: {:?} | present mode: {:?}",
            config.width, config.height, config.format, config.present_mode
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            clear_color: sky_color(),
        })
    }

    /// Ajusta la superficie al nuevo tamano de la ventana.
    ///
    /// Ignoramos los tamanos nulos (ventana minimizada) porque configurar una
    /// superficie de 0x0 es un error de validacion en wgpu.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Dibuja y presenta un frame.
    pub fn render(&mut self) {
        // Paso 1: pedir la textura del frame. El resultado no es un simple
        // Result: hay varios estados que el sistema nos puede devolver.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            // La configuracion cambio (p.ej. la ventana se movio de monitor):
            // reconfiguramos y saltamos este frame.
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            // Sin frame disponible ahora mismo: no es un error, reintentamos.
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("[render] error de validacion al adquirir el frame");
                return;
            }
        };

        // Paso 2: una vista sobre la textura, para usarla como color target.
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Paso 3: el cuaderno de ordenes de este frame.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("solaria.encoder"),
            });

        // Paso 4: un render pass. En v0.1.0 solo limpiamos el color target.
        // El pass se cierra solo al salir de este bloque (`Drop`).
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("solaria.clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // Paso 5 y 6: enviar y presentar.
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
    }
}
