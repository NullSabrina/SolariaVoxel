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
//! ## Estado actual: v0.7.6 — Optimizacion del streaming (sin tirones de FPS)
//!
//! * El meshing de las columnas nuevas ya no se hace de golpe en un frame: se
//!   **encola** y se procesa con un **presupuesto de 6 ms/frame** (empezando por
//!   las cercanas). El pico de ~80 ms al descubrir chunks baja a ~6 ms.
//! * **Greedy mas rapido**: la mascara 2D es plana y se reutiliza entre capas
//!   (antes reservaba un `Vec<Vec>` por capa) y las consultas de bloque dentro de
//!   la columna se leen directo, sin `HashMap`. Meshing ~55 -> ~37 ms.
//! * **Luz de cielo base** con `fill` + borrar solo lo subterraneo (~10 ms).
//! * Sobre v0.7.5: cuevas (Perlin 3D) + luz de cielo lateral. v0.7.4: culling +
//!   niebla. v0.7.3: re-mesheo de vecinas. v0.7.2: colision por huella +
//!   auto-escalon. v0.7.1: altura por bloque. v0.7.0: biomas (Worley).
//!
//! Siguiente (v0.7.x del roadmap): **oceanos** (v0.7.7): nivel del mar, agua,
//! playas y un pase de transparencia. Pendiente tambien el **palo 3D** del
//! `.bbmodel` y la **antorcha de pared**.
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
