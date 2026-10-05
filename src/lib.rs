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
//! ## Estado actual: v0.7.7 — Texturas de tierra con grano fino (estilo Luanti)
//!
//! * El **dirt** (tile 2) y el **lateral de hierba** (tile 1) se redibujan como
//!   **grano fino de bajo contraste** (nuestra paleta, tonos comprimidos y
//!   repartidos píxel a píxel) en vez de manchas grandes de tono oscuro/claro.
//!   Referencia de estilo: Luanti/Minetest (16x16, `grass_side` sobre `dirt`).
//! * El **fallback procedural** (`atlas.rs`) se alinea con el mismo grano, para
//!   clones sin `assets/atlas.png`.
//! * Sobre v0.7.6: optimizacion del streaming (cola de meshing con presupuesto,
//!   greedy y luz mas rapidos). v0.7.5: cuevas + luz de cielo lateral.
//!
//! Siguiente (v0.7.x del roadmap): **oceanos** (v0.7.8): nivel del mar, agua,
//! playas y un **pase de transparencia**. Pendiente tambien el **palo 3D** del
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
