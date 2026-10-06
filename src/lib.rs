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
//! ## Estado actual: v0.14.0 - Diagnosticos y benchmarks
//!
//! FASE 13 de la auditoria:
//! * Overlay **F3** (o `SOLARIA_STATS=1`): el titulo muestra fps, tiempos de
//!   `update`/`render`, draw calls, triangulos, columnas, cola de meshing,
//!   memoria del mundo y de GPU, y estado de guardado; ademas traza `[stats]`.
//!   (Sin fuente de texto aun, el "overlay" usa el titulo de la ventana.)
//! * `world::bench` (solo tests): benchmarks reproducibles de generacion,
//!   meshing, luz incremental, fluidos y guardado; informe en
//!   [`docs/performance.md`](../docs/performance.md).
//!
//! Con esto se cierra la lista de fases de la auditoria (7, 9, 10, 11, 12, 13).
//! Pendiente: batching/LOD (FASE 11), interpolacion de render (FASE 12), luz de
//! bloque incremental en streaming, y un overlay de texto con fuente.
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
