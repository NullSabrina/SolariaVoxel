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
use crate::render::font;
use crate::render::gui;
use crate::render::highlight::{HighlightPipeline, cube_edges};
use crate::render::mesh::Mesh;
use crate::render::mesh_worker::{MeshJob, MeshOutput, MeshScheduler};
use crate::render::model::{ModelMesh, ModelPipeline};
use crate::render::pipeline::ScenePipeline;
use crate::render::sky::{SkyBasis, SkyPipeline};
use crate::render::ui::{UiPipeline, UiQuad};
use crate::scene::player;
use crate::scene::{DayCycle, SkyParams, SkyState};
use crate::world::mesh_snapshot::section_snapshot;
use crate::world::{
    Block, CHUNK_SIZE, ChunkPos, ChunkRecord, FluidBudget, FluidDirty, RayHit, SECTION_COUNT,
    StreamChange, World, raycast,
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
                *slot = Some(Mesh::new(device, queue, label, vertices, indices));
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

/// Estado de cielo por defecto (media manana) para el arranque.
fn default_sky_state() -> SkyState {
    SkyState::at(&DayCycle::default(), &SkyParams::default())
}

/// `wgpu::Color` (lineal) a partir del horizonte del estado del cielo. El pase de
/// cielo cubre la pantalla, pero el clear evita parpadeos si algo lo saltara.
fn clear_color_from_sky(state: &SkyState) -> wgpu::Color {
    let c = (state.horizon_sun_side + state.horizon_anti_side) * 0.5;
    wgpu::Color {
        r: c.x as f64,
        g: c.y as f64,
        b: c.z as f64,
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

/// Radio de carga/culling por defecto, en chunks. Configurable con
/// `SOLARIA_VIEW_RADIUS` (1..=12). La niebla y el culling por distancia se atan a
/// el, asi que subirlo alarga la vista sin tocar nada mas.
const DEFAULT_VIEW_RADIUS: i32 = 4;

/// Radio de vista desde el entorno, acotado para no reventar la memoria.
fn view_radius_from_env() -> i32 {
    std::env::var("SOLARIA_VIEW_RADIUS")
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(DEFAULT_VIEW_RADIUS)
        .clamp(1, 12)
}

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

/// La seccion y sus vecinas verticales (la de arriba y la de abajo, si existen).
/// La geometria de agua de una seccion lee la celda de arriba (cara superior) y
/// las laterales comparten nivel con las de su misma `y`, asi que un cambio en
/// una seccion puede afectar a la contigua.
fn vertical_neighbor_sections(section: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(3);
    if section > 0 {
        out.push(section - 1);
    }
    out.push(section);
    if section + 1 < SECTION_COUNT {
        out.push(section + 1);
    }
    out
}

/// Ordena los drawables de agua de **lejos a cerca** (back-to-front) para que el
/// blending alfa componga bien. `f32::total_cmp` evita el `unwrap` de
/// `partial_cmp` y el panic con NaN.
fn sort_water_back_to_front(order: &mut [(f32, ChunkPos, usize)]) {
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
}

/// Distancia (0 si esta dentro) de `p` al intervalo `[lo, hi]` en un eje.
#[inline]
fn axis_distance(p: f32, lo: f32, hi: f32) -> f32 {
    if p < lo {
        lo - p
    } else if p > hi {
        p - hi
    } else {
        0.0
    }
}

/// Distancia al cuadrado del punto `cam` a la AABB `[min, max]` (0 si dentro).
/// Base del **culling por distancia**: una seccion cuya AABB entera queda mas
/// alla de la niebla (`fog_end`) esta totalmente cubierta por el color de cielo y
/// no hace falta dibujarla.
#[inline]
fn nearest_dist2(cam: Vec3, min: [f32; 3], max: [f32; 3]) -> f32 {
    let dx = axis_distance(cam.x, min[0], max[0]);
    let dy = axis_distance(cam.y, min[1], max[1]);
    let dz = axis_distance(cam.z, min[2], max[2]);
    dx * dx + dy * dy + dz * dz
}

/// Metricas del ultimo frame dibujado (sin la interfaz). Sirve para **medir** el
/// efecto del culling y alimentara el overlay de diagnostico (FASE 13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Columnas con mallas registradas.
    pub columns: u32,
    /// Secciones efectivamente dibujadas.
    pub sections_drawn: u32,
    /// Draw calls emitidos (opaco + agua).
    pub draw_calls: u32,
    /// Triangulos enviados (opaco + agua).
    pub triangles: u64,
    /// Secciones descartadas por el frustum.
    pub culled_frustum: u32,
    /// Secciones descartadas por distancia (totalmente en la niebla).
    pub culled_distance: u32,
}

/// Datos de la **mano en primera persona** para un frame: la proyeccion (la mano
/// va en espacio de vista, sin la matriz de vista) y las fases de animacion.
#[derive(Clone, Copy, Debug)]
pub struct HandView {
    pub projection: Mat4,
    /// Golpe en `0..1` (romper/colocar).
    pub swing: f32,
    /// Fase de balanceo al andar (radianes).
    pub bob: f32,
}

/// Datos del **personaje** en tercera persona: la vista completa (proyeccion *
/// vista), la matriz del modelo en el mundo y la fase de andar.
#[derive(Clone, Copy, Debug)]
pub struct CharacterView {
    pub view_projection: Mat4,
    pub world: Mat4,
    pub walk: f32,
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

    /// Pipeline de la interfaz 2D (hotbar/inventario) y sus texturas.
    ui: UiPipeline,
    _gui_texture: wgpu::Texture,
    /// Atlas de la fuente bitmap (overlay F3).
    _font_texture: wgpu::Texture,

    /// Modelo de la mano/personaje (cubos de color).
    model: ModelPipeline,
    /// Brazo en primera persona y el cubo del item sostenido.
    hand_arm: ModelMesh,
    hand_item: ModelMesh,
    /// Bloque del item actual (para no reconstruir su malla sin necesidad).
    hand_item_block: Option<Block>,
    /// Mallas de las piezas del personaje (una por hueso) y sus pivotes.
    char_parts: Vec<ModelMesh>,
    char_pivots: Vec<[f32; 3]>,

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
    /// Orden de dibujo del agua (translucido) del ultimo frame: `(dist2, pos,
    /// seccion)`. Se reutiliza entre frames (sin allocar por frame) y se ordena
    /// de **lejos a cerca** para que el blending alfa componga bien.
    water_order: Vec<(f32, ChunkPos, usize)>,
    /// Metricas del ultimo frame (culling, draw calls, triangulos).
    stats: FrameStats,

    clear_color: wgpu::Color,
    /// Pase de cielo (triangulo a pantalla completa).
    sky: SkyPipeline,
    /// Estado del cielo del ultimo frame, resuelto en CPU.
    sky_state: SkyState,
    /// Factor dia/noche (0..1) del ultimo frame, subido al shader.
    day_factor: f32,
    /// Distancia (bloques) a la que empieza la niebla. Atada al radio de vista.
    fog_start: f32,
    /// Distancia (bloques) a la que la niebla es total: borde del area cargada.
    /// El culling por distancia usa este valor.
    fog_end: f32,
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
        // Textura de interfaz + fuente + su pipeline (comparte la vista del atlas).
        let (gui_texture, gui_view) = Self::create_gui_texture(&device, &queue);
        let (font_texture, font_view) = Self::create_font_texture(&device, &queue);
        let ui = UiPipeline::new(
            &device,
            pipeline.atlas_view(),
            &gui_view,
            &font_view,
            config.format,
            Self::DEPTH_FORMAT,
        );
        // Modelo de la mano (cubos de color) y su pipeline.
        let model = ModelPipeline::new(&device, config.format, Self::DEPTH_FORMAT);
        // Pase de cielo (gradiente cenit <-> horizonte).
        let sky = SkyPipeline::new(&device, config.format, Self::DEPTH_FORMAT);
        let hand_arm = ModelMesh::new(&device, &queue, "hand.arm", &player::first_person_arm());
        let hand_item = ModelMesh::new(
            &device,
            &queue,
            "hand.item",
            &player::held_item([0.7, 0.7, 0.7]),
        );
        // Piezas del personaje (una malla por hueso + su pivote para animar).
        let body = player::character();
        let char_parts = body
            .iter()
            .enumerate()
            .map(|(i, p)| ModelMesh::new(&device, &queue, &format!("char.{i}"), &p.cuboids))
            .collect();
        let char_pivots = body.iter().map(|p| p.pivot).collect();

        // Radio de vista configurable (`SOLARIA_VIEW_RADIUS`, por defecto 4).
        // La niebla termina justo en el borde del area cargada (radio * 16), asi
        // que lo funde con el cielo, y el culling por distancia usa ese valor.
        let view_radius = view_radius_from_env();
        let fog_end = view_radius as f32 * CHUNK_SIZE as f32;
        let fog_start = fog_end * 0.625; // mismo ratio que 40/64
        println!(
            "[render] radio de vista: {view_radius} chunks (niebla {fog_start:.0}..{fog_end:.0})"
        );
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
            _font_texture: font_texture,
            model,
            hand_arm,
            hand_item,
            hand_item_block: None,
            char_parts,
            char_pivots,
            world,
            meshes: HashMap::new(),
            mesh_queue: VecDeque::new(),
            mesh_scheduler: MeshScheduler::new(2),
            mesh_rev: HashMap::new(),
            water_order: Vec::new(),
            stats: FrameStats::default(),
            clear_color: clear_color_from_sky(&default_sky_state()),
            sky,
            sky_state: default_sky_state(),
            day_factor: 1.0,
            fog_start,
            fog_end,
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

    /// Crea el atlas de la **fuente bitmap** (overlay F3).
    fn create_font_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: font::FONT_W,
            height: font::FONT_H,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solaria.font"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let pixels = font::build_pixels();
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
                bytes_per_row: Some(font::FONT_W * 4),
                rows_per_image: Some(font::FONT_H),
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
        // recomputamos (region afectada) antes de meshear. La luz de bloque usa
        // la **region** de las columnas que entran/salen (no todo el mundo).
        self.world.recompute_skylight(&dirty);
        let block_changed: Vec<ChunkPos> = change
            .loaded
            .iter()
            .chain(change.unloaded.iter())
            .copied()
            .collect();
        self.world.recompute_block_light_region(&block_changed);
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

    /// Avanza la simulacion de agua y **re-meshea solo las secciones** que
    /// cambiaron (mas las verticales colindantes y, si el cambio toco un borde
    /// de chunk, la columna vecina). Antes se re-mesheaba el anillo 3x3 completo
    /// de columnas (todas sus 24 secciones). El agua no emite luz, asi que no se
    /// recomputa luz: solo la geometria. Devuelve cuantas celdas se simularon.
    pub fn tick_water(&mut self, budget: FluidBudget) -> usize {
        let dirty = self.world.tick_water_with(budget);
        if dirty.is_empty() {
            return 0;
        }
        for d in &dirty {
            self.queue_fluid_dirty(d);
        }
        dirty.len()
    }

    /// Encola las secciones a re-meshear por un cambio de fluido. La geometria de
    /// una seccion lee las celdas vecinas en los 6 ejes, asi que se encola la
    /// seccion y las de arriba/abajo; si el cambio toco un borde X/Z del chunk,
    /// tambien las secciones correspondientes de la(s) columna(s) vecina(s).
    fn queue_fluid_dirty(&mut self, d: &FluidDirty) {
        let sections = vertical_neighbor_sections(d.section);
        for &s in &sections {
            self.queue_section(d.pos, s);
        }
        let mut cols: Vec<ChunkPos> = Vec::new();
        if d.edge_x {
            cols.push(ChunkPos::new(d.pos.x - 1, d.pos.z));
            cols.push(ChunkPos::new(d.pos.x + 1, d.pos.z));
        }
        if d.edge_z {
            cols.push(ChunkPos::new(d.pos.x, d.pos.z - 1));
            cols.push(ChunkPos::new(d.pos.x, d.pos.z + 1));
        }
        if d.edge_x && d.edge_z {
            cols.push(ChunkPos::new(d.pos.x - 1, d.pos.z - 1));
            cols.push(ChunkPos::new(d.pos.x + 1, d.pos.z + 1));
            cols.push(ChunkPos::new(d.pos.x - 1, d.pos.z + 1));
            cols.push(ChunkPos::new(d.pos.x + 1, d.pos.z - 1));
        }
        for c in cols {
            for &s in &sections {
                self.queue_section(c, s);
            }
        }
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
            Mesh::new(&self.device, &self.queue, "highlight", &v, &i)
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

    /// Tamano de la **superficie** de render, en pixels fisicos. Es el que usa
    /// la conversion a NDC de la interfaz, asi que el layout (centrar, alinear)
    /// debe basarse en el, no en el tamano de la ventana.
    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Fija el bloque que el jugador sostiene (reconstruye el cubo del item solo
    /// si cambia).
    pub fn set_hand_item(&mut self, block: Block) {
        if self.hand_item_block == Some(block) {
            return;
        }
        self.hand_item_block = Some(block);
        self.hand_item = ModelMesh::new(
            &self.device,
            &self.queue,
            "hand.item",
            &player::held_item(player::item_color(block)),
        );
    }

    /// Luz de cielo (0..15) en coordenadas de voxel (overlay F3).
    pub fn sky_light_at(&self, voxel: [i32; 3]) -> u8 {
        self.world.sky_light_at(voxel)
    }

    /// Luz de bloque (0..15) en coordenadas de voxel (overlay F3).
    pub fn block_light_at(&self, voxel: [i32; 3]) -> u8 {
        self.world.block_light_at(voxel)
    }

    /// Bioma en `(x, z)` (overlay F3).
    pub fn biome_at(&self, x: i32, z: i32) -> crate::world::Biome {
        self.world.biome_at(x, z)
    }

    /// Informe de memoria del mundo (CPU) por categorias. Ver [`crate::world::memory`].
    pub fn world_memory(&self) -> crate::world::WorldMemory {
        self.world.memory_report()
    }

    /// Bytes reservados en GPU por las mallas actuales (vertices + indices).
    pub fn gpu_mesh_bytes(&self) -> u64 {
        self.meshes
            .values()
            .flat_map(|column| column.iter())
            .map(|section| {
                section.opaque.as_ref().map_or(0, Mesh::gpu_bytes)
                    + section.water.as_ref().map_or(0, Mesh::gpu_bytes)
            })
            .sum()
    }

    /// Columnas con alguna malla registrada.
    pub fn mesh_columns(&self) -> usize {
        self.meshes.len()
    }

    /// Secciones pendientes de meshing en la cola.
    pub fn pending_mesh_sections(&self) -> usize {
        self.mesh_queue.len()
    }

    /// Metricas del ultimo frame (culling, draw calls, triangulos).
    pub fn frame_stats(&self) -> FrameStats {
        self.stats
    }

    /// Actualiza el cielo del frame. La unica fuente de verdad es
    /// [`crate::scene::SkyState`]: de aqui salen el `day_factor`, el color del
    /// clear (horizonte) y los colores del pase de cielo.
    pub fn set_sky(&mut self, state: &SkyState) {
        self.day_factor = state.day_factor.clamp(0.0, 1.0);
        self.clear_color = clear_color_from_sky(state);
        self.sky_state = *state;
    }

    /// Dibuja y presenta un frame.
    pub fn render(
        &mut self,
        view_projection: &Mat4,
        camera_pos: Vec3,
        sky_basis: &SkyBasis,
        ui_quads: &[UiQuad],
        hand: Option<HandView>,
        character: Option<CharacterView>,
    ) {
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

        // Cielo del frame: sube la base de camara y los colores ya resueltos.
        self.sky.update(
            &self.queue,
            &self.sky_state,
            sky_basis,
            self.start.elapsed().as_secs_f32(),
        );

        self.pipeline.update_uniforms(
            &self.queue,
            view_projection,
            [camera_pos.x, camera_pos.y, camera_pos.z],
            &self.sky_state,
            self.fog_start,
            self.fog_end,
            self.start.elapsed().as_secs_f32(),
        );

        // Mano en primera persona: la colocamos en espacio de vista y subimos sus
        // matrices al buffer del modelo (dos slots: brazo e item). `queue.write_buffer`
        // se aplica antes del pase, por eso se escribe todo aqui.
        let hand_light = 0.35 + 0.65 * self.day_factor;
        if let Some(h) = &hand {
            let root = player::hand_transform(h.swing, h.bob);
            self.model
                .set(&self.queue, 0, &(h.projection * root), hand_light);
            self.model.set(
                &self.queue,
                1,
                &(h.projection * root * player::item_transform()),
                hand_light,
            );
        }
        // Personaje (tercera persona): una matriz por hueso (slots 2..).
        if let Some(c) = &character {
            let pose = player::character_pose(c.walk);
            for (i, pivot) in self.char_pivots.iter().enumerate() {
                let m = c.view_projection * c.world * player::part_matrix(*pivot, pose[i]);
                self.model.set(&self.queue, 2 + i as u32, &m, hand_light);
            }
        }

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

        let mut stats = FrameStats {
            columns: self.meshes.len() as u32,
            ..Default::default()
        };
        let cull_dist2 = self.fog_end * self.fog_end;

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

            // Cielo primero (fondo): cubre la pantalla sin escribir profundidad.
            self.sky.draw(&mut pass);

            pass.set_pipeline(self.pipeline.pipeline());
            pass.set_bind_group(0, self.pipeline.bind_group(), &[]);
            let section = CHUNK_SIZE as f32;
            // Pase opaco. Culling jerarquico: 1) frustum, 2) distancia (una
            // seccion entera en la niebla no se dibuja: no se veria).
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
                    let aabb_min = [wx, y0, wz];
                    let aabb_max = [wx + section, y0 + section, wz + section];
                    if !frustum.intersects_aabb(aabb_min, aabb_max) {
                        stats.culled_frustum += 1;
                        continue;
                    }
                    if nearest_dist2(camera_pos, aabb_min, aabb_max) > cull_dist2 {
                        stats.culled_distance += 1;
                        continue;
                    }
                    mesh.draw(&mut pass);
                    stats.sections_drawn += 1;
                    stats.draw_calls += 1;
                    stats.triangles += (mesh.index_count() / 3) as u64;
                }
            }
            // Pase de agua (translucido): mismo bind group, otro pipeline con
            // blending alfa. **Decision**: z-test ON y z-write OFF (ver
            // `pipeline.rs`), porque el agua no debe tapar lo que tiene detras ni
            // escribir profundidad entre sus propias caras. Como el blending alfa
            // es sensible al orden y el `HashMap` de meshes no lo garantiza,
            // ordenamos las secciones de agua de **lejos a cerca** por distancia
            // a la camara (reutilizando el buffer `water_order`).
            pass.set_pipeline(self.pipeline.water_pipeline());
            self.water_order.clear();
            for (pos, column) in &self.meshes {
                let (wx, wz) = (
                    (pos.x * CHUNK_SIZE as i32) as f32,
                    (pos.z * CHUNK_SIZE as i32) as f32,
                );
                for (index, section_meshes) in column.iter().enumerate() {
                    if section_meshes.water.is_none() {
                        continue;
                    }
                    let y0 = index as f32 * section;
                    let aabb_min = [wx, y0, wz];
                    let aabb_max = [wx + section, y0 + section, wz + section];
                    if !frustum.intersects_aabb(aabb_min, aabb_max) {
                        continue;
                    }
                    if nearest_dist2(camera_pos, aabb_min, aabb_max) > cull_dist2 {
                        stats.culled_distance += 1;
                        continue;
                    }
                    // Distancia al centro de la seccion (basta para ordenar).
                    let dx = wx + section * 0.5 - camera_pos.x;
                    let dy = y0 + section * 0.5 - camera_pos.y;
                    let dz = wz + section * 0.5 - camera_pos.z;
                    self.water_order
                        .push((dx * dx + dy * dy + dz * dz, *pos, index));
                }
            }
            sort_water_back_to_front(&mut self.water_order);
            for &(_, pos, index) in &self.water_order {
                if let Some(mesh) = self
                    .meshes
                    .get(&pos)
                    .and_then(|column| column[index].water.as_ref())
                {
                    mesh.draw(&mut pass);
                    stats.draw_calls += 1;
                    stats.triangles += (mesh.index_count() / 3) as u64;
                }
            }

            if let Some(highlight) = self.highlight_mesh.as_ref() {
                pass.set_pipeline(self.highlight_pipeline.pipeline());
                highlight.draw(&mut pass);
            }

            // Mano en primera persona (brazo + item). Va muy cerca de la camara,
            // asi que gana el z-test frente al mundo.
            if hand.is_some() {
                self.model.draw(&mut pass, 0, &self.hand_arm);
                self.model.draw(&mut pass, 1, &self.hand_item);
            }
            // Personaje (tercera persona).
            if character.is_some() {
                for (i, mesh) in self.char_parts.iter().enumerate() {
                    self.model.draw(&mut pass, 2 + i as u32, mesh);
                }
            }

            // Interfaz 2D (hotbar/inventario) al final, siempre encima.
            self.ui.draw(&mut pass);
        }
        self.stats = stats;

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn las_secciones_vecinas_verticales_no_se_salen_del_rango() {
        assert_eq!(vertical_neighbor_sections(0), vec![0, 1]);
        assert_eq!(vertical_neighbor_sections(5), vec![4, 5, 6]);
        assert_eq!(
            vertical_neighbor_sections(SECTION_COUNT - 1),
            vec![SECTION_COUNT - 2, SECTION_COUNT - 1]
        );
    }

    #[test]
    fn la_distancia_a_la_aabb_es_cero_dentro_y_positiva_fuera() {
        let cam = Vec3::new(0.0, 0.0, 0.0);
        // Dentro de la caja: 0.
        assert_eq!(nearest_dist2(cam, [-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]), 0.0);
        // A 3 bloques en +X: distancia^2 = 9.
        let d = nearest_dist2(cam, [3.0, -1.0, -1.0], [5.0, 1.0, 1.0]);
        assert!((d - 9.0).abs() < 1e-4, "d={d}");
        // Diagonal (3, 4): 3^2 + 4^2 = 25.
        let d = nearest_dist2(cam, [3.0, 4.0, -1.0], [5.0, 6.0, 1.0]);
        assert!((d - 25.0).abs() < 1e-4, "d={d}");
    }

    #[test]
    fn el_agua_se_ordena_de_lejos_a_cerca() {
        let mut order = vec![
            (4.0, ChunkPos::new(0, 0), 0),
            (100.0, ChunkPos::new(3, 3), 5),
            (25.0, ChunkPos::new(1, 0), 2),
        ];
        sort_water_back_to_front(&mut order);
        let dists: Vec<f32> = order.iter().map(|d| d.0).collect();
        assert_eq!(dists, vec![100.0, 25.0, 4.0]);
        // La seccion mas lejana se dibuja primero.
        assert_eq!(order[0].1, ChunkPos::new(3, 3));
    }

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
