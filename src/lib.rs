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
//! ## Estado actual: v0.16.0 - Radio de vista configurable
//!
//! `SOLARIA_VIEW_RADIUS` (1..=12, por defecto 4) fija el radio de carga y, con el,
//! la **niebla** (`fog_end = radio * 16`) y el **culling por distancia**. Permite
//! escalar la vista y medir: radio 8 = 289 columnas, ~76 MB de mundo, 530 draw
//! calls, render ~2.4 ms (medido en `docs/performance.md`). El culling por
//! distancia mantiene el coste de render casi plano, asi que no se implemento
//! batching/LOD (optimizacion prematura sin un cuello medido).
//!
//! Hereda de v0.15.3 (hardening de tests). Fases 7/9/10/11/12/13 completadas.
//! Pendiente: interpolacion de render y overlay de texto con fuente.
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
