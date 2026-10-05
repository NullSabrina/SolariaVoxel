//! # Solaria Voxel
//!
//! Un motor de voxeles escrito en Rust **desde cero**, sin motor de juego.
//! Usamos dos unicas crates de bajo nivel:
//!
//! * [`winit`] — la ventana y los eventos del sistema operativo.
//! * [`wgpu`] — el acceso a la GPU (Vulkan / Metal / Direct3D12 / OpenGL).
//!
//! Todo lo demas (matematicas, camara, bucle de juego, y en el futuro el
//! meshing de chunks, la iluminacion, el guardado versionado del mundo...) lo
//! escribimos y documentamos nosotros.
//!
//! ## Estado actual: v0.8.12 - Persistencia v4 (columna completa, atomica)
//!
//! Primera fase de la auditoria maestra (data correctness):
//! * `ChunkRecord` guarda **toda la columna** (24 secciones), no solo `y=64..80`:
//!   las ediciones en cualquier `y` (0..383) sobreviven. `FORMAT_VERSION = 4`
//!   con migrador v3->v4.
//! * Guardado **atomico** (`world.vf.tmp` -> `world.vf`, rotando `.bak`) y
//!   reintentable (`world_saved` solo se marca si la escritura termino bien).
//! * Registro **perezoso** (dirty set): no se recomprime la columna en cada
//!   `set_block`, solo al guardar o descargar.
//! * Al cargar se validan chunks corruptos y **IDs de bloque desconocidos** (no
//!   se cargan en silencio como aire).
//! * Highlight: se reutiliza la malla GPU si el bloque apuntado no cambia.
//!
//! Hereda de v0.8.11 (agua interactiva, antorcha), v0.8.9 (inventario completo).
//!
//! Siguiente (auditoria): streaming asincrono, luz incremental, meshing async.
//!
//! ## Organizacion del codigo
//!
//! * [`engine`] — ciclo de vida de la app y bucle de eventos.
//! * [`render`] — la capa de GPU (una fina envoltura sobre wgpu).
//! * [`scene`] — que hay en el mundo (camara).
//! * [`world`] — bloques, chunks, generacion (terreno/cuevas), meshing, agua.
//! * [`player`] — fisica del jugador (gravedad, suelo, salto, vuelo).
//! * [`physics`] — fisica AABB de entidades (base de los mobs).
//! * [`math`] — matematicas 3D propias (`Vec3`, `Mat4`).
//!
//! ## Como se ejecuta
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     solaria_voxel::run()
//! }
//! ```

pub mod engine;
pub mod math;
pub mod physics;
pub mod player;
pub mod render;
pub mod scene;
pub mod world;

pub use engine::run;
