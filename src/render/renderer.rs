//! El [`Renderer`]: duena de todos los recursos de GPU.
//!
//! Flujo de un frame en wgpu (el mismo que seguiremos siempre, cada vez con
//! mas pasos entre medias):
//!
//! 1. Actualizar los uniforms (la matriz MVP de la camara).
//! 2. `surface.get_current_texture()` -> pide a la ventana la textura del frame.
//! 3. `texture.create_view()` -> una "vista" sobre esa textura.
//! 4. `device.create_command_encoder()` -> un cuaderno de ordenes.
//! 5. `encoder.begin_render_pass(...)` -> limpiamos color y profundidad y
//!    dibujamos el cubo.
//! 6. `queue.submit(...)` -> la GPU ejecuta las ordenes.
//! 7. `queue.present(frame)` -> se muestra el frame en la ventana.

use std::fmt;
use std::sync::Arc;

use winit::window::Window;

use crate::math::{Mat4, Vec3};
use crate::render::color::srgb_to_linear;
use crate::render::mesh::Mesh;
use crate::render::pipeline::ScenePipeline;

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

/// Color de cielo por defecto, expresado en sRGB y convertido a lineal.
fn sky_color() -> wgpu::Color {
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
    /// La superficie sobre la que presentamos (atada a la ventana).
    surface: wgpu::Surface<'static>,
    /// El dispositivo logico: la "GPU virtual" con la que creamos recursos.
    device: wgpu::Device,
    /// La cola: por donde se envian los comandos y se presentan los frames.
    queue: wgpu::Queue,
    /// Configuracion de la superficie (tamano, formato, modo de presentacion).
    config: wgpu::SurfaceConfiguration,

    /// Z-buffer: guarda la profundidad de cada pixel para que lo de delante
    /// tape a lo de detras.
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    /// El pipeline de dibujo y la malla del cubo.
    pipeline: ScenePipeline,
    mesh: Mesh,
    /// Transformacion del cubo en el mundo (posicion + rotacion fija).
    model: Mat4,

    /// Color con el que limpiamos el color buffer cada frame.
    clear_color: wgpu::Color,
}

impl Renderer {
    /// Formato del z-buffer. `Depth32Float` es el estandar y esta en todas partes.
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Inicializa wgpu sobre `window`.
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let size = window.inner_size();

        // 1. El "Instance" es el punto de entrada a wgpu: enumera backends.
        let instance = wgpu::Instance::default();

        // 2. La superficie conecta wgpu con la ventana.
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

        // 5. Configuramos la superficie.
        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(RendererError::NoSurfaceConfig)?;

        // Preferimos un formato sRGB si esta disponible.
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

        // 6. Z-buffer, pipeline y malla.
        let (depth_texture, depth_view) = Self::create_depth(&device, &config);
        let pipeline = ScenePipeline::new(&device, config.format, Self::DEPTH_FORMAT);
        let mesh = Mesh::cube(&device, 1.0);

        // 7. Colocamos el cubo delante de la camara y un poco rotado para que
        //    se vean tres caras (y asi se aprecia que es 3D de verdad).
        let model = Mat4::translation(Vec3::new(0.0, 0.0, -6.0))
            * Mat4::rotation_y(0.6)
            * Mat4::rotation_x(-0.5);

        let info = adapter.get_info();
        println!("[render] GPU: {} | backend: {:?}", info.name, info.backend);
        println!(
            "[render] superficie: {}x{} | formato: {:?}",
            config.width, config.height, config.format
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            pipeline,
            mesh,
            model,
            clear_color: sky_color(),
        })
    }

    /// Crea (o recrea) la textura de profundidad para el tamano actual.
    fn create_depth(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solaria.depth"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Ajusta la superficie y el z-buffer al nuevo tamano de la ventana.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) = Self::create_depth(&self.device, &self.config);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
    }

    /// Dibuja y presenta un frame. `view_projection` es la matriz de la camara
    /// (proyeccion * vista); el renderer le aplica la transformacion del cubo.
    pub fn render(&mut self, view_projection: &Mat4) {
        // 1. Uniforms: modelo * vista * proyeccion.
        self.pipeline
            .update_mvp(&self.queue, &(*view_projection * self.model));

        // 2. Pedir la textura del frame.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("[render] error de validacion al adquirir el frame");
                return;
            }
        };

        // 3. Vista sobre la textura de color.
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // 4. Cuaderno de ordenes.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("solaria.encoder"),
            });

        // 5. Render pass: limpiar color + profundidad y dibujar el cubo.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("solaria.scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        // 1.0 = profundidad maxima (el fondo); la geometria
                        // escribe valores mas pequenos y por eso la vemos.
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(self.pipeline.pipeline());
            pass.set_bind_group(0, self.pipeline.bind_group(), &[]);
            self.mesh.draw(&mut pass);
        }

        // 6 y 7. Enviar y presentar.
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
    }
}
