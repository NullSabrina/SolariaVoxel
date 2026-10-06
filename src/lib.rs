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
//! ## Estado actual: v0.10.0 - Registro central de bloques
//!
//! FASE 9 de la auditoria: se elimina la duplicacion de la metadata de bloques.
//! Antes vivia en tres sitios (`block.rs`, `ITEMS` de `app.rs`, tiles de
//! `atlas.rs`) que podian desincronizarse.
//! * Nuevo `world::registry` con `BlockDefinition` + tabla `BLOCKS`: nombre,
//!   tiles por cara, solidez, visibilidad, `RenderKind`, `FluidKind`, emision,
//!   si es item y dureza (reservada).
//! * `Block` sigue siendo un `u8` y delega sus consultas en la tabla.
//! * `atlas::TILES` se **deriva** del registro y `app.rs` consume
//!   `BlockRegistry::items()` (sin lista propia). Tests garantizan coherencia.
//!
//! Hereda de v0.9.3 (transparencia ordenada), cerrando la FASE 7. Siguiente
//! (auditoria): memoria (FASE 10), renderer scale (FASE 11).
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
