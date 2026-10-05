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
//! ## Estado actual: v0.8.5 — Clima, biomas avanzados y cuevas 3D
//!
//! Worldgen de nueva generacion: mapa de **clima** (temperatura/humedad) -> 7
//! biomas, **relieve por bioma** (ridged en montanas), superficie variada
//! (`CoarseDirt`/`Gravel`/`Podzol`), **cuevas** spaghetti/cheese/pillar y
//! **acuiferos**. Ademas: fuentes de agua 2x2 y oceanos en equilibrio a coste
//! cero.
//!
//! Hereda de v0.8.4 (fisica AABB + agua), v0.8.3 (texturas + dim) y v0.8.2
//! (mesa de crafteo).
//!
//! Siguiente (Etapa 2): **mobs** (usando `physics`) y **lava`.
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
