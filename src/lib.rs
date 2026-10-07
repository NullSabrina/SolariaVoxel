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
//! ## Estado actual: v0.22.0 - Worldgen FASE 7: decoracion por reglas
//!
//! La decoracion (arboles, rocas) pasa a un sistema de **reglas**
//! (`worldgen/decoration.rs`): cada tipo es una `DecorationRule` con
//! condiciones de bioma, altura, humedad, pendiente y cercania a rio, mas una
//! probabilidad. Dos ruidos de baja frecuencia dan coherencia: **clusters** (los
//! arboles se agrupan en bosques con claros) y **manchas de roca** en laderas
//! altas. La pendiente y la altura salen de una **rejilla de muestras con
//! padding** (una sola pasada), no de re-muestrear `height()` por candidato.
//!
//! `GENERATOR_VERSION -> 13`. `FORMAT_VERSION` intacto (5). Hereda de v0.21.1.
//!
//! ## v0.21.1 - El agua generada se asienta sola
//!
//! El agua de **worldgen** (rios/lagos) nacia como fuente y **nunca se
//! encolaba**: se quedaba congelada hasta que el jugador editaba algo cerca. Al
//! cargar una columna ahora se registra su **superficie de agua** (en `Column`,
//! una pista de runtime que no se guarda) y se encola la parte que **no esta en
//! equilibrio**, asi que se **asienta sola** al llegar. Ademas el tick baja a
//! **10 Hz** para que colocar agua no se vea nervioso.
//!
//! `GENERATOR_VERSION`/`FORMAT_VERSION` intactos (12/5). Hereda de v0.21.0.
//!
//! ## v0.21.0 - Agua estilo Minecraft (fuente -> distancia)
//!
//! `world/water.rs` deja de ser un **igualador que conserva volumen** (se
//! comportaba como una banera) y pasa al modelo de **Minecraft**:
//! * **Nivel 8 = fuente**; el flujo vale `8 - distancia` (alcance 7).
//! * El nivel de una celda de flujo se **recalcula** desde sus vecinos
//!   (`max - 1`), no de un volumen compartido → al quitar la fuente el agua
//!   **retrocede y desaparece**.
//! * **Fuentes infinitas** (2+ vecinos fuente), **caida** a nivel 8 con
//!   preferencia por bajar, y el flujo huerfano se seca.
//! * Tick **mas rapido** que Minecraft (20 Hz frente a 0.25 s por paso).
//!
//! `GENERATOR_VERSION`/`FORMAT_VERSION` intactos (12/5): no cambia el mundo ni
//! el guardado, solo la simulacion. Hereda de v0.20.0.
//!
//! ## v0.20.0 - Worldgen FASE 6: cuevas jerarquicas
//!
//! `world/caves.rs` pasa de un unico campo de densidad a **varios sistemas**
//! (tubos spaghetti que se cruzan, tuneles regionales, camaras `cheese`, pozos
//! verticales y canones) con una **mascara de preservacion** (pilares/puentes) y
//! **entradas** raras que rompen la corteza. La densidad se atenua por
//! profundidad (`surface - y`) y se refuerza bajo montanas (`mountain_mask`,
//! anadido a `TerrainSample`). El trabajo 2D (region de camaras, pozos, canones
//! y entradas) se calcula una vez por columna en `CaveContext`.
//!
//! `GENERATOR_VERSION → 12`. Hereda de v0.19.1.
//!
//! ## v0.19.1 - Limpieza y orden del repositorio
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
