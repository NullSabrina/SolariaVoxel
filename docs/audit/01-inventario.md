# Fase 1 de 3 — Inventario de SolariaVoxel

**Alcance:** mapa del proyecto y señales iniciales para la Fase 2; no es una auditoría profunda de implementación.

## Estado y fuente examinada

- Al iniciar la fase, la carpeta de trabajo contenía únicamente `.git/`. `git status` marcaba **278 archivos versionados como borrados** (aprox. 41.370 líneas de texto eliminadas en el diff de trabajo). No había fuentes, assets ni documentación del proyecto presentes en el árbol de trabajo. La única ruta añadida durante esta fase es este informe, `docs/audit/01-inventario.md`.
- Para no reconstruir ni alterar trabajo local, el mapa de contenidos se obtiene de los objetos del **repositorio Git local** en `HEAD` (`4d1c32d`, tag `v0.47.0`). Esto permite describir el snapshot local almacenado, pero no demuestra que los archivos estén materializados en disco ni que la compilación actual funcione.
- La comparación permitida con GitHub dio el mismo `HEAD`: `4d1c32d05f330e9d4ed495cb43192014ab7f94f6` en `HEAD`, `origin/master` y el `HEAD` anunciado por el remoto. **No hay divergencia de commits entre local y remoto.** La divergencia es el árbol de trabajo local: las 278 eliminaciones pendientes frente a ese commit.
- No se ejecutaron `cargo test`, `cargo fmt` ni `cargo clippy`: falta `Cargo.toml` en el árbol de trabajo. Las cifras de tests citadas abajo proceden de la documentación versionada, no de una ejecución de esta fase.

## 1. Árbol comentado (máximo 3 niveles)

Árbol del snapshot local `HEAD`; las carpetas del proyecto listadas no están actualmente materializadas en el árbol de trabajo (salvo `docs/audit/`, creado para este informe).

```text
.
├── Cargo.toml / Cargo.lock       Manifiesto y lockfile de la aplicación Rust.
├── README.md                     Estado, ejecución, controles y comandos de desarrollo.
├── ARCHITECTURE.md               Diseño, límites entre módulos e invariantes.
├── DECISIONS.md                  Diario extenso de decisiones y evolución por versión.
├── src/                          Código de la librería y del ejecutable (87 archivos).
│   ├── main.rs / lib.rs          Arranque mínimo y API raíz `run()`.
│   ├── engine/                   Aplicación, eventos, input, ventana, demos y guardado en hilo.
│   ├── world/                    Bloques/chunks, terreno, mundo, guardado, streaming y fluidos.
│   │   └── worldgen/             Generadores Legacy/Graph y pipeline de Larion.
│   ├── render/                   Renderer wgpu, mallas, UI/cielo y shaders WGSL.
│   │   └── shaders/              Shader WGSL específico del agua.
│   ├── ui/                       Estado de pantallas, inventario, opciones, input e idiomas.
│   ├── scene/                    Cámara, jugador, cielo y ciclo día/noche.
│   ├── player/                   Control y movimiento del jugador.
│   ├── math/                     Tipos y operaciones matemáticas 3D propias.
│   └── physics.rs                Colisiones/física AABB de entidades.
├── assets/                       Atlas, GUI, texturas y fuentes editables.
│   ├── backup/                   Arte y mapas históricos; no usados por el runtime.
│   ├── models/                   Fuente Blockbench del modelo de antorcha.
│   └── src/models/               Modelo celeste de referencia y fuentes Aseprite.
├── examples/                     8 binarios auxiliares para previews, pruebas y generación.
├── tools/                         4 scripts de generación de arte/capturas.
├── docs/                          Rendimiento y guía de generación de mundo.
└── screenshots/                  Capturas de versiones, pruebas y previews (143 PNG).
```

El snapshot tiene 278 archivos versionados: 89 `.rs`, 6 `.wgsl` y 154 `.png`, además de documentación, datos y herramientas. Los conteos se solapan por subdirectorios (por ejemplo `src/world/` incluye `src/world/worldgen/`). No se encontró configuración versionada de CI ni un workspace Cargo de múltiples crates.

## 2. Stack, dependencias y entradas

| Área | Inventario |
|---|---|
| Motor | Motor de voxeles propio; no usa un motor de juego. `winit` gestiona ventana/eventos y `wgpu` es la capa de GPU. |
| Lenguaje principal | Rust, edición 2024; `rust-version = 1.87`. README declara probado con Rust 1.98.1. |
| Shaders | WGSL: escena, agua, cielo, modelo, UI y resaltado; el módulo `render` incluye un test de parseo/validación con Naga. |
| Build y dependencias | Cargo, `Cargo.toml` + `Cargo.lock`, una crate con librería y binario. Perfiles `dev` optimizan dependencias a nivel 3. |
| Dependencias principales | `wgpu 30.0.1`, `winit 0.30.13`, `pollster 1.0.1`, `bytemuck` (derive), `noise 0.9.0`, `bincode 2` (derive), `lz4_flex 0.14.0`, `png 0.18.1`, `serde` (derive) y `serde_json`. |
| Punto de entrada ejecutable | `src/main.rs` delega en `solaria_voxel::run()`. |
| Entrada de la librería | `src/lib.rs` exporta módulos y `engine::run`; el ciclo de vida principal está en `src/engine/app.rs`. |
| Herramientas de desarrollo | `cargo run`, `cargo test`, `cargo fmt`, `cargo clippy`; ejemplos ejecutables con `cargo run --example <nombre>`. `tools/` incluye Python, JavaScript ejecutado en LibreSprite y PowerShell. |

Datos de versión del snapshot: paquete `0.47.0`, `GENERATOR_VERSION = 21`, `FORMAT_VERSION = 6`, `LARION_CONFIG_VERSION = 1`. README documenta 429 tests; la última entrada de `DECISIONS.md` declara 432. Es una discrepancia documental pendiente de confirmar al restaurar un checkout ejecutable.

## 3. Candidatos de limpieza (no borrados)

Los candidatos se identifican en `HEAD`; el árbol de trabajo ya presenta borrados generales, por lo que esta tabla **no** atribuye intención ni recomienda borrar esos 278 archivos. “Riesgo” valora el efecto de retirar el candidato del snapshot si se confirma que ya no se necesita.

| Candidato | Motivo / evidencia | Confianza | Riesgo de borrarlo |
|---|---|---:|---|
| `assets/backup/` completo (9 archivos) | `assets/README.md` declara que son variantes históricas que no carga el motor y que se pueden borrar si no se necesita trazabilidad. Incluye mapas, arte de antorcha y modelos anteriores. | Alta | Bajo para runtime; medio para conservar historia de arte. |
| `assets/backup/models/solaria_torch.backup.bbmodel` y `solaria_torch.pre28.bbmodel` | Sus hashes Git son idénticos (`16b9e1…`). Duplica exactamente el mismo contenido bajo dos nombres. | Alta | Bajo para runtime; bajo/medio para archivo histórico. |
| `assets/star.png` | La guía lo describe como reserva y dice que las estrellas visibles del cielo son procedurales; no hay carga/uso de `star.png` en código. | Media | Bajo para ejecución; medio si se quiere preservar el recurso artístico. |
| `assets/gui.json` | Se describe como manifiesto de regiones, pero la implementación documentada mantiene las regiones como constantes y las búsquedas en Rust solo lo mencionan en comentarios. Confirmar consumidores externos antes de retirar. | Media | Bajo para runtime actual; medio para herramientas o flujo futuro. |
| `tools/gen_atlas_medieval.py` | Herramienta alternativa no usada en el runtime; `assets/README.md` la identifica como variante medieval, no como generador canónico. Candidata a archivar si la variante ya no se mantiene. | Media | Bajo para runtime; medio para reproducir esa variante de arte. |
| `assets/src/sun.aseprite`, `assets/src/star.aseprite` y `assets/src/models/solaria_celestial.bbmodel` | Fuentes anteriores o de referencia; la documentación explica que se conservan por trazabilidad/proporciones. No son código muerto, pero podrían moverse a archivo si se decide reducir el repositorio. | Baja | Bajo para runtime; medio/alto para edición o referencia de arte. |

**No clasificar como huérfanos sin más comprobación:** `src/world/mesher.rs` sigue siendo usado por greedy meshing, al menos para emitir antorchas y como referencia de tests; `worldgen/graph.rs` sigue siendo generador admitido y probado; `assets/atlas.png`, `gui.png`, `sun.png` y `moon.png` tienen consumidores/fallbacks documentados. Las capturas son principalmente históricas, pero forman un índice por versión, no duplicados obvios.

**Hueco de reproducibilidad observado:** documentación y `tools/gen_gui.js` citan `assets/src/ui/gui.aseprite` como fuente editable y el script dice que la guarda allí, pero el árbol de `HEAD` no contiene `assets/src/ui/` ni ese archivo. Verificar si se omitió intencionalmente o si falta al snapshot antes de depender de la regeneración de GUI.

## 4. Peso e importancia de módulos (prioridad para Fase 2)

Prioridad cualitativa basada en responsabilidad, alcance transversal y tamaño/documentación del snapshot; no equivale a severidad de defectos.

| Prioridad | Módulo | Peso / importancia | Por qué priorizarlo |
|---|---|---|---|
| P0 | `src/world/` y `src/world/worldgen/` | Muy alto — corazón funcional y de persistencia del mundo. | Generación, datos de chunks, meshing, luz, fluidos, streaming y tres caminos de terreno. Cambios pueden afectar determinismo y compatibilidad de mundos. |
| P0 | `src/engine/app.rs` y `src/engine/` | Muy alto — integración/orquestación. | Punto de acople entre eventos, UI, jugador, renderer, mundos y guardado; `app.rs` ronda 2.936 líneas en el snapshot. |
| P0 | `src/world/save.rs`, `library.rs` | Muy alto — integridad y datos de usuario. | Formato binario versionado, cadena de migraciones, copias/recuperación y biblioteca de mundos; `save.rs` ronda 1.380 líneas. |
| P1 | `src/render/` | Alto — salida visual, buffers y sincronización GPU. | Renderer de 1.378 líneas, pipeline, mallas, shaders, agua translúcida, modelos y UI. Tiene validación WGSL sin GPU, pero no sustituye comprobación visual. |
| P1 | `src/ui/` + UI en `engine/app.rs` | Alto — lógica de interacción y persistencia de opciones. | Inventario/arrastre, navegación, pantallas, traducciones y controles; parte importante de la integración y del input vive en `app.rs`. |
| P1 | `src/player/`, `src/physics.rs`, `src/scene/` | Medio-alto — movimiento, cámara y representación. | Colisión, timestep, vuelo/tercera persona, cámara y estado de cielo/día-noche; influye en interacción y en la percepción de errores del mundo. |
| P2 | `examples/` | Medio — herramientas de validación y calibración. | 8 ejemplos producen mapas, galerías, métricas, capturas y assets. Son relevantes para reproducir worldgen sin GUI. |
| P2 | `assets/`, `tools/`, `screenshots/` | Medio en soporte; bajo en ejecución salvo los assets cargados. | Atlas, interfaz y texturas afectan la presentación; scripts reproducen arte; capturas e índices sirven como evidencia visual histórica. |
| P3 | `docs/`, `ARCHITECTURE.md`, `DECISIONS.md` | Alto como contexto, bajo como código ejecutable. | Son la principal fuente de invariantes, límites medidos y deuda conocida; necesitan conciliar las divergencias de versión mencionadas. |

## 5. Hotspots para orientar la auditoría profunda

1. **Integración en `engine/app.rs`:** concentración de responsabilidades y gran tamaño; revisar límites entre entrada, estados de pantalla, streaming, guardado y render al iniciar la Fase 2.
2. **Compatibilidad de mundo y guardado:** `FORMAT_VERSION = 6` y `GENERATOR_VERSION = 21` son independientes. Revisar migraciones históricas, metadatos `level.json`, selección de generador y recuperación de `.bak` como flujo de datos crítico.
3. **Tres generadores coexistentes:** Legacy16, Graph y Larion comparten `terrain.rs`. La documentación declara que Larion no altera los anteriores; validar invariantes de compatibilidad y selección efectiva por mundo.
4. **Worldgen Larion:** mucho código nuevo (ruido, splines, erosión, densidad en banda, mezcla de biomas y ríos), cifras calibradas en previews offline y coste documentado de 1,44× legacy. Queda pendiente validar el terreno integrado visualmente en GPU.
5. **Streaming y presupuesto:** `store.rs` ronda 2.073 líneas y conecta carga/descarga, iluminación, agua y colas de meshing. La documentación registra ~10,7 ms de iluminación por cruce, ~38 ms por 125 secciones y presupuesto de meshing de ~6 ms/frame; contrastar con perfiles actuales.
6. **Memoria y radio de vista:** cifras documentadas estiman ~477 MB CPU+GPU con R12 y ~871 MB con R16 frente al objetivo de <500 MB. La reserva de buffers GPU, número de secciones y falta de LOD aparecen como deuda conocida.
7. **Fluidos entre límites:** `water.rs`, `fluid_mesher.rs`, carga de columnas y radio de simulación tienen comportamiento de activación/reanudación documentado. Revisar continuidad entre chunks, guardado y transiciones de streaming.
8. **Meshing y render de agua:** greedy meshing, mallas asíncronas y orden de transparencia. La documentación deja pendiente el orden dentro de una sección y admite que falta validación visual en GPU.
9. **UI/input y persistencia:** `inventory_state.rs` tiene lógica stateful y arrastre; la hotbar requiere migración v5→v6 y opciones persistentes. Alinear pruebas de estado, flujo real de eventos y datos guardados.
10. **Documentación y arte reproducible:** `README.md`/`src/lib.rs` contienen resúmenes históricos que aún citan v0.45, `FORMAT_VERSION = 5` o 429 tests, frente a v0.47, formato 6 y 432 tests documentados más adelante. Además, falta en `HEAD` el `.aseprite` de GUI que el script dice generar. Conciliar antes de tomar estos textos como autoridad.
11. **Verificación local bloqueada por el estado del checkout:** los borrados globales impiden construir o correr tests desde el árbol actual. Antes de la auditoría de código de Fase 2 habrá que disponer de un árbol materializado sin perder ni descartar intencionadamente las eliminaciones locales.

## == Ancla Fase 2 ==

- SolariaVoxel es un motor voxel propio, sin motor de juego, escrito principalmente en Rust.
- Snapshot local `HEAD`: `4d1c32d`, tag `v0.47.0`; mismo commit que `origin/master` y remoto.
- Al inicio solo estaba `.git/`; esta fase añadió este informe. Los 278 archivos rastreados siguen borrados; no restaurarlos sin decisión explícita.
- Cargo, edición Rust 2024, mínimo Rust 1.87; ejecutable `src/main.rs` llama a `solaria_voxel::run()`.
- Plataforma/GPU: `winit` + `wgpu`; shaders WGSL y adaptador async `pollster`.
- Persistencia/serialización: `bincode 2`, `lz4_flex`, `serde`/`serde_json`; imagen PNG con `png`.
- Módulos prioritarios: `world`/`worldgen`, `engine/app.rs`, guardado/biblioteca y renderer.
- Tres generadores: Legacy16, Graph y Larion; versiones de mundo 21 y guardado 6.
- Hotspots documentados: streaming/iluminación, presupuesto de meshing, fluidos en fronteras y memoria GPU.
- Larion está medido a 1,44× legacy; falta validación visual GPU del mundo integrado.
- El objetivo documentado es menos de 500 MB; R12 ~477 MB y R16 ~871 MB CPU+GPU.
- README declara 429 tests y la última decisión 432; no se ejecutaron tests en esta fase.
- Hay candidatos de archivo en `assets/backup`; dos copias del modelo de antorcha tienen hash idéntico.
- `assets/src/ui/gui.aseprite` se menciona como fuente, pero no aparece en el árbol versionado.
