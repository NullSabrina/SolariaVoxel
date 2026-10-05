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
//! ## Estado actual: v0.6.2 — La antorcha se dibuja como cruz fina
//!
//! * La antorcha ya **no es un cubo**: el mesher emite **dos quads cruzados**
//!   (planos `X = centro` y `Z = centro`) con el tile 8, y el shader hace
//!   *cutout* (descarta alfa < 0.5): se ve el palo fino y la llama, con el fondo
//!   transparente, como el modelo de Blockbench.
//! * El **raycast** golpea tambien bloques visibles no solidos (`is_solid ||
//!   is_visible`), asi que la antorcha se puede apuntar, resaltar y romper.
//! * Las caras de los bloques solidos vecinos a una antorcha se siguen
//!   dibujando (una antorcha no ocluye).
//! * Sobre v0.6.1: block light (antorchas, flood-fill BFS) y atlas cargado de
//!   `assets/atlas.png` como array de texturas con cutout. v0.6.0: luz de cielo;
//!   v0.5.x: LZ4, mundo en memoria + streaming.
//!
//! Pendiente (anotado en DECISIONS.md): el **palo 3D** del `.bbmodel` (cubo
//! `7,0,7→9,10,9`) y la **antorcha de pared** inclinada 22.5°, para una version
//! posterior dedicada al modelo de la antorcha.
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
