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
//! ## Estado actual: v0.12.0 - Renderer scale: culling y metricas
//!
//! FASE 11 de la auditoria (parte 1):
//! * `FrameStats`: columnas, secciones dibujadas, draw calls, triangulos y
//!   secciones descartadas por frustum/distancia. Se muestra en el titulo de la
//!   ventana y (con `SOLARIA_STATS=1`) se traza por consola para medir.
//! * **Culling jerarquico por distancia** ademas del frustum: una seccion cuya
//!   AABB entera queda mas alla de `FOG_END` esta totalmente cubierta por la
//!   niebla y no se dibuja. Medido en la vista de oceano: **128 draw calls**
//!   frente a 292 sin el culling por distancia (~56% menos).
//! * Hereda de v0.11.0 (memoria por categorias, FASE 10).
//!
//! Pendiente de FASE 11: batching por columna/material y LOD. Siguiente
//! (auditoria): fisica a timestep fijo (FASE 12).
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
