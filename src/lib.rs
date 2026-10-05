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
//! ## Estado actual: v0.7.5 — Cuevas + luz de cielo lateral
//!
//! * **Cuevas**: ruido **Perlin 3D** por bloque; donde su valor cruza un umbral
//!   (iso-superficie) se talla el terreno, formando tuneles y salas. No perfora
//!   la corteza (2 bloques bajo la superficie) ni el suelo. `GENERATOR_VERSION` 4.
//! * **Luz de cielo con propagacion lateral** (BFS a nivel de mundo): el aire bajo
//!   un techo (cuevas, voladizos) se ilumina de lado con atenuacion, en vez de
//!   quedar a oscuras de golpe. Recalculada por **region** (dirty + anillo 3x3).
//! * Sobre v0.7.4: culling (back-face + frustum), niebla, luz de bloque
//!   cross-chunk, FPS en el titulo. v0.7.3: re-mesheo de vecinas. v0.7.2: colision
//!   por huella + auto-escalon. v0.7.1: altura por bloque. v0.7.0: biomas (Worley).
//!
//! Siguiente (v0.7.x del roadmap): **oceanos** (v0.7.6). Pendiente tambien el
//! **palo 3D** del `.bbmodel` y la **antorcha de pared**.
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
