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
//! ## Estado actual: v0.19.1 - Limpieza y orden del repositorio
//!
//! Pasada de higiene antes de seguir con el worldgen (FASE 6+): se elimina
//! codigo muerto, los backups de arte van a `assets/backup/`, `Cargo.toml` gana
//! metadatos, se anaden licencia dual y `.gitattributes`, y se alinean
//! `README`/`docs`. No cambia el mundo: `GENERATOR_VERSION`/`FORMAT_VERSION`
//! intactos (11/5). Sigue a v0.19.0.
//!
//! ## v0.19.0 - Worldgen FASE 5: hidrologia (rios y lagos)
//!
//! Rios **estructurales**, no `noise > umbral`:
//! * La linea del rio sigue una **cresta** (`1 - |n|`) con su propio **domain
//!   warp** → trazados sinuosos y alargados.
//! * **Caudal** = humedad + ruido de baja frecuencia → ancho y profundidad
//!   variables (rios pequenos a grandes).
//! * El cauce **cava** el terreno (`cut = prox^power * depth`) y se rellena de
//!   agua hasta un nivel contenido bajo el borde; el material pasa a arena/grava.
//! * **Lagos**: depresion cerrada en valles humedos (`valle · humedad · cuenca`).
//! * El agua de worldgen nace **estable**: no entra en el active set del automata
//!   (solo las ediciones del jugador lo hacen).
//!
//! `GENERATOR_VERSION → 11`. Escena demo `SOLARIA_RIVER=1`; capa `river` en
//! `worldgen_preview`. Hereda de v0.18.0 (FASE 3).
//!
//! ## v0.18.0 - Worldgen FASE 3: bioma por region celular
//!
//! El bioma deja de ser una cascada de `if` sobre umbrales de clima:
//! * `worldgen/biomes.rs`: `BiomeDefinition` (rangos de temperatura/humedad/
//!   altura + densidad de arboles) y `select` por **scoring** con bandas suaves;
//!   anadir un bioma es anadir una fila.
//! * **Regionalizacion**: el clima se mezcla con el del **centro de la celda**
//!   (domina en el interior, clima local en el borde) → regiones de bioma
//!   coherentes de ~`cell_distance` bloques con transiciones suaves.
//! * **Lapse de altitud**: la temperatura baja con la altura (nieve en cumbres).
//! * El clima (`temperature`/`humidity`) vive ya en `WorldGen` (una sola muestra
//!   por (x,z) da geografia + clima + bioma).
//!
//! `GENERATOR_VERSION → 10`. Medido (preview 2048x2048, seed 13371): 7 biomas,
//! Plains 38% / Forest 33% / Savanna 18% / Swamp 6% / Desert 4% / Tundra 1% /
//! Taiga 1%. Hereda de v0.17.1 (fix buffer GPU).
//!
//! ## v0.17.1 - Fix: overrun del buffer de malla
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
