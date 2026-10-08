# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara, el meshing, la iluminacion, el guardado
versionado del mundo y la generacion procedural.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.44.2` — Worldgen con grafo (altura, densidad y clima)

- **Clima como canales del grafo** (`v0.44.2`): el camino `Graph` evalúa grafos de
  **temperatura** y **lluvia** y elige el bioma con el scoring existente
  (`biomes::select`); el bioma decide materiales y decoración. *(El generador
  legacy sigue con su clima de `WorldGen`.)*
- **Campo de densidad 3D** (`v0.44.1`): densidad `superficie − y + cueva` en una
  **retícula gruesa 4×4×4** interpolada trilinealmente; el ruido 3D recortado cava
  **cuevas/túneles** sin islas flotantes. El océano solo rellena por encima del
  terreno.
- **Grafo + coexistencia** (`v0.44.0`): `GeneratorKind::{Legacy16, Graph}` en
  `level.json` (sin migrar el binario). Grafo serializable a **JSON**. Mundos
  nuevos: `SOLARIA_GENERATOR=graph`. Determinista (test 1 vs N hilos).
- **Fix**: la luz de bloque no propaga a columnas no cargadas (antes podía crecer
  sin cota).
- **Fuente con tildes** (`v0.43.3`), **Opciones/Controles** (`v0.43.1`/`v0.43.2`),
  **mundos múltiples y pantallas** (`v0.42.0`), **astros texturizados** (`v0.35.0`),
  **UI creativa** (`v0.41.0`), **distancia de vista** (`v0.33.0`) y
  **cielo/atmosfera** (`v0.30`–`v0.31.1`).
- **Pendiente honesto**: cuevas del grafo sin acuífero, restricciones de decoración
  (C5), toolkit nine-slice, `sky_physical` opcional.

`GENERATOR_VERSION`/`FORMAT_VERSION` intactos (16/5).

- **Overlay F3** (`v0.27.0`): pantalla de diagnostico en pantalla (dos columnas)
  con una **fuente bitmap 5x7** propia.
- **Interpolacion de render** (`v0.26.0`) y agua estilo Minecraft
  (`v0.21.0`–`v0.21.1`).
- **Worldgen** (`v0.17`–`v0.25`, FASE 4/6/7/9 + tuning). Ver
  [`docs/worldgen.md`](./docs/worldgen.md).
- **Repo** (`v0.19.1`): limpieza y orden.
- **Formato de guardado** (`FORMAT_VERSION = 5`): independiente del generador,
  con migradores y test de datos del jugador.
- **Render**: greedy meshing con face culling, pase de agua translucida
  ordenado, frustum + culling por distancia, luz de cielo/ bloque por vertice,
  niebla atada al radio de vista, overlay F3.

Historial y decisiones en [`DECISIONS.md`](./DECISIONS.md); rendimiento medido en
[`docs/performance.md`](./docs/performance.md).

## Requisitos

- Rust stable (probado con `1.98.1`). Instala desde <https://rustup.rs>.
- Una GPU con soporte Vulkan / Direct3D12 / Metal.

## Como ejecutar

```bash
cargo run
```

La primera compilacion tarda unos minutos (compila `wgpu` y sus dependencias).
Veras en consola la GPU y el formato de superficie elegidos.

Al cerrar con **Escape** o la **X** de la ventana, el mundo se guarda en
`world.vf` (junto al ejecutable) y se recarga automaticamente la proxima vez.

## Controles

| Entrada | Accion |
| ------- | ------ |
| Click izquierdo (sin captura) | Captura el raton. |
| Click izquierdo (capturado) | **Rompe** el bloque apuntado. |
| Click derecho (capturado) | **Coloca** el bloque de la ranura activa. |
| `1`–`9` / rueda | Elige la ranura de la hotbar. |
| `E` | Abre/cierra el **inventario** (click para asignar). |
| `W`/`A`/`S`/`D` | Andar. |
| `Espacio` | Saltar / nadar. |
| `F` | Alterna modo **vuelo** (`Espacio`/`Shift` sube/baja). |
| `F3` | Alterna el **overlay de diagnostico** en pantalla. |
| `F5` | Alterna **primera / tercera persona** (ver el personaje). |
| `Escape` | Cierra inventario/menu; jugando abre la **pausa**. |

## Variables de entorno

Todas son opcionales y sirven para arrancar escenas de demo o ajustar limites.

| Variable | Efecto |
| -------- | ------ |
| `SOLARIA_DEMO` | Escena fija de demostracion (sin jugador). |
| `SOLARIA_OCEAN` | Demo de oceano. |
| `SOLARIA_BIOMES` | Demo de biomas. |
| `SOLARIA_RIVER` | Demo de rios. |
| `SOLARIA_CAVE` | Demo de cuevas. |
| `SOLARIA_CRAFT` | Demo de crafteo. |
| `SOLARIA_COLLIDE` | Demo de colision. |
| `SOLARIA_STATS` | Muestra el overlay de diagnostico al arrancar (`F3`). |
| `SOLARIA_THIRD` | Arranca en tercera persona (`F5`). |
| `SOLARIA_VIEW_RADIUS` | Radio de render en columnas (2–32; por defecto 12). |
| `SOLARIA_SIM_RADIUS` | Radio de simulacion de fluidos (1–12; por defecto 6). |
| `SOLARIA_FOG` | Modo de niebla: `off`, `far`, `normal` (def.), `short`. |
| `SOLARIA_INVENTORY` | Abre el inventario creativo al arrancar (capturas). |
| `SOLARIA_SEARCH` | Texto de busqueda inicial del inventario (abre el inventario). |
| `SOLARIA_TOAST` | Muestra el nombre del bloque de esa ranura sobre la hotbar. |
| `SOLARIA_HOME` | Directorio base de los mundos (`saves/` cuelga de aqui). |
| `SOLARIA_GENERATOR` | Generador de mundos **nuevos**: `legacy16` (def.) o `graph`. |
| `SOLARIA_SCREEN` | Arranca en una pantalla: `title`, `worlds`, `create`, `pause`, `options`, `controls`. |
| `SOLARIA_FLUID_BUDGET_CELLS` | Celdas de fluido simuladas por tick. |
| `SOLARIA_FLUID_BUDGET_MS` | Presupuesto de tiempo del autómata de fluidos. |
| `SOLARIA_TIME` | Hora inicial del ciclo dia/noche (0..1). |
| `SOLARIA_DAY_SPEED` | Acelera el ciclo (multiplicador; 1 = normal, 24 = un dia por 25 s). |
| `SOLARIA_DAY` | Dia de juego inicial (fases lunares). |
| `SOLARIA_LOOK` | Orientacion de la camara del demo (`yaw,pitch` en grados), p.ej. `96,18` para mirar al sol. |

Ejemplo:

```bash
SOLARIA_RIVER=1 SOLARIA_VIEW_RADIUS=8 SOLARIA_DEMO=1 cargo run
```

## Tests

```bash
cargo test
```

335 tests de unidad e integracion (determinismo, persistencia, meshing, luz,
fluidos estilo Minecraft, raycast, worldgen, cuevas, cielo/color, distancia de
vista, interfaz creativa, grafo de densidad, libreria de mundos, pantallas y
opciones). Lint:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

## Preview del worldgen (offline, PNG)

```bash
cargo run --release --example worldgen_preview -- <seed> <px> <bloques_por_px> <layer>
```

`layer`: `biome`, `height`, `continental`, `river`, `landform` (mapas cenitales),
`cave` (slice horizontal de cuevas a `y=30`) o `cave_yz` (slice vertical).

```bash
cargo run --release --example worldgen_preview -- 13371 512 6 landform
cargo run --release --example worldgen_preview -- 13371 320 4 cave
```

Galeria de semillas (mosaico de mapas de bioma):

```bash
cargo run --release --example seed_gallery
```

Preview del **cielo** (tira de 24 h + hemisferio completo, offline):

```bash
cargo run --release --example sky_preview -- 0.28
```

Capturas:

```powershell
powershell -ExecutionPolicy Bypass -File tools/screenshot.ps1 `
  -Exe .\target\debug\solaria_voxel.exe -Out .\screenshots\vX.Y.Z.png -WaitSeconds 8
```

Indice de capturas: [`screenshots/README.md`](./screenshots/README.md).

## Estructura del proyecto

```
src/
├── main.rs             Punto de entrada: solo llama a la libreria.
├── lib.rs              Documentacion general y lista de modulos.
├── engine/
│   ├── app.rs          ApplicationHandler: ventana + renderer + camara, eventos.
│   ├── demo.rs         Escenas de demostracion (env SOLARIA_*).
│   ├── input.rs        Estado de teclado y raton (ejes, delta).
│   ├── save_worker.rs  Hilo de guardado/carga del mundo (no bloquea el frame).
│   └── window.rs       Atributos de la ventana (tamano, titulo).
├── render/
│   ├── renderer.rs     Superficie, device, z-buffer, frame y edicion del mundo.
│   ├── pipeline.rs     Pipeline de escena (shader, vertices, uniforms, atlas).
│   ├── mesh.rs         Vertices + indices y su subida a la GPU.
│   ├── mesh_worker.rs  Meshing en hilos de trabajo con presupuesto por frame.
│   ├── highlight.rs    Resaltado wireframe del bloque apuntado.
│   ├── ui.rs / gui.rs  Interfaz 2D (hotbar, inventario) e iconos.
│   ├── sky.rs          Pase de cielo (triangulo a pantalla completa).
│   ├── sky.wgsl        Shader del cielo (gradiente + dithering).
│   ├── scene.wgsl      Shader de la escena (vertex + fragment, cutout).
│   ├── highlight.wgsl  Shader del resaltado.
│   ├── ui.wgsl         Shader de la interfaz 2D.
│   └── shaders/water.wgsl  Shader del agua (pase translucido).
├── player/
│   └── controller.rs   Fisica del jugador: gravedad, suelo, salto, vuelo.
├── scene/
│   ├── camera.rs       Camara FPS (posicion, yaw/pitch, matrices).
│   ├── daynight.rs     Hora del mundo y contador de dias.
│   └── sky.rs          Cielo/atmosfera: paleta, orbita solar y SkyState.
├── ui/                 Estado/logica de interfaz sin GPU (i18n, inventario).
�   +-- lang.rs         Traducciones es/en.
�   +-- inventory.rs    Categorias, busqueda y filtrado del inventario.
�   +-- options.rs      Opciones persistentes (options.json) + conflictos de teclas.
�   +-- screens.rs      Pila de pantallas (titulo, mundos, crear, opciones, pausa).
├── physics.rs          Fisica AABB de entidades (gravedad, colision, flotar).
├── math/
│   ├── vec3.rs         Vector de 3 componentes.
│   ├── mat4.rs         Matriz 4x4 column-major (perspectiva, look-at).
│   ├── color.rs        sRGB <-> lineal y mezcla perceptual en OKLab.
│   └── frustum.rs      Frustum de la camara (frustum culling).
└── world/
    ├── block.rs        Tipos de bloque (id) que delegan en el registro.
    ├── registry.rs     Registro central de bloques (metadata unica).
    ├── chunk.rs        Seccion 16^3 y columna 16x16x384.
    ├── atlas.rs        Atlas de texturas (carga assets/atlas.png; fallback).
    ├── terrain.rs      Generacion: geografia, clima/biomas, superficie, cuevas.
    ├── worldgen/       Motor de worldgen por etapas (config, math, cells, biomes,
    │                   decoration, graph: grafo de densidad DAG).
    ├── caves.rs        Cuevas 3D (spaghetti/cheese/pillar) con densidad por Y.
    ├── mesher.rs       Meshing naive con face culling (referencia).
    ├── greedy.rs       Greedy meshing (fusiona caras; separa el agua).
    ├── fluid_mesher.rs Meshing de la superficie de agua (altura por nivel).
    ├── mesh_snapshot.rs Foto inmutable de una seccion para meshear en hilos.
    ├── raycast.rs      Raycast de voxeles (que bloque se apunta).
    ├── recipe.rs       Recetas de crafteo (rejilla 3x3 -> resultado).
    ├── water.rs        Simulacion de agua (niveles, propagacion, 10 Hz).
    ├── streaming.rs    Carga/descarga de columnas por radio (StreamChange).
    ├── view.rs         ViewSettings (radios de render/simulacion/niebla).
    ├── memory.rs       Contabilidad de memoria del mundo por categorias.
    ├── bench.rs        Benchmarks reproducibles (solo tests).
    ├── save.rs         Versionado + guardado/carga del mundo (bincode + LZ4).
    ├── library.rs      Mundos multiples: saves/<slug>/ + level.json + importar.
    ├── generator.rs    GeneratorKind (coexistencia legacy/graph).
    └── store.rs        World: columnas en memoria + streaming + luz.
```

Otros directorios: `assets/` (ver [`assets/README.md`](./assets/README.md)),
`docs/`, `examples/worldgen_preview.rs`, `tools/` (generadores de atlas y
`tools/screenshot.ps1`), `screenshots/`.

Principio de diseno: **el resto del motor no sabe que wgpu existe**. Solo el
modulo `render` habla con la GPU.

## Documentacion

| Documento | Contenido |
| --------- | --------- |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | Como se organiza el motor por dentro; flujo de un frame; invariantes. |
| [`docs/worldgen.md`](./docs/worldgen.md) | Pipeline de generacion de mundo y estado de las fases. |
| [`docs/performance.md`](./docs/performance.md) | Benchmarks y memoria medidos, por fase. |
| [`DECISIONS.md`](./DECISIONS.md) | Registro historico de decisiones (que, por que, alternativas). |
| [`assets/README.md`](./assets/README.md) | Que es cada recurso y cuales son canonicos. |
| [`screenshots/README.md`](./screenshots/README.md) | Indice de capturas por version. |

## Convenios

- **Ejes**: mano derecha, +X derecha, +Y arriba, **-Z al frente**.
- **Matrices**: column-major (como wgpu/Vulkan/OpenGL), sin transponer.
- **Un bloque = un `u8`**; la metadata vive en `world/registry.rs`.
- **Determinismo**: `(seed, x, z)` da siempre el mismo resultado; sin RNG con estado.
- **Cero comentarios de relleno**: se comenta el *porque*, no el *que*.

## Roadmap (resumen)

| Etapa | Versiones | Hito | Estado |
| ----- | --------- | ---- | ------ |
| 0. Fundamentos | `v0.1.x`–`v0.3.x` | Ventana, camara, primer cubo, primer chunk, terreno | ✅ |
| 1. Mundo jugable | `v0.4.x`–`v0.7.x` | Romper/colocar, guardado, luz, biomas, cuevas, oceanos | ✅ |
| 2. Gameplay | `v0.8.x`–`v0.10.x` | Inventario, crafteo, mobs, guardado completo | parcial |
| 3. Optimizacion | `v0.11.x`–`v0.16.x` | Memoria, culling, timestep fijo, radio de vista | parcial |
| 4. Worldgen avanzado | `v0.17.x`–`v0.19.x` | Fases 1/2, 3 y 5 del generador por etapas | en curso |
| 5. Pulido | `v1.0.0` | Menus, audio, particulas, data packs, release | pendiente |

- La **Etapa 1** cerro en `v0.7.9`; las **Fases 7, 9–13** de la auditoría en
  `v0.16.1`. Pendiente del worldgen: decoracion por reglas (FASE 7), landforms
  (FASE 4) y tooling/metricas (FASE 9).

## Licencia

Dual: **MIT OR Apache-2.0**. Ver [`LICENSE`](./LICENSE) (y los textos completos
en [`LICENSE-MIT`](./LICENSE-MIT) y [`LICENSE-APACHE`](./LICENSE-APACHE)).
