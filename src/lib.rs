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
//! ## Estado actual: v0.8.1 — Texturas cartoon y hotbar fiel a la referencia
//!
//! Reestilizado visual completo, sin cambiar gameplay:
//! * Los 12 tiles del atlas se **repintaron en LibreSprite** con la paleta
//!   "Solaria Cartoon": 3 tonos cercanos por material, manchas suaves de 4x4,
//!   sin negro puro (estilo Luanti/Terasology, dibujo original).
//! * La hotbar es la **referencia D del usuario** (medida del PNG): marco
//!   `#4E351E`, ranuras hundidas `#1C0B02/#2B190C/#352011/#311C0F`. La textura
//!   vive en `assets/gui.png` (pintada en LibreSprite) con fallback procedural.
//! * El fallback procedural usa los mismos tonos base.
//!
//! Hereda de v0.8.0 (Etapa 2): hotbar de 9 ranuras, inventario (`E`) y guardado
//! de posicion.
//!
//! Siguiente (Etapa 2): **crafteo** (rejilla + recetas) y **mobs**.
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
