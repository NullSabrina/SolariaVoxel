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
//! ## Estado actual: v0.9.0 - Fluido local por columna
//!
//! FASE 7 (fluidos, parte 1) de la auditoria: el estado del agua deja de ser un
//! `HashMap<[i32; 3], Fluid>` global.
//! * Los **niveles de flujo** viven en la `Column`, empaquetados en **nibbles**
//!   (4 bits, `MAX_LEVEL = 8`) y asignados de forma **dispersa**: un oceano (todo
//!   fuentes) no reserva ni un byte. El flag "fuente" no se guarda: se infiere de
//!   `Block::Water` con flujo 0.
//! * El **active set** es la cola deduplicada de celdas: una celda en equilibrio
//!   (oceano quieto) sale al procesarse y no se re-encola, asi que no cuesta CPU.
//!   `World::pending_water_cells()` lo expone para diagnostico y tests.
//!
//! Hereda de v0.8.18 (reuso de buffers GPU), v0.8.17 (meshing CPU asincrono).
//!
//! Siguiente (auditoria): persistencia de fluidos, remeshing incremental,
//! transparencia ordenada, registry, memoria.
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
