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
//! ## Estado actual: v0.8.13 - Guardado en segundo plano (async)
//!
//! Segunda fase de la auditoria (save asincrono):
//! * Nuevo `engine::save_worker`: un hilo serializa y escribe el mundo de forma
//!   atomica; el hilo principal solo pide el guardado y consulta el resultado.
//! * **Autoguardado** cada 5 min sin bloquear el render, y al cerrar se espera
//!   (`join`) para no perder el ultimo estado (idempotente).
//! * Prerequisito del siguiente paso: `TerrainGenerator` es `Send + Sync`
//!   (`Cell<u32>` -> `AtomicU32`), listo para workers de generacion.
//!
//! Hereda de v0.8.12 (persistencia v4: columna completa, atomica, validada).
//!
//! Siguiente (auditoria): streaming por jobs + revisiones, luz incremental,
//! meshing async.
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
