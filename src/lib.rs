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
//! ## Estado actual: v0.7.8 — Oceanos (agua translucida, playas, nado)
//!
//! * Nuevo bloque **`Water`** (id 9, tile 10): no solido y **translucido**.
//! * **Generacion**: el aire entre la superficie y el **nivel del mar** se rellena
//!   de agua (estilo `ocean.level` de Terasology / `water_level` de Luanti); las
//!   columnas a ras de agua tienen **playa/fondo de arena**.
//! * **Pase de transparencia**: el agua se separa de la geometria opaca en el
//!   mesher y se dibuja en un pipeline con **blending alfa** y sin escritura de z.
//! * **Nado**: en el agua la gravedad es menor (flotabilidad) y Espacio sube.
//! * Sobre v0.7.7: texturas de tierra con grano fino. v0.7.6: optimizacion del
//!   streaming. v0.7.5: cuevas + luz de cielo lateral.
//!
//! Siguiente: pulido (palo 3D del `.bbmodel`, antorcha de pared) o inventario
//! segun el roadmap (etapa 2, gameplay).
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
