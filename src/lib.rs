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
//! ## Estado actual: v0.7.9 — Arboles y texturas de madera/hojas
//!
//! * **Arboles** por bioma (bosque 5%, nieve 2%, desierto no): tronco de `Wood`
//!   y copa de `Leaves`, deterministicos y sin cortar en el borde del chunk.
//! * **Hojas transparentes** (no solidas, con huecos de alfa 0 y cutout) como en
//!   Minecraft/Luanti: se ven y se atraviesan.
//! * Texturas de **tronco** (veta vertical), **extremo** (anillos) y **tablones**
//!   redibujadas con nuestra paleta (referencia Luanti). Nuevo bloque `Planks`.
//! * Sobre v0.7.8: oceanos. v0.7.7: texturas de tierra. v0.7.5: cuevas.
//!
//! **Etapa 1 del roadmap cerrada.** Siguiente: **Etapa 2 (gameplay)** — hotbar/
//! inventario, crafteo, mobs y **guardado completo** (posicion del jugador).
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
