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
//! ## Estado actual: v0.8.14 - Streaming de terreno por jobs (async)
//!
//! FASE 2 de la auditoria: generar terreno fuera del hilo principal.
//! * Nuevo `world::streaming::TerrainScheduler`: pool de hilos que comparten el
//!   `TerrainGenerator` y generan `Column` con **id de peticion**; los resultados
//!   obsoletos (fuera del radio) se descartan.
//! * `World::plan_streaming` + `poll_generation`: el renderer encola y recoge en
//!   frames posteriores; la vista no se congela al cruzar de chunk.
//! * Modelo **Loaded/Unloaded**: la fisica trata una columna sin cargar como
//!   muro (`is_solid_or_unloaded`) para no caer al vacio; posar/meshing usan solo
//!   lo cargado.
//! * Arranque con `warm_streaming` (carga sincrona del area del jugador) y
//!   saneamiento de la posicion guardada. `Column` va **boxeada** (98 KB) para no
//!   desbordar la pila al moverla.
//!
//! Hereda de v0.8.13 (guardado async), v0.8.12 (persistencia v4).
//!
//! Siguiente (auditoria): luz incremental, meshing async, registry.
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
