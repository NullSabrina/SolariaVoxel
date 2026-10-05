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
//! ## Estado actual: v0.8.15 - Luz de bloque incremental
//!
//! P0 de la auditoria: editar un bloque ya **no** recalcula la luz de bloque de
//! todo el mundo cargado.
//! * `World::relight_block`: cola de **remocion** (apaga la luz que partia de la
//!   celda y re-siembra desde las celdas con otra fuente) + cola de **adicion**
//!   (solo sube) acotadas al alcance de la luz (< 16 bloques). Cruza chunks.
//! * `set_block` la ejecuta; el renderer ya no llama a `recompute_block_light` en
//!   las ediciones. Test: incremental == recalculo global.
//!
//! Hereda de v0.8.14 (streaming por jobs), v0.8.13 (guardado async).
//!
//! Siguiente (auditoria): meshing async, fluids (active set), registry.
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
