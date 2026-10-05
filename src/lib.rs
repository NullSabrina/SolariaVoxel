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
//! ## Estado actual: v0.6.3 — Consolidacion (tests, arquitectura, limpieza)
//!
//! Version sin features nuevas: refuerza la calidad de lo ya construido.
//!
//! * **Tests** (83 en total): se anaden los de `block_overlaps_player`
//!   (colocacion sin meterse en el jugador), `face_tile` en las 6 caras de
//!   todos los bloques, transparencia del tile de la antorcha y **migracion de
//!   mundos preservando las ediciones del jugador**.
//! * **Bug corregido:** el atlas procedural (fallback sin `assets/atlas.png`)
//!   dibujaba el fondo de la antorcha opaco; ahora es transparente, como el
//!   atlas real, asi el cutout funciona tambien sin assets.
//! * **`ARCHITECTURE.md`**: mapa de modulos, flujo de un frame, versionado del
//!   mundo e invariantes del motor.
//! * **Limpieza:** la escena demo se extrae a `engine::demo` (fuera de
//!   `app.rs`).
//! * Sobre v0.6.2: la antorcha como cruz fina (cutout) + atlas de texturas.
//!   v0.6.1: block light. v0.6.0: luz de cielo. v0.5.x: LZ4 + streaming.
//!
//! Pendiente (anotado en DECISIONS.md): el **palo 3D** del `.bbmodel` y la
//! **antorcha de pared** inclinada 22.5°, y el ciclo dia/noche (que la guia
//! situa en v0.6.2 pero aun no hicimos).
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
