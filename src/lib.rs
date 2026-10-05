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
//! ## Estado actual: v0.7.3 — Caras de borde del streaming (muros/grietas)
//!
//! * Al cargar/descargar columnas se **reconstruyen las vecinas** de borde: ya
//!   no aparecen muros oscuros (caras de mas) ni huecos (caras de menos) en el
//!   limite entre chunks.
//! * `build_column_meshes` **salta secciones vacias** (no hace greedy de las 24;
//!   solo de las que tienen geometria), lo que abarata el re-mesheo.
//! * Sobre v0.7.2: colision vertical por huella + auto-escalon. v0.7.1: altura y
//!   bioma por bloque. v0.7.0: biomas (Worley) y bloque `Snow`. v0.6.5: colision
//!   horizontal. v0.6.4: ciclo dia/noche. v0.6.3: consolidacion. v0.6.2: antorcha
//!   como cruz fina. v0.6.1: block light. v0.6.0: luz de cielo. v0.5.x: LZ4 +
//!   streaming.
//!
//! Siguiente (v0.7.x del roadmap): cuevas (v0.7.4) y oceanos (v0.7.5). Pendiente
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
