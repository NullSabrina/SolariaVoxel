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
//! ## Estado actual: v0.6.5 — Colision horizontal (la camara no entra en bloques)
//!
//! * El jugador ya **no atraviesa paredes**: `PlayerController::move_horizontal`
//!   lo mueve eje a eje contra el mundo (caja de radio `PLAYER_RADIUS` y alto
//!   `PLAYER_HEIGHT`) y se **desliza** a lo largo de las paredes.
//! * Antes solo habia fisica vertical, asi que la camara podia meterse dentro
//!   del terreno caminando en horizontal. Ese bug queda resuelto.
//! * Sobre v0.6.4: ciclo dia/noche (luz de cielo y de bloque separadas).
//!   v0.6.3: consolidacion. v0.6.2: antorcha como cruz fina. v0.6.1: block
//!   light. v0.6.0: luz de cielo. v0.5.x: LZ4 + streaming.
//!
//! Pendiente (anotado en DECISIONS.md): el **palo 3D** del `.bbmodel`, la
//! **antorcha de pared**, y los **biomas** (v0.7.x del roadmap).
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
