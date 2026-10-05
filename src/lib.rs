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
//! ## Estado actual: v0.8.0 — Hotbar, inventario y guardado de posicion
//!
//! Inicio de la **Etapa 2 (gameplay)**:
//! * **Hotbar** de 9 ranuras (barra rapida) con iconos de bloque, arte generado
//!   por codigo (referencia del usuario, opcion D). Se elige con `1`..`9` o la
//!   rueda; la ranura activa se resalta.
//! * **Inventario** (`E`): rejilla 3x3 con todos los bloques; click para
//!   asignarlos a la ranura activa.
//! * **Guardado completo de la posicion** del jugador (`FORMAT_VERSION` 3); se
//!   restaura al cargar. Compatible con mundos v2.
//! * Nuevo **pipeline de interfaz 2D** (`render::ui`) y textura de GUI
//!   (`render::gui`), ademas del bloque `Planks` (ya en v0.7.9).
//!
//! Siguiente (Etapa 2): **crafteo** (rejilla + recetas) y **mobs**; el crafteo
//! reusara este sistema de interfaz.
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
