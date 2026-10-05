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
//! ## Estado actual: v0.7.2 — Colision vertical por huella + auto-escalon
//!
//! * La fisica vertical y el `settle` miran la **huella completa** del jugador
//!   (no solo el punto central): ya no se hunde al pisar un escalon ni queda
//!   embebido en el terreno.
//! * **Auto-escalon** (`STEP_HEIGHT = 1.0`): las colinas de 1 bloque se suben
//!   andando; un muro de 2 bloques sigue exigiendo salto.
//! * Sobre v0.7.1: altura y bioma **por bloque** (colinas suaves). v0.7.0:
//!   biomas (Worley) y bloque `Snow`. v0.6.5: colision horizontal. v0.6.4:
//!   ciclo dia/noche. v0.6.3: consolidacion. v0.6.2: antorcha como cruz fina.
//!   v0.6.1: block light. v0.6.0: luz de cielo. v0.5.x: LZ4 + streaming.
//!
//! Siguiente (v0.7.x del roadmap): cuevas (v0.7.3) y oceanos (v0.7.4). Pendiente
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
