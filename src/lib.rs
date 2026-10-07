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
//! ## Estado actual: v0.17.1 - Fix: overrun del buffer de malla
//!
//! `Mesh::new` creaba los buffers GPU con el tamano **exacto** de los datos pero
//! registraba `capacity = next_power_of_two()` (mayor). Al re-meshear una seccion
//! que crecia dentro de ese rango, `Mesh::update` creia que cabia y
//! `write_buffer` se salia del buffer (`wgpu Validation Error`), cerrando el
//! juego. El nuevo worldgen (variabilidad de mallas) lo destapo. Ahora el buffer
//! se crea con la capacidad reservada; test de la invariante `capacidad >= bytes`.
//!
//! Hereda de v0.17.0 (worldgen FASE 1/2). Siguiente: FASE 3 (bioma por celda).
//!
//! ## v0.17.0 - Worldgen por etapas (FASE 1/2)
//!
//! Primer rediseno del generador de mundo (auditoria de worldgen). Nuevo modulo
//! [`world::worldgen`] con:
//! * `WorldGenConfig` central + validacion y `WORLDGEN_CONFIG_VERSION`.
//! * seeds derivadas por campo (sin RNG con estado) y helpers de math
//!   (`smoothstep`, `remap`, `spline`).
//! * muestreador **celular (Worley)** determinista con id estable por celda.
//! * **continentalness** con domain warping y clasificacion
//!   `DeepOcean..Interior`; **costas de ancho variable** por celda; altura base
//!   continental + relieve macro + cordilleras (mascara de rango + cresta) +
//!   valles.
//! * preview offline: `cargo run --release --example worldgen_preview`.
//!
//! El relieve ya no depende del bioma; el bioma sigue por clima (FASE 3
//! `DEFERRED`). `GENERATOR_VERSION` sube a 9. Hereda de v0.16.1.
//!
//! Pendiente (honesto): bioma por region celular (FASE 3), hidrologia/rios
//! (FASE 5), jerarquia de cuevas (FASE 6), decoracion por reglas (FASE 7),
//! interpolacion de render y overlay de texto.
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
