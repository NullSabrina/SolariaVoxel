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
//! ## Estado actual: v0.7.4 — Rendimiento, niebla y luz que cruza chunks
//!
//! * **Back-face culling** (winding corregido en las caras horizontales) y
//!   **frustum culling** por seccion: menos trabajo de GPU.
//! * **Niebla a distancia** (se funde con el cielo, que cambia con el dia/noche)
//!   que disimula el borde del area cargada.
//! * **Luz de bloque que cruza chunks**: una antorcha cerca de un borde ilumina
//!   la columna vecina (BFS a nivel de mundo); se acaba el corte de luz.
//! * FPS visibles en el titulo de la ventana.
//! * Sobre v0.7.3: re-mesheo de vecinas al hacer streaming. v0.7.2: colision por
//!   huella + auto-escalon. v0.7.1: altura por bloque. v0.7.0: biomas (Worley).
//!   v0.6.x: luz, antorcha, ciclo dia/noche, colision horizontal.
//!
//! Siguiente (v0.7.x del roadmap): cuevas (v0.7.5) y oceanos (v0.7.6). Al llegar
//! las cuevas habra que dar a la **luz de cielo propagacion lateral** (hoy es por
//! columna vertical, correcta para un terreno de altura pero no para voladizos ni
//! cuevas). Pendiente tambien el **palo 3D** del `.bbmodel` y la **antorcha de
//! pared**.
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
