//! # Solaria Voxel
//!
//! Motor de voxeles propio en Rust, sin motor de juego. `winit` gestiona la
//! ventana y los eventos; `wgpu` es la capa de GPU. Todo lo demas (bucle de
//! juego, matematicas, camara, meshing, iluminacion, fluidos, guardado versionado
//! y generacion procedural) es codigo de este repositorio.
//!
//! ## Estado actual (`v0.48.0`)
//!
//! * **Generadores**: `legacy16` (por etapas), `graph` (DAG de densidad) y
//!   `larion` (pipeline multiescala). `GENERATOR_VERSION = 22`; la version del pipeline se guarda en `level.json` (`generator_config`).
//! * **Guardado**: formato binario versionado `FORMAT_VERSION = 7` (bincode +
//!   LZ4 + checksum FNV-1a), con migradores v1..v7, escritura atomica, `.bak` y
//!   recuperacion automatica. Ver [`world::save`].
//! * **Mundos multiples**: `saves/<slug>/` con `level.json` legible. La base se
//!   resuelve con `SOLARIA_HOME`, una biblioteca existente en el directorio de
//!   trabajo o el directorio del ejecutable. Ver [`world::library`].
//! * **Fluidos** estilo Minecraft con presupuesto por tick y congelacion en los
//!   bordes de columnas no cargadas. Ver [`world::water`].
//! * **Render**: greedy meshing, pase de agua, frustum, luz por vertice, cielo
//!   dia/noche y UI 2D. Solo [`render`] habla con la GPU.
//!
//! El historico de decisiones y el detalle de cada version viven en
//! `DECISIONS.md`; la arquitectura y los invariantes, en `ARCHITECTURE.md`.
//!
//! ## Organizacion
//!
//! * [`engine`] — ciclo de vida de la app y bucle de eventos.
//! * [`render`] — la capa de GPU (envoltura fina sobre wgpu).
//! * [`scene`] — camara, cielo y ciclo dia/noche.
//! * [`world`] — bloques, columnas, generacion, meshing, agua y guardado.
//! * [`player`] — fisica del jugador.
//! * [`physics`] — fisica AABB de entidades.
//! * [`math`] — matematicas 3D propias.
//! * [`ui`] — estado de pantallas, inventario, opciones e idiomas.
//!
//! ## Ejecucion
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
pub mod ui;
pub mod world;

pub use engine::run;
