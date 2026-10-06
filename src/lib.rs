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
//! ## Estado actual: v0.8.16 - Meshing por secciones (dirty sections)
//!
//! FASE 6 (parte de dirty sections) de la auditoria:
//! * La cola de meshing es por **seccion** `(columna, seccion)`, no por columna.
//! * Editar un bloque encola solo la seccion afectada (y las vecinas de borde
//!   cuando toca un limite de seccion/chunk); antes `refresh_area` reconstruia 9
//!   columnas x 24 secciones.
//! * El resto (streaming/agua) encola todas las secciones de la columna; el pump
//!   con presupuesto de tiempo salta las vacias.
//!
//! Hereda de v0.8.15 (luz de bloque incremental), v0.8.14 (streaming por jobs).
//!
//! Siguiente (auditoria): meshing async en workers + revisiones, buffers GPU
//! reutilizables, fluids (active set), registry.
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
