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
//! ## Estado actual: v0.8.18 - Reuso de buffers GPU
//!
//! FASE 6 (buffers) de la auditoria: re-meshear una seccion ya no crea/destruye
//! buffers GPU.
//! * `Mesh` reserva cada buffer con holgura (`next_power_of_two`) y expone
//!   `update(device, queue, vertices, indices)`: reescribe con `write_buffer`
//!   mientras quepa, y solo recrea si el nuevo tamano no cabe.
//! * `poll_meshing` usa `update` (via `update_mesh`) si la seccion ya tenia
//!   malla; `draw` salta si no hay indices.
//!
//! Hereda de v0.8.17 (meshing CPU asincrono), v0.8.16 (meshing por secciones).
//!
//! Siguiente (auditoria): fluids (active set + persistencia), registry, memoria.
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
