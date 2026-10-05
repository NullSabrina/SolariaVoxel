//! El [`Renderer`]: duena de todos los recursos de GPU.
//!
//! Desde v0.5.1 el mundo vive en [`crate::world::World`] (columnas en memoria
//! con cache) y el renderer mantiene una **malla por (columna, seccion)**. Al
//! hacer streaming solo se construyen/liberan las mallas de las columnas que
//! entran o salen, en lugar de regenerar todo.
//!
//! Flujo de un frame:
//! 1. `world.update_streaming(...)` -> carga/descarga columnas; si hay cambios,
//!    el renderer reconstruye solo las mallas afectadas.
//! 2. Actualizar uniforms (matriz de la camara).
//! 3. Adquirir textura, render pass (limpiar + dibujar mallas + resaltado) y
//!    presentar.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use winit::window::Window;

use crate::math::{Mat4, Vec3};
use crate::render::color::srgb_to_linear;
use crate::render::highlight::{HighlightPipeline, cube_edges};
use crate::render::mesh::Mesh;
use crate::render::pipeline::ScenePipeline;
use crate::world::{
    Block, CHUNK_SIZE, ChunkPos, ChunkRecord, RayHit, SECTION_COUNT, World, greedy, raycast,
};

/// Errores que pueden ocurrir al inicializar el renderer.
#[derive(Debug)]
pub enum RendererError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
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

/// Mallas de una columna: una `Option<Mesh>` por seccion.
type ColumnMeshes = [Option<Mesh>; SECTION_COUNT];

/// Todos los recursos de GPU viven aqui.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    pipeline: ScenePipeline,
    highlight_pipeline: HighlightPipeline,
    highlight_mesh: Option<Mesh>,

    /// El mundo en memoria.
    world: World,
    /// Mallas por columna: se recrean al entrar/salir columnas del radio.
    meshes: HashMap<ChunkPos, Box<ColumnMeshes>>,

    clear_color: wgpu::Color,
}

impl Renderer {
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Inicializa wgpu y el mundo. `restored` son los chunks cargados de disco.
    pub fn new(
        window: Arc<Window>,
        seed: u32,
        restored: Vec<(ChunkPos, ChunkRecord)>,
    ) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(RendererError::Surface)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(RendererError::Adapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("solaria.device"),
            ..Default::default()
        }))
        .map_err(RendererError::Device)?;

        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(RendererError::NoSurfaceConfig)?;
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

        let (depth_texture, depth_view) = Self::create_depth(&device, &config);
        let pipeline = ScenePipeline::new(&device, &queue, config.format, Self::DEPTH_FORMAT);
        let highlight_pipeline = HighlightPipeline::new(
            &device,
            pipeline.layout(),
            config.format,
            Self::DEPTH_FORMAT,
        );

        // Mundo con radio 4 (9x9 = 81 columnas), con los chunks restaurados.
        let view_radius = 4;
        let restored_count = restored.len();
        let world = World::new(seed, view_radius, restored);
        if restored_count > 0 {
            println!("[world] {restored_count} chunks restaurados de disco");
        }

        let info = adapter.get_info();
        println!("[render] GPU: {} | backend: {:?}", info.name, info.backend);

        let mut renderer = Self {
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            pipeline,
            highlight_pipeline,
            highlight_mesh: None,
            world,
            meshes: HashMap::new(),
            clear_color: sky_color(),
        };
        // Carga inicial del mundo alrededor del origen.
        renderer.sync_streaming(Vec3::new(0.0, 64.0, 0.0));
        Ok(renderer)
    }

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

    /// Ajusta la superficie y el z-buffer al nuevo tamano de ventana.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let (t, v) = Self::create_depth(&self.device, &self.config);
        self.depth_texture = t;
        self.depth_view = v;
    }

    /// Actualiza el streaming del mundo y sincroniza las mallas: libera las de
    /// las columnas descargadas y construye las de las nuevas.
    pub fn sync_streaming(&mut self, player_pos: Vec3) {
        let change = self
            .world
            .update_streaming([player_pos.x, player_pos.y, player_pos.z]);
        if change.is_empty() {
            return;
        }
        // Liberar mallas de columnas descargadas.
        for pos in &change.unloaded {
            self.meshes.remove(pos);
        }
        // Construir mallas de columnas nuevas.
        for pos in &change.loaded {
            let meshes = self.build_column_meshes(*pos);
            self.meshes.insert(*pos, Box::new(meshes));
        }
        if !change.loaded.is_empty() || !change.unloaded.is_empty() {
            println!(
                "[world] streaming: +{} -{} columnas ({} cargadas)",
                change.loaded.len(),
                change.unloaded.len(),
                self.meshes.len()
            );
        }
    }

    /// Construye las mallas de todas las secciones de una columna, consultando
    /// los **vecinos** (para no dibujar muros internos entre chunks).
    fn build_column_meshes(&self, pos: ChunkPos) -> ColumnMeshes {
        let origin = World::chunk_origin(pos);
        // Geometria ya en coordenadas de mundo.
        let base_x = pos.x * CHUNK_SIZE as i32;
        let base_z = pos.z * CHUNK_SIZE as i32;

        // Consulta de bloque: recibe coordenadas **locales** de la columna (que
        // pueden salirse a -1 o 16) y devuelve el bloque del mundo en ese punto,
        // mirando la columna vecina si hace falta.
        let query =
            |x: i32, y: i32, z: i32| -> Block { self.world.get_block([base_x + x, y, base_z + z]) };
        let light =
            |x: i32, y: i32, z: i32| -> u8 { self.world.light_at([base_x + x, y, base_z + z]) };

        let mut out: ColumnMeshes = std::array::from_fn(|_| None);
        for (section, slot) in out.iter_mut().enumerate() {
            let (v, i) = greedy::greedy_section_query(&query, &light, section, origin);
            if !v.is_empty() {
                *slot = Some(Mesh::new(
                    &self.device,
                    &format!("col_{}_{}_sec_{section}", pos.x, pos.z),
                    &v,
                    &i,
                ));
            }
        }
        out
    }

    /// Reconstruye las mallas de la columna afectada (y sus vecinas, porque la
    /// cara del borde cambia) tras editar un bloque.
    fn refresh_around(&mut self, pos: ChunkPos) {
        let neighbors = [
            pos,
            ChunkPos::new(pos.x + 1, pos.z),
            ChunkPos::new(pos.x - 1, pos.z),
            ChunkPos::new(pos.x, pos.z + 1),
            ChunkPos::new(pos.x, pos.z - 1),
        ];
        for n in neighbors {
            if self.world.is_loaded(n) {
                let m = self.build_column_meshes(n);
                self.meshes.insert(n, Box::new(m));
            }
        }
    }

    /// ¿Hay bloque solido en este punto del mundo (coordenadas en bloques)?
    pub fn is_solid_at(&self, point: Vec3) -> bool {
        self.world.is_solid([
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        ])
    }

    /// Lanza un rayo y devuelve el primer bloque **golpeable** (solido, o visible
    /// no solido como la antorcha) de cualquier columna cargada.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<RayHit> {
        let is_hit = |x: i32, y: i32, z: i32| -> bool {
            let block = self.world.get_block([x, y, z]);
            block.is_solid() || block.is_visible()
        };
        raycast(origin, dir, max_distance, is_hit)
    }

    /// Cambia un bloque (coordenadas de mundo) y regenera las mallas afectadas.
    pub fn set_block(&mut self, voxel: [i32; 3], block: Block) -> bool {
        if !self.world.set_block(voxel, block) {
            return false;
        }
        let (pos, _) = World::world_to_local(voxel);
        self.refresh_around(pos);
        true
    }

    /// Actualiza el wireframe del bloque apuntado.
    pub fn set_highlight(&mut self, hit: Option<RayHit>) {
        self.highlight_mesh = hit.map(|h| {
            let (v, i) = cube_edges(
                h.block[0] as f32 + 0.5,
                h.block[1] as f32 + 0.5,
                h.block[2] as f32 + 0.5,
                0.002,
            );
            Mesh::new(&self.device, "highlight", &v, &i)
        });
    }

    /// Volca TODAS las columnas modificadas, para guardar el mundo.
    pub fn snapshot_modified(&self) -> Vec<(ChunkPos, ChunkRecord)> {
        self.world
            .modified_records()
            .iter()
            .map(|(pos, rec)| (*pos, rec.clone()))
            .collect()
    }

    /// Semilla del mundo.
    pub fn seed(&self) -> u32 {
        self.world.seed()
    }

    /// Dibuja y presenta un frame.
    pub fn render(&mut self, view_projection: &Mat4) {
        self.pipeline.update_mvp(&self.queue, view_projection);

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
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("solaria.encoder"),
            });

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
            for column in self.meshes.values() {
                for mesh in column.iter().flatten() {
                    mesh.draw(&mut pass);
                }
            }

            if let Some(highlight) = self.highlight_mesh.as_ref() {
                pass.set_pipeline(self.highlight_pipeline.pipeline());
                highlight.draw(&mut pass);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
    }
}
