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
use crate::render::gui;
use crate::render::highlight::{HighlightPipeline, cube_edges};
use crate::render::mesh::Mesh;
use crate::render::mesh_worker::{MeshJob, MeshOutput, MeshScheduler};
use crate::render::pipeline::ScenePipeline;
use crate::render::ui::{UiPipeline, UiQuad};
use crate::world::mesh_snapshot::section_snapshot;
use crate::world::{
    Block, CHUNK_SIZE, ChunkPos, ChunkRecord, RayHit, SECTION_COUNT, StreamChange, World, raycast,
};

/// Reutiliza la malla `slot` con la nueva geometria (o la crea si falta). Los
/// buffers GPU se reutilizan mientras quepan (ver [`Mesh::update`]), evitando
/// crear/destruir buffers en cada re-mesheo.
fn update_mesh(
    slot: &mut Option<Mesh>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vertices: &[crate::render::mesh::Vertex],
    indices: &[u32],
    label: &str,
) {
    match slot {
        Some(mesh) => mesh.update(device, queue, vertices, indices),
        None => {
            if !vertices.is_empty() {
                *slot = Some(Mesh::new(device, label, vertices, indices));
            }
        }
    }
}

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

/// Mallas de una seccion: la **opaca** y la de **agua** (translucida), separadas
/// porque el agua se dibuja en un pase con blending y sin escritura de z.
#[derive(Default)]
struct SectionMeshes {
    opaque: Option<Mesh>,
    water: Option<Mesh>,
}

/// Mallas de una columna: una `SectionMeshes` por seccion.
type ColumnMeshes = [SectionMeshes; SECTION_COUNT];

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
    /// Bloque del ultimo resaltado, para no recrear la malla GPU si el jugador
    /// sigue apuntando al mismo bloque (se recreaba cada frame).
    highlight_hit: Option<[i32; 3]>,

    /// Pipeline de la interfaz 2D (hotbar/inventario) y su textura.
    ui: UiPipeline,
    _gui_texture: wgpu::Texture,

    /// El mundo en memoria.
    world: World,
    /// Mallas por columna: se recrean al entrar/salir columnas del radio.
    meshes: HashMap<ChunkPos, Box<ColumnMeshes>>,
    /// Secciones pendientes de (re)meshear `(columna, seccion)`. Se van vaciando
    /// con un presupuesto de tiempo por frame para no dar tirones.
    mesh_queue: VecDeque<(ChunkPos, usize)>,
    /// Pool de meshing CPU en hilos de trabajo.
    mesh_scheduler: MeshScheduler,
    /// Revision por seccion: descarta resultados de meshing obsoletos.
    mesh_rev: HashMap<(ChunkPos, usize), u64>,

    clear_color: wgpu::Color,
    /// Factor dia/noche (0..1) del ultimo frame, subido al shader.
    day_factor: f32,
    /// Instante de arranque: da el tiempo que anima la superficie del agua.
    start: Instant,
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
        // Textura de interfaz + su pipeline (comparte la vista del atlas).
        let (gui_texture, gui_view) = Self::create_gui_texture(&device, &queue);
        let ui = UiPipeline::new(
            &device,
            pipeline.atlas_view(),
            &gui_view,
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
            highlight_hit: None,
            ui,
            _gui_texture: gui_texture,
            world,
            meshes: HashMap::new(),
            mesh_queue: VecDeque::new(),
            mesh_scheduler: MeshScheduler::new(2),
            mesh_rev: HashMap::new(),
            clear_color: sky_color(),
            day_factor: 1.0,
            start: Instant::now(),
        };
        // Carga inicial **sincrona** del mundo alrededor del origen. El app
        // vuelve a calentar en la posicion real del jugador antes de posarlo.
        renderer.warm_streaming(Vec3::new(0.0, 64.0, 0.0));
        Ok(renderer)
    }

    /// Crea la textura de la interfaz (hotbar/inventario) a partir de [`gui`].
    fn create_gui_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: gui::GUI_W,
            height: gui::GUI_H,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solaria.gui"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let pixels = gui::load_pixels();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(gui::GUI_W * 4),
                rows_per_image: Some(gui::GUI_H),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
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
        let mut change = self
            .world
            .plan_streaming([player_pos.x, player_pos.y, player_pos.z]);
        // Columnas generadas por los workers que ya estan listas este frame.
        change.loaded.extend(self.world.poll_generation());
        self.apply_stream_change(&change, player_pos);
    }

    /// Carga **sincrona** inicial del area del jugador (arranque). Garantiza que
    /// hay terreno antes de posar al jugador o montar la demo; a partir de ahi
    /// el streaming normal es asincrono.
    pub fn warm_streaming(&mut self, player_pos: Vec3) {
        let change = self
            .world
            .warm_streaming([player_pos.x, player_pos.y, player_pos.z]);
        self.apply_stream_change(&change, player_pos);
    }

    /// Aplica un cambio de streaming: ilumina, libera/encola mallas y registra.
    fn apply_stream_change(&mut self, change: &StreamChange, player_pos: Vec3) {
        if change.is_empty() {
            return;
        }
        // Columnas "sucias" para la luz: las que entran/salen y su anillo.
        let dirty = {
            let loaded = &self.world;
            let mut dirty: Vec<ChunkPos> = change.loaded.clone();
            for n in columns_to_remesh(change, |p| loaded.is_loaded(p)) {
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
            self.mesh_queue.retain(|(p, _)| p != pos);
        }
        // Encolar las columnas nuevas y sus vecinas de borde; se meshean
        // repartidas entre frames (`pump_meshing`), empezando por las cercanas.
        let mut to_queue: Vec<ChunkPos> = change.loaded.clone();
        for n in columns_to_remesh(change, |p| self.world.is_loaded(p)) {
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
            self.queue_column(pos);
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

    /// Encola una **seccion** para (re)meshear, si la columna esta cargada.
    fn queue_section(&mut self, pos: ChunkPos, section: usize) {
        if self.world.is_loaded(pos) && !self.mesh_queue.contains(&(pos, section)) {
            self.mesh_queue.push_back((pos, section));
        }
    }

    /// Encola todas las secciones de una columna (chunk nuevo del streaming).
    fn queue_column(&mut self, pos: ChunkPos) {
        if !self.world.is_loaded(pos) {
            return;
        }
        for section in 0..SECTION_COUNT {
            if !self.mesh_queue.contains(&(pos, section)) {
                self.mesh_queue.push_back((pos, section));
            }
        }
    }

    /// Envia a los workers el meshing de las secciones de la cola hasta agotar un
    /// presupuesto de tiempo (construye el snapshot en el hilo principal, que es
    /// barato). Reparte el coste entre frames y no da un tiron.
    fn pump_meshing(&mut self, budget_ms: f32) {
        let start = Instant::now();
        while let Some((pos, section)) = self.mesh_queue.pop_front() {
            if !self.world.is_loaded(pos) {
                continue;
            }
            // Seccion vacia: no hay geometria. Limpia la malla y descarta
            // cualquier resultado pendiente (revision nueva), sin snapshot.
            if self.world.section_is_empty(pos, section) {
                let entry = self
                    .meshes
                    .entry(pos)
                    .or_insert_with(|| Box::new(ColumnMeshes::default()));
                entry[section] = SectionMeshes::default();
                let slot = self.mesh_rev.entry((pos, section)).or_insert(0);
                *slot = slot.wrapping_add(1);
                continue;
            }
            let revision = {
                let entry = self.mesh_rev.entry((pos, section)).or_insert(0);
                *entry = entry.wrapping_add(1);
                *entry
            };
            let snapshot = section_snapshot(&self.world, pos, section);
            let origin = World::chunk_origin(pos);
            if !self.mesh_scheduler.request(MeshJob {
                pos,
                section,
                revision,
                origin,
                snapshot,
            }) {
                eprintln!("[render] no se pudo encolar meshing de {pos:?} sec {section}");
            }
            if start.elapsed().as_secs_f32() * 1000.0 >= budget_ms {
                break;
            }
        }
    }

    /// Recoge las mallas terminadas en los workers y las sube a la GPU. Valida la
    /// **revision**: si la seccion cambio mientras se mesheaba, el resultado se
    /// descarta.
    fn poll_meshing(&mut self) {
        let outputs: Vec<MeshOutput> =
            std::iter::from_fn(|| self.mesh_scheduler.try_recv()).collect();
        for out in outputs {
            if self.mesh_rev.get(&(out.pos, out.section)) != Some(&out.revision) {
                continue; // obsoleto (la seccion cambio despues)
            }
            if !self.world.is_loaded(out.pos) {
                continue;
            }
            let entry = self
                .meshes
                .entry(out.pos)
                .or_insert_with(|| Box::new(ColumnMeshes::default()));
            let slot = &mut entry[out.section];
            let label = format!("col_{}_{}_sec_{}", out.pos.x, out.pos.z, out.section);
            update_mesh(
                &mut slot.opaque,
                &self.device,
                &self.queue,
                &out.opaque.0,
                &out.opaque.1,
                &label,
            );
            update_mesh(
                &mut slot.water,
                &self.device,
                &self.queue,
                &out.water.0,
                &out.water.1,
                &format!("{label}_water"),
            );
        }
    }

    /// Encola las secciones afectadas por una **edicion** en `voxel` (y las
    /// vecinas de borde): la seccion editada; la de al lado si el voxel toca un
    /// limite de seccion; y las columnas vecinas si toca un borde de chunk.
    /// Mucho mas barato que reconstruir 9 columnas x 24 secciones.
    fn refresh_sections(&mut self, voxel: [i32; 3]) {
        let (pos, local) = World::world_to_local(voxel);
        let section = local[1] / CHUNK_SIZE;
        let mut sections: Vec<usize> = vec![section];
        if local[1].is_multiple_of(CHUNK_SIZE) && section > 0 {
            sections.push(section - 1);
        }
        if local[1] % CHUNK_SIZE == CHUNK_SIZE - 1 && section + 1 < SECTION_COUNT {
            sections.push(section + 1);
        }
        let on_x = local[0] == 0 || local[0] == CHUNK_SIZE - 1;
        let on_z = local[2] == 0 || local[2] == CHUNK_SIZE - 1;
        let mut chunks: Vec<ChunkPos> = vec![pos];
        if on_x {
            chunks.push(ChunkPos::new(pos.x - 1, pos.z));
            chunks.push(ChunkPos::new(pos.x + 1, pos.z));
        }
        if on_z {
            chunks.push(ChunkPos::new(pos.x, pos.z - 1));
            chunks.push(ChunkPos::new(pos.x, pos.z + 1));
        }
        if on_x && on_z {
            chunks.push(ChunkPos::new(pos.x - 1, pos.z - 1));
            chunks.push(ChunkPos::new(pos.x + 1, pos.z + 1));
            chunks.push(ChunkPos::new(pos.x - 1, pos.z + 1));
            chunks.push(ChunkPos::new(pos.x + 1, pos.z - 1));
        }
        for c in chunks {
            for &s in &sections {
                self.queue_section(c, s);
            }
        }
    }

    /// ¿Hay bloque solido en este punto del mundo (coordenadas en bloques)?
    ///
    /// Para la **fisica** usamos `is_solid_or_unloaded`: una columna aun no
    /// generada cuenta como muro, de modo que el jugador no cae al vacio
    /// mientras el streaming asincrono la trae.
    pub fn is_solid_at(&self, point: Vec3) -> bool {
        self.world.is_solid_or_unloaded([
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        ])
    }

    /// Solidez mirando solo columnas **cargadas** (`Unloaded` = aire, no muro).
    /// Se usa para **posar** al jugador al arrancar (tras la carga sincrona
    /// inicial): asi encuentra el suelo real y no el "techo" del mundo vacio.
    pub fn is_solid_loaded_at(&self, point: Vec3) -> bool {
        self.world.is_solid([
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        ])
    }

    /// Bloque en estas coordenadas de voxel (aire si no hay columna).
    pub fn block_at(&self, voxel: [i32; 3]) -> Block {
        self.world.get_block(voxel)
    }

    /// ¿Hay **agua** en este punto del mundo (coordenadas en bloques)?
    pub fn is_water_at(&self, point: Vec3) -> bool {
        self.world
            .get_block([
                point.x.floor() as i32,
                point.y.floor() as i32,
                point.z.floor() as i32,
            ])
            .is_liquid()
    }

    /// Lanza un rayo y devuelve el primer bloque **golpeable** (solido, o la
    /// antorcha) de cualquier columna cargada.
    ///
    /// El agua y la lava **no** se apuntan: el rayo las **atraviesa**, de modo
    /// que se puede romper o colocar el bloque del fondo/detras (como en
    /// Minecraft). Si se apuntaran, el resaltado marcaria el liquido y no
    /// dejaria interactuar con lo que hay debajo.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<RayHit> {
        let is_hit = |x: i32, y: i32, z: i32| -> bool {
            // No se dispara a traves de lo **no cargado**: se trata como muro
            // (evita apuntar/colocar en el borde del load con datos fantasma).
            if !self.world.is_column_loaded([x, y, z]) {
                return true;
            }
            let block = self.world.get_block([x, y, z]);
            if block.is_liquid() {
                return false;
            }
            block.is_solid() || block == Block::Torch
        };
        raycast(origin, dir, max_distance, is_hit)
    }

    /// Cambia un bloque (coordenadas de mundo) y regenera las mallas afectadas.
    pub fn set_block(&mut self, voxel: [i32; 3], block: Block) -> bool {
        if !self.world.set_block(voxel, block) {
            return false;
        }
        // La luz de cielo se recalcula por region; la de bloque ya se actualizo
        // de forma incremental dentro de `set_block`. Re-mesheamos el area.
        let (pos, _) = World::world_to_local(voxel);
        self.world.recompute_skylight(&area3x3(pos));
        self.refresh_sections(voxel);
        true
    }

    /// Avanza la simulacion de agua y **re-meshea** las columnas que cambiaron
    /// (y sus vecinas). El agua no emite luz, asi que no recomputamos la luz de
    /// bloque: solo la geometria. Devuelve cuantas celdas proceso.
    pub fn tick_water(&mut self, budget: usize) -> usize {
        let dirty = self.world.tick_water(budget);
        if dirty.is_empty() {
            return 0;
        }
        // La superficie del agua depende de sus vecinas: encolamos las columnas
        // del anillo 3x3 (todas sus secciones; el pump salta las vacias).
        let mut to_remesh: Vec<ChunkPos> = Vec::new();
        for pos in &dirty {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = ChunkPos::new(pos.x + dx, pos.z + dz);
                    if self.world.is_loaded(n) && !to_remesh.contains(&n) {
                        to_remesh.push(n);
                    }
                }
            }
        }
        for n in to_remesh {
            self.queue_column(n);
        }
        dirty.len()
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
        // La luz de cielo se recalcula por region (area 3x3 de cada columna
        // tocada); el meshing se encola a granularidad de seccion por edicion.
        let mut sky_dirty: Vec<ChunkPos> = Vec::new();
        for pos in touched {
            for n in area3x3(pos) {
                if !sky_dirty.contains(&n) {
                    sky_dirty.push(n);
                }
            }
        }
        self.world.recompute_skylight(&sky_dirty);
        for &(voxel, _) in edits {
            self.refresh_sections(voxel);
        }
        applied
    }

    /// Actualiza el wireframe del bloque apuntado.
    ///
    /// No recrea la malla si el bloque apuntado no cambio: apuntar al mismo
    /// bloque durante muchos frames es lo normal, y crear buffers GPU cada frame
    /// era churn innecesario.
    pub fn set_highlight(&mut self, hit: Option<RayHit>) {
        let key = hit.map(|h| h.block);
        if key == self.highlight_hit {
            return;
        }
        self.highlight_hit = key;
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

    /// Volca TODAS las columnas modificadas, para guardar el mundo. Antes de
    /// clonarlas, vuelca al registro persistente las ediciones aun pendientes.
    pub fn snapshot_modified(&mut self) -> Vec<(ChunkPos, ChunkRecord)> {
        self.world.sync_modified();
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
    pub fn render(&mut self, view_projection: &Mat4, camera_pos: Vec3, ui_quads: &[UiQuad]) {
        // Manda a los workers el meshing pendiente (con presupuesto) y recoge lo
        // terminado, subiendolo a la GPU (validando revisiones).
        self.pump_meshing(MESH_BUDGET_MS);
        self.poll_meshing();

        // Prepara la interfaz (pixels -> NDC, subida al buffer) antes del pase.
        self.ui.prepare(
            &self.device,
            &self.queue,
            ui_quads,
            self.config.width,
            self.config.height,
        );

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
            self.start.elapsed().as_secs_f32(),
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
            // Pase opaco.
            for (pos, column) in &self.meshes {
                let (wx, wz) = (
                    (pos.x * CHUNK_SIZE as i32) as f32,
                    (pos.z * CHUNK_SIZE as i32) as f32,
                );
                for (index, section_meshes) in column.iter().enumerate() {
                    let Some(mesh) = section_meshes.opaque.as_ref() else {
                        continue;
                    };
                    let y0 = index as f32 * section;
                    if frustum
                        .intersects_aabb([wx, y0, wz], [wx + section, y0 + section, wz + section])
                    {
                        mesh.draw(&mut pass);
                    }
                }
            }
            // Pase de agua (translucido): mismo bind group, otro pipeline
            // (blending, sin escritura de z).
            pass.set_pipeline(self.pipeline.water_pipeline());
            for (pos, column) in &self.meshes {
                let (wx, wz) = (
                    (pos.x * CHUNK_SIZE as i32) as f32,
                    (pos.z * CHUNK_SIZE as i32) as f32,
                );
                for (index, section_meshes) in column.iter().enumerate() {
                    let Some(mesh) = section_meshes.water.as_ref() else {
                        continue;
                    };
                    let y0 = index as f32 * section;
                    if frustum
                        .intersects_aabb([wx, y0, wz], [wx + section, y0 + section, wz + section])
                    {
                        mesh.draw(&mut pass);
                    }
                }
            }

            if let Some(highlight) = self.highlight_mesh.as_ref() {
                pass.set_pipeline(self.highlight_pipeline.pipeline());
                highlight.draw(&mut pass);
            }

            // Interfaz 2D (hotbar/inventario) al final, siempre encima.
            self.ui.draw(&mut pass);
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
