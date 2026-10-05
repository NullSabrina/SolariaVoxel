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
//! ## Estado actual: v0.7.0 — Biomas (desierto, bosque, nieve)
//!
//! * El generador reparte el mundo en **tres biomas** con ruido **Worley**
//!   (cellular): desierto (arena), bosque (hierba) y nieve.
//! * Nuevo bloque `Snow` (id 8, tile 9, pintado en `assets/atlas.png`).
//! * `GENERATOR_VERSION` sube a 2 (cambio de generacion de terreno).
//! * Cierra la etapa 1 del roadmap ("mundo jugable": romper/colocar, versionado,
//!   luz, biomas). Sobre v0.6.5: colision horizontal. v0.6.4: ciclo dia/noche.
//!   v0.6.3: consolidacion. v0.6.2: antorcha como cruz fina. v0.6.1: block
//!   light. v0.6.0: luz de cielo. v0.5.x: LZ4 + streaming.
//!
//! Siguiente (v0.7.x del roadmap): cuevas (v0.7.1) y oceanos (v0.7.2). Pendiente
//! tambien el **palo 3D** del `.bbmodel` y la **antorcha de pared**.
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
