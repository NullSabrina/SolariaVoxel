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
//! ## Estado actual: v0.15.2 - Luz de bloque regional
//!
//! La luz de bloque al cruzar de chunk ya no recorre todo el mundo cargado:
//! `recompute_block_light_region` limpia y reconstruye solo la **region** (las
//! columnas que entran/salen mas su anillo de 1), y siembra la **frontera** desde
//! la luz preservada de fuera (sin bordes oscuros). El BFS de propagacion queda
//! acotado por los emisores de la region. Un test comprueba que coincide
//! **exactamente** con el recalculo global en un escenario de altas/bajas/frontera.
//!
//! Medido: cruce de chunk ~10.7 ms (era ~12 ms con solo la cache y ~19 ms antes).
//!
//! Hereda de v0.15.1 (cache de emisores). Fases 7/9/10/11/12/13 completadas.
//! Pendiente: batching/LOD, interpolacion de render y overlay de texto con fuente.
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
