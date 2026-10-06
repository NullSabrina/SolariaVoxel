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
//! ## Estado actual: v0.15.0 - Migracion v1 y determinismo
//!
//! Dos pendientes de la auditoria:
//! * **Migracion v1 real**: `ChunkRecordV1`/`WorldSaveV1` (bloques sin comprimir,
//!   sin `compressed` ni `player_pos`). `load_from` ya no adivina el layout de un
//!   archivo v1 con el de v2; una version desconocida da error claro en vez de
//!   leer bytes mal interpretados.
//! * **Test de determinismo**: generar el mundo secuencialmente o con el pool de
//!   workers da exactamente el mismo hash de columna (orden de resultados
//!   irrelevante).
//!
//! Hereda de v0.14.0 (diagnosticos, FASE 13). Fases 7/9/10/11/12/13 completadas.
//! Pendiente: batching/LOD, interpolacion de render, luz de bloque incremental
//! en cambios de streaming, y overlay de texto con fuente.
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
