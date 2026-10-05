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

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use winit::window::Window;

use crate::math::{Frustum, Mat4, Vec3};
use crate::render::color::srgb_to_linear;
use crate::render::highlight::{HighlightPipeline, cube_edges};
use crate::render::mesh::Mesh;
use crate::render::pipeline::ScenePipeline;
use crate::world::{
    Block, CHUNK_SIZE, ChunkPos, ChunkRecord, RayHit, SECTION_COUNT, StreamChange, WORLD_HEIGHT,
    World, greedy, raycast,
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
    sky_color_from_srgb([0.47, 0.71, 0.97])
}

/// Convierte un color de cielo sRGB (0..1) al `wgpu::Color` lineal del clear.
fn sky_color_from_srgb(c: [f32; 3]) -> wgpu::Color {
    wgpu::Color {
        r: srgb_to_linear(c[0]) as f64,
        g: srgb_to_linear(c[1]) as f64,
        b: srgb_to_linear(c[2]) as f64,
        a: 1.0,
    }
}

/// Mallas de una columna: una `Option<Mesh>` por seccion.
type ColumnMeshes = [Option<Mesh>; SECTION_COUNT];

/// Distancia (bloques) a la que empieza la niebla.
const FOG_START: f32 = 40.0;
/// Distancia (bloques) a la que la niebla es total. Coincide con el borde del
/// area cargada (~64 bloques en los ejes), asi que lo funde con el cielo.
const FOG_END: f32 = 64.0;

/// Presupuesto de meshing por frame, en milisegundos. Al descubrir chunks se
/// encolan las columnas y se meshean a este ritmo en lugar de todas de golpe: el
/// pico de ~40 ms por cruce se reparte entre varios frames y no hay tiron.
const MESH_BUDGET_MS: f32 = 6.0;

/// Columnas ya cargadas cuyas mallas hay que reconstruir tras un cambio de
/// streaming: las **colindantes** de cada columna cargada o descargada, en el
/// anillo 3x3 (incluidas diagonales: la luz de bloque viaja a columnas que tocan
/// la esquina). Se deduplica y se excluyen las propias columnas cargadas (ya se
/// acaban de meshear en `sync_streaming`).
fn columns_to_remesh(change: &StreamChange, is_loaded: impl Fn(ChunkPos) -> bool) -> Vec<ChunkPos> {
    let mut out: Vec<ChunkPos> = Vec::new();
    for pos in change.loaded.iter().chain(change.unloaded.iter()) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let n = ChunkPos::new(pos.x + dx, pos.z + dz);
                if is_loaded(n) && !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// Las 9 columnas del anillo 3x3 centrado en `center` (incluida el misma).
fn area3x3(center: ChunkPos) -> Vec<ChunkPos> {
    let mut out = Vec::with_capacity(9);
    for dz in -1..=1 {
        for dx in -1..=1 {
            out.push(ChunkPos::new(center.x + dx, center.z + dz));
        }
    }
    out
}

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
    /// Columnas pendientes de (re)meshear. Se van vaciando con un presupuesto de
    /// tiempo por frame para no dar tirones al descubrir chunks.
    mesh_queue: VecDeque<ChunkPos>,

    clear_color: wgpu::Color,
    /// Factor dia/noche (0..1) del ultimo frame, subido al shader.
    day_factor: f32,
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
            mesh_queue: VecDeque::new(),
            clear_color: sky_color(),
            day_factor: 1.0,
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
    ///
    /// Ojo con las **caras de borde**: al meshear una columna se mira el bloque
    /// del vecino, asi que el resultado depende de que columnas esten cargadas
    /// en ese momento. Si una columna se mesheo con su vecina ausente, al llegar
    /// la vecina conserva un **muro** (y oscuro, porque la luz de la celda de
    /// delante es 0); si la vecina se descarga, queda un **hueco**. Por eso, tras
    /// cargar/descargar, reconstruimos tambien las columnas **colindantes** ya
    /// mesheadas.
    pub fn sync_streaming(&mut self, player_pos: Vec3) {
        let change = self
            .world
            .update_streaming([player_pos.x, player_pos.y, player_pos.z]);
        if change.is_empty() {
            return;
        }
        // Columnas "sucias" para la luz: las que entran/salen y su anillo.
        let dirty = {
            let loaded = &self.world;
            let mut dirty: Vec<ChunkPos> = change.loaded.clone();
            for n in columns_to_remesh(&change, |p| loaded.is_loaded(p)) {
                if !dirty.contains(&n) {
                    dirty.push(n);
                }
            }
            dirty
        };
        // La luz puede haber cambiado (torches/cuevas que entran/salen): la
        // recomputamos (region afectada) antes de meshear.
        self.world.recompute_skylight(&dirty);
        self.world.recompute_block_light();
        // Liberar mallas de columnas descargadas (y sacarlas de la cola).
        for pos in &change.unloaded {
            self.meshes.remove(pos);
            self.mesh_queue.retain(|p| p != pos);
        }
        // Encolar las columnas nuevas y sus vecinas de borde; se meshean
        // repartidas entre frames (`pump_meshing`), empezando por las cercanas.
        let mut to_queue: Vec<ChunkPos> = change.loaded.clone();
        for n in columns_to_remesh(&change, |p| self.world.is_loaded(p)) {
            if !to_queue.contains(&n) {
                to_queue.push(n);
            }
        }
        let center = ChunkPos::new(
            (player_pos.x / CHUNK_SIZE as f32).floor() as i32,
            (player_pos.z / CHUNK_SIZE as f32).floor() as i32,
        );
        to_queue.sort_by_key(|p| (p.x - center.x).abs() + (p.z - center.z).abs());
        for pos in to_queue {
            self.queue_mesh(pos);
        }
        if !change.loaded.is_empty() || !change.unloaded.is_empty() {
            println!(
                "[world] streaming: +{} -{} columnas ({} cargadas, {} por meshear)",
                change.loaded.len(),
                change.unloaded.len(),
                self.world.loaded_positions().count(),
                self.mesh_queue.len(),
            );
        }
    }

    /// Encola una columna para (re)meshear, si esta cargada y no esta ya en cola.
    fn queue_mesh(&mut self, pos: ChunkPos) {
        if self.world.is_loaded(pos) && !self.mesh_queue.contains(&pos) {
            self.mesh_queue.push_back(pos);
        }
    }

    /// Meshea columnas de la cola hasta agotar un presupuesto de tiempo. Meshea
    /// **al menos una** para garantir progreso. Asi el coste de descubrir chunks
    /// se reparte entre frames y no produce un tiron.
    fn pump_meshing(&mut self, budget_ms: f32) {
        let start = Instant::now();
        while let Some(pos) = self.mesh_queue.pop_front() {
            if !self.world.is_loaded(pos) {
                continue;
            }
            let meshes = self.build_column_meshes(pos);
            self.meshes.insert(pos, Box::new(meshes));
            if start.elapsed().as_secs_f32() * 1000.0 >= budget_ms {
                break;
            }
        }
    }

    /// Construye las mallas de todas las secciones de una columna, consultando
    /// los **vecinos** (para no dibujar muros internos entre chunks).
    fn build_column_meshes(&self, pos: ChunkPos) -> ColumnMeshes {
        let origin = World::chunk_origin(pos);
        // Geometria ya en coordenadas de mundo.
        let base_x = pos.x * CHUNK_SIZE as i32;
        let base_z = pos.z * CHUNK_SIZE as i32;

        let Some(column) = self.world.column(pos) else {
            return std::array::from_fn(|_| None);
        };
        // La gran mayoria de consultas caen **dentro** de la columna (lectura
        // directa, sin HashMap ni `div_euclid`); solo los bordes (-1 / 16) miran
        // el chunk vecino. Era el otro gran coste de meshear.
        let in_col = |x: i32, y: i32, z: i32| {
            (0..CHUNK_SIZE as i32).contains(&x)
                && (0..CHUNK_SIZE as i32).contains(&z)
                && (0..WORLD_HEIGHT as i32).contains(&y)
        };
        let query = |x: i32, y: i32, z: i32| -> Block {
            if in_col(x, y, z) {
                column.get_or_air(x, y, z)
            } else {
                self.world.get_block([base_x + x, y, base_z + z])
            }
        };
        let light = |x: i32, y: i32, z: i32| -> (u8, u8) {
            if in_col(x, y, z) {
                (
                    column.light_or_zero(x, y, z),
                    column.block_light_or_zero(x, y, z),
                )
            } else {
                let w = [base_x + x, y, base_z + z];
                (self.world.sky_light_at(w), self.world.block_light_at(w))
            }
        };

        let mut out: ColumnMeshes = std::array::from_fn(|_| None);
        for (section, slot) in out.iter_mut().enumerate() {
            // Las secciones sin nada que dibujar no generan geometria; saltarlas
            // evita 24 pasadas de greedy por columna (y hace barato el re-mesheo
            // de vecinas del streaming).
            if self.world.section_is_empty(pos, section) {
                continue;
            }
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

    /// Reconstruye la malla de una columna cargada y la de su anillo **3x3**
    /// (incluidas diagonales). Hace falta tras editar porque la luz de bloque
    /// viaja hasta 15 bloques y puede cambiar caras de columnas vecinas en
    /// diagonal; y tras cargar/descargar porque cambian las caras de borde.
    fn refresh_area(&mut self, center: ChunkPos) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let n = ChunkPos::new(center.x + dx, center.z + dz);
                if self.world.is_loaded(n) {
                    let m = self.build_column_meshes(n);
                    self.meshes.insert(n, Box::new(m));
                }
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
        // La luz cambio (cielo y bloque): recomputamos la region afectada y
        // re-mesheamos el area.
        let (pos, _) = World::world_to_local(voxel);
        self.world.recompute_skylight(&area3x3(pos));
        self.world.recompute_block_light();
        self.refresh_area(pos);
        true
    }

    /// Aplica **muchos** cambios de bloque y regenera las mallas afectadas una
    /// sola vez (en lugar de una vez por bloque, como `set_block`). Es lo que
    /// usa la escena demo para construir rapido. Devuelve cuantos se aplicaron.
    pub fn set_blocks(&mut self, edits: &[([i32; 3], Block)]) -> usize {
        let mut touched: Vec<ChunkPos> = Vec::new();
        let mut applied = 0;
        for &(voxel, block) in edits {
            if self.world.set_block(voxel, block) {
                applied += 1;
                let (pos, _) = World::world_to_local(voxel);
                touched.push(pos);
            }
        }
        // Reconstruimos el area 3x3 de cada columna tocada, sin repetir; ese
        // mismo conjunto son las columnas "sucias" para la luz.
        let mut to_remesh: Vec<ChunkPos> = Vec::new();
        for pos in touched {
            for n in area3x3(pos) {
                if !to_remesh.contains(&n) {
                    to_remesh.push(n);
                }
            }
        }
        self.world.recompute_skylight(&to_remesh);
        self.world.recompute_block_light();
        for n in to_remesh {
            if self.world.is_loaded(n) {
                let m = self.build_column_meshes(n);
                self.meshes.insert(n, Box::new(m));
            }
        }
        applied
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

    /// Actualiza el entorno visual del frame: factor dia/noche y color de cielo
    /// (sRGB, canales 0..1). El color se convierte a lineal para el clear.
    pub fn set_environment(&mut self, day_factor: f32, sky_color: [f32; 3]) {
        self.day_factor = day_factor.clamp(0.0, 1.0);
        self.clear_color = sky_color_from_srgb(sky_color);
    }

    /// Dibuja y presenta un frame.
    pub fn render(&mut self, view_projection: &Mat4, camera_pos: Vec3) {
        // Antes de dibujar, avanzamos el meshing pendiente con un presupuesto de
        // tiempo para no dar tirones al descubrir chunks.
        self.pump_meshing(MESH_BUDGET_MS);

        let fog_color = [
            self.clear_color.r as f32,
            self.clear_color.g as f32,
            self.clear_color.b as f32,
        ];
        self.pipeline.update_uniforms(
            &self.queue,
            view_projection,
            [camera_pos.x, camera_pos.y, camera_pos.z],
            self.day_factor,
            fog_color,
            FOG_START,
            FOG_END,
        );

        // Frustum de la camara: descartamos las secciones fuera de la vista sin
        // siquiera emitir su draw call.
        let frustum = Frustum::from_view_projection(view_projection);

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
            let section = CHUNK_SIZE as f32;
            for (pos, column) in &self.meshes {
                let (wx, wz) = (
                    (pos.x * CHUNK_SIZE as i32) as f32,
                    (pos.z * CHUNK_SIZE as i32) as f32,
                );
                for (index, mesh) in column.iter().enumerate() {
                    let Some(mesh) = mesh else {
                        continue;
                    };
                    let y0 = index as f32 * section;
                    let min = [wx, y0, wz];
                    let max = [wx + section, y0 + section, wz + section];
                    if frustum.intersects_aabb(min, max) {
                        mesh.draw(&mut pass);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn columnas_a_remeshear_son_las_vecinas_cargadas() {
        let change = StreamChange {
            loaded: vec![ChunkPos::new(1, 0)],
            unloaded: vec![ChunkPos::new(5, 0)],
        };
        let loaded: HashSet<ChunkPos> = [
            ChunkPos::new(1, 0), // la cargada (no debe re-meshearse aparte)
            ChunkPos::new(0, 0), // vecina de la cargada
            ChunkPos::new(2, 0), // vecina de la cargada
            ChunkPos::new(6, 0), // vecina de la descargada
        ]
        .into_iter()
        .collect();

        let out = columns_to_remesh(&change, |p| loaded.contains(&p));

        assert!(out.contains(&ChunkPos::new(0, 0)));
        assert!(out.contains(&ChunkPos::new(2, 0)));
        assert!(out.contains(&ChunkPos::new(6, 0)));
        // La propia columna cargada ya se meshea en el bucle de `loaded`.
        assert!(!out.contains(&ChunkPos::new(1, 0)));
        // Sin duplicados.
        let unique: HashSet<ChunkPos> = out.iter().copied().collect();
        assert_eq!(out.len(), unique.len());
    }
}
