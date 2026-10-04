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
//! ## Estado actual: v0.3.0 — Generacion de terreno
//!
//! * [`world::TerrainGenerator`]: altura procedural con ruido Perlin (crate
//!   `noise`), deterministica por semilla.
//! * Superficie de hierba, subsuelo de tierra y piedra por profundidad.
//! * Columnas de 16x16x384 divididas en secciones de 16^3; solo se dibujan las
//!   no vacias.
//! * Atlas de texturas procedural, camara FPS y z-buffer.
//!
//! ## Organizacion del codigo
//!
//! * [`engine`] — ciclo de vida de la app y bucle de eventos.
//! * [`render`] — la capa de GPU (una fina envoltura sobre wgpu).
//! * [`scene`] — que hay en el mundo (camara, y pronto entidades y chunks).
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
pub mod render;
pub mod scene;
pub mod world;

pub use engine::run;
