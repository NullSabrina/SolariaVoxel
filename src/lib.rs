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
//! ## Estado actual: v0.9.1 - Persistencia de fluidos
//!
//! FASE 7 (fluidos, parte 2) de la auditoria: el agua que fluye ya no vuelve a
//! fuente al recargar.
//! * `FORMAT_VERSION = 5`: `ChunkRecord` guarda los **niveles de flujo** del agua
//!   (campo `fluid`, mismo indice que los bloques, LZ4; vacio si no hay flujo).
//! * Migrador **v4 -> v5**: un mundo anterior no traia fluido, asi que todo
//!   `Water` se interpreta como **fuente**, exactamente su comportamiento previo
//!   (cero perdida). `ChunkRecordV4`/`WorldSaveV4` son el espejo posicional.
//! * `apply_record` restaura bloques y niveles; el flujo sobrevive tambien a
//!   descargar y recargar una columna en la misma sesion.
//!
//! Hereda de v0.9.0 (fluido local por columna). Siguiente (auditoria): remeshing
//! incremental de fluidos, transparencia ordenada, registry, memoria.
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
