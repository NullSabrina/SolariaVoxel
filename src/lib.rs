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
//! ## Estado actual: v0.8.17 - Meshing CPU asincrono (+ revisiones)
//!
//! FASE 6 (meshing async) de la auditoria:
//! * `world::mesh_snapshot`: `SectionSnapshot` (18x18x18 con anillo de 1 bloque)
//!   que copia bloques/luz/agua de una seccion; el greedy/fluido corre sobre el.
//! * `render::mesh_worker::MeshScheduler`: pool de hilos que meshea el snapshot
//!   **sin tocar wgpu** y devuelve vertices/indices.
//! * El hilo principal: construye el snapshot (barato), manda el trabajo, y en
//!   `poll_meshing` **valida la revision** (descarta lo obsoleto) y sube a GPU.
//! * Las secciones vacias se saltan sin snapshot.
//!
//! Hereda de v0.8.16 (meshing por secciones), v0.8.15 (luz incremental).
//!
//! Siguiente (auditoria): reuso de buffers GPU, fluids (active set), registry.
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
