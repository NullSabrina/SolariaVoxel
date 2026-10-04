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

use crate::math::Mat4;
use crate::render::color::srgb_to_linear;
use crate::render::mesh::Mesh;
use crate::render::pipeline::ScenePipeline;
use crate::world::{Column, TerrainGenerator, mesh_column};

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

/// Resultado de construir la geometria de una rejilla de columnas.
struct ColumnMeshes {
    meshes: Vec<Mesh>,
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

    /// El pipeline de dibujo y una malla por seccion con geometria.
    pipeline: ScenePipeline,
    meshes: Vec<Mesh>,

    /// Semilla del mundo (para regenerar el terreno al hacer streaming).
    seed: u32,
    /// Distancia de carga en chunks, en cada direccion (1 = rejilla 3x3).
    view_radius: i32,
    /// Centro de la ultima rejilla generada, en coordenadas de chunk.
    loaded_center: (i32, i32),
    /// La columna del chunk central, para consultar bloques (colisiones,
    /// raycast...). Las columnas vecinas no se guardan en v0.3.2.
    center_column: Column,

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

        // 6. Z-buffer, pipeline y mundo.
        let (depth_texture, depth_view) = Self::create_depth(&device, &config);
        let pipeline = ScenePipeline::new(&device, &queue, config.format, Self::DEPTH_FORMAT);

        let seed = 13_371;
        let view_radius = 3; // 3 => rejilla 7x7 de columnas
        let meshes = Self::build_column_meshes(&device, seed, (0, 0), view_radius);
        // Guardamos la columna central para las consultas de bloques.
        let center_column = TerrainGenerator::new(seed).generate_column(0, 0);

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
            meshes: meshes.meshes,
            seed,
            view_radius,
            loaded_center: (0, 0),
            center_column,
            clear_color: sky_color(),
        })
    }

    /// Genera la geometria de todas las columnas de una rejilla centrada en
    /// `center` (coordenadas de chunk), con radio `radius` (1 => 3x3).
    fn build_column_meshes(
        device: &wgpu::Device,
        seed: u32,
        center: (i32, i32),
        radius: i32,
    ) -> ColumnMeshes {
        let generator = TerrainGenerator::new(seed);
        let mut meshes = Vec::new();
        let mut triangles = 0usize;
        let mut columns = 0usize;

        for cz in (center.1 - radius)..=(center.1 + radius) {
            for cx in (center.0 - radius)..=(center.0 + radius) {
                let world_x = cx * crate::world::CHUNK_SIZE as i32;
                let world_z = cz * crate::world::CHUNK_SIZE as i32;
                let column = generator.generate_column(world_x, world_z);
                // Las posiciones del mesher son locales (0..16); colocamos la
                // columna en su sitio del mundo con `origin`.
                let origin = [world_x as f32, 0.0, world_z as f32];
                let sections = mesh_column(&column, origin);
                columns += 1;
                for s in sections {
                    triangles += s.indices.len() / 3;
                    meshes.push(Mesh::new(
                        device,
                        &format!("col_{cx}_{cz}_sec_{}", s.section),
                        &s.vertices,
                        &s.indices,
                    ));
                }
            }
        }

        println!(
            "[world] rejilla {}x{} (semilla {seed}): {columns} columnas, {triangles} triangulos",
            radius * 2 + 1,
            radius * 2 + 1,
        );
        ColumnMeshes { meshes }
    }

    /// Recarga el mundo si el jugador ha cruzado a otra columna de chunks.
    /// (v0.3.1: recarga toda la rejilla de golpe.)
    pub fn update_streaming(&mut self, player: crate::math::Vec3) {
        let cs = crate::world::CHUNK_SIZE as f32;
        let chunk = (
            (player.x / cs).floor() as i32,
            (player.z / cs).floor() as i32,
        );
        if chunk == self.loaded_center {
            return;
        }
        self.loaded_center = chunk;
        // Libera las mallas antiguas (se destruyen al soltar el Vec) y genera
        // las nuevas. Sin cache todavia: se regeneran todas.
        self.meshes.clear();
        let built = Self::build_column_meshes(&self.device, self.seed, chunk, self.view_radius);
        self.meshes = built.meshes;
        // Actualiza la columna central (la que usan las colisiones).
        let cs = crate::world::CHUNK_SIZE as i32;
        self.center_column =
            TerrainGenerator::new(self.seed).generate_column(chunk.0 * cs, chunk.1 * cs);
        println!("[world] streaming -> centro de chunk {chunk:?}");
    }

    /// Devuelve el bloque en un punto del **chunk central**, o `None` si esta
    /// fuera de esa columna. Es la base para la deteccion de suelo.
    pub fn block_at(&self, world: crate::math::Vec3) -> Option<crate::world::Block> {
        if world.y < 0.0 {
            return None;
        }
        // Coordenadas locales dentro del chunk central (0..16).
        let local_x = world.x - (self.loaded_center.0 * crate::world::CHUNK_SIZE as i32) as f32;
        let local_z = world.z - (self.loaded_center.1 * crate::world::CHUNK_SIZE as i32) as f32;
        if local_x < 0.0
            || local_z < 0.0
            || local_x >= crate::world::CHUNK_SIZE as f32
            || local_z >= crate::world::CHUNK_SIZE as f32
        {
            return None;
        }
        let xi = local_x as usize;
        let zi = local_z as usize;
        let yi = world.y as usize;
        if yi >= crate::world::WORLD_HEIGHT {
            return None;
        }
        Some(self.center_column.get(xi, yi, zi))
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
        // 1. Uniforms: la matriz de la camara (el mundo ya esta en coordenadas
        //    de mundo, no hace falta modelo por objeto).
        self.pipeline.update_mvp(&self.queue, view_projection);

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
            // Una llamada de dibujo por seccion no vacia.
            for mesh in &self.meshes {
                mesh.draw(&mut pass);
            }
        }

        // 6 y 7. Enviar y presentar.
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
    }
}
