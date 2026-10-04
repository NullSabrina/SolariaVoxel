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
//! ## Estado actual: v0.5.0 — Versionado y guardado del mundo
//!
//! * [`world::save`]: `WorldHeader` (formato, generador, motor, semilla, fecha)
//!   y `ChunkRecord` (versionado por chunk).
//! * Serializacion binaria con `bincode`; el mundo se **guarda al salir** y se
//!   **carga al arrancar**, conservando las ediciones del jugador.
//! * [`world::save::MigrationChain`]: migradores encadenados por version de
//!   formato, con tests (v0 -> v1) y rechazo explicito de formatos futuros.
//! * Sobre v0.4.x: raycast, resaltado, romper/colocar, greedy meshing.
//!
//! ## Organizacion del codigo
//!
//! * [`engine`] — ciclo de vida de la app y bucle de eventos.
//! * [`render`] — la capa de GPU (una fina envoltura sobre wgpu).
//! * [`scene`] — que hay en el mundo (camara).
//! * [`world`] — bloques, chunks, generacion, meshing y raycast.
//! * [`player`] — fisica del jugador (gravedad, suelo, salto, vuelo).
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
pub mod player;
pub mod render;
pub mod scene;
pub mod world;

pub use engine::run;
