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
//! ## Estado actual: v0.6.4 — Ciclo dia/noche
//!
//! * [`scene::DayCycle`] guarda la **hora del mundo** (0 = medianoche, 0.5 =
//!   mediodia) y calcula el **factor de luz del sol** (0..1) y el **color del
//!   cielo** (azul de dia, naranja al amanecer/atardecer, oscuro de noche).
//! * El vertice ahora lleva **dos luces separadas**: `sky` (cielo) y `block`
//!   (antorchas). El shader dibuja `max(sky * day_factor, block)`, de modo que
//!   al anochecer se apaga el sol pero **las antorchas siguen brillando**.
//! * El color de cielo se interpola en CPU y se sube como color de clear.
//! * `SOLARIA_TIME` (con `SOLARIA_DEMO`) fija la hora de la captura.
//! * Sobre v0.6.3: consolidacion (tests + `ARCHITECTURE.md`). v0.6.2: antorcha
//!   como cruz fina + atlas de texturas. v0.6.1: block light. v0.6.0: luz de
//!   cielo. v0.5.x: LZ4 + streaming.
//!
//! Pendiente (anotado en DECISIONS.md): el **palo 3D** del `.bbmodel` y la
//! **antorcha de pared** inclinada 22.5°, y los **biomas** (v0.7.x del roadmap).
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
