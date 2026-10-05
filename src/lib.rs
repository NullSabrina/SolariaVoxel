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
//! ## Estado actual: v0.8.7 - Optimizacion de worldgen y fluidos
//!
//! * Terreno: el **ruido 2D se cachea por columna** (16x16 evaluaciones, no una
//!   por bloque `y`), **decoracion inteligente** (arboles en pendiente/flotando
//!   prohibidos) y **mezcla en bordes de bioma**.
//! * Cuevas: campo de **densidad 3D** (`tuneles*0.7 + camaras*0.3`) con
//!   atenuacion por profundidad.
//! * Agua: **equilibrio** (el oceano generado no cuesta CPU) y **fuentes 2x2**
//!   (`water::check_2x2_source`).
//!
//! Hereda de v0.8.6 (lava, obsidiana) y v0.8.5 (clima, cuevas 3D, acuiferos).
//!
//! Siguiente (Etapa 2): **mobs** (usando `physics`).
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
