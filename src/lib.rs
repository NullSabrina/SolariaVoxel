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
//! ## Estado actual: v0.8.9 - Inventario completo y antorcha 3D
//!
//! * El inventario muestra **todos los bloques** (`ITEMS`, rejilla de 8
//!   columnas): ya salen tablones, agua, lava, obsidiana, tierra gruesa, grava
//!   y podzol. La hotbar son los 9 primeros.
//! * La antorcha anade el **palo 3D** de `assets/models/solaria_torch.bbmodel`
//!   (cubo central) ademas de las dos tablas cruzadas.
//! * Backend heredado de v0.8.8 (agua continua, fluid_mesher + water.wgsl),
//!   v0.8.7 (cache de ruido 2D, decoracion, cuevas por densidad) y v0.8.6 (lava).
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
