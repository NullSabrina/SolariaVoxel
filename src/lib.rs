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
//! ## Estado actual: v0.13.0 - Fisica a timestep fijo
//!
//! FASE 12 de la auditoria:
//! * El jugador simula a **timestep fijo** (`FIXED_DT = 1/120`) con un
//!   acumulador acotado (`MAX_FIXED_STEPS`): el movimiento deja de depender del
//!   framerate (determinismo y base para entidades/multijugador). El giro de
//!   camara sigue siendo por frame.
//! * **Colisiones unificadas**: `physics::box_hits_solid` es la consulta comun
//!   que usa el jugador; gravedad/tope de caida/escala de agua son la misma
//!   constante en `player` y `physics` (antes duplicadas).
//! * Modelo explicito `VoxelAvailability::{Loaded, Unloaded, OutOfBounds}`: la
//!   fisica no confunde "sin cargar" con aire.
//!
//! Hereda de v0.12.0 (culling por distancia, FASE 11). Siguiente (auditoria):
//! diagnosticos/overlay (FASE 13).
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
