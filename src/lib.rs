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
//! ## Estado actual: v0.8.2 — Mesa de crafteo funcional (Etapa 2)
//!
//! Como una mesa de Minecraft, con nuestra hotbar D:
//! * Nuevo bloque `CraftingTable` (tiles 12 lateral / 13 tapa, atlas 64x64).
//!   Click derecho sobre ella abre la interfaz (click derecho normal coloca).
//! * Rejilla 3x3 + flecha pergamino + resultado; recetas con normalize como MC:
//!   1 madera -> tablones, 2x2 tablones -> mesa. Sin conteos (creativo): tomar
//!   el resultado lo asigna a la ranura activa y limpia la rejilla.
//! * Los tablones salen de ITEMS (se craftean); entra la mesa. `E`/Escape cierra.
//!
//! Hereda de v0.8.1 (texturas cartoon + hotbar D) y v0.8.0 (hotbar, inventario,
//! guardado de posicion).
//!
//! Siguiente (Etapa 2): **drops** (quedo pendiente de la guia) y **mobs**.
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
