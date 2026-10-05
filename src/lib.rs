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
//! ## Estado actual: v0.8.8 - Agua como liquido continuo
//!
//! * Nuevo `world::fluid_mesher`: el agua se dibuja como **lamina continua**;
//!   las esquinas del quad superior interpolan `y + nivel/8` (rampa, sin
//!   escalones) y solo se emiten superficie y caras expuestas.
//! * Shader propio `water.wgsl`: **UVs animadas** con `time`, mezcla de dos
//!   muestras, **especular** Blinn-Phong por derivadas y agua mas oscura sin luz.
//! * Pipeline de agua: `cull_mode: None`, sin escritura de z y bandas alfa.
//! * Hereda de v0.8.7 (worldgen/fluidos), v0.8.6 (lava, obsidiana).
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
