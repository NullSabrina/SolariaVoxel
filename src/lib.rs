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
//! ## Estado actual: v0.15.3 - Hardening de tests (raycast y streaming)
//!
//! Bateria de tests de robustez que pedia la auditoria (§21 y §41):
//! * Raycast: origen dentro de un bloque, rayos negativos, direccion nula y casi
//!   cero, `max_distance = 0`, borde de chunk (x=15 -> 16), predicado que
//!   atraviesa liquidos, rayo sobre un borde de celda y diagonal en coordenadas
//!   negativas.
//! * Streaming: carga y edicion en **chunks negativos**.
//!
//! Hereda de v0.15.2 (luz de bloque regional). Fases 7/9/10/11/12/13 completadas.
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
