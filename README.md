# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara, el meshing, la iluminacion, el guardado
versionado del mundo y la generacion procedural.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.43.1` â€” Opciones persistentes + mundos y pantallas

- **Opciones** (`v0.43.1`): menÃº **Opciones** (desde tÃ­tulo y pausa) con distancia
  de render/simulaciÃ³n, niebla, FOV, sensibilidad, idioma, autoguardado y F3 al
  iniciar. Se guardan en `options.json` (global, atÃ³mico) y se **aplican en vivo**;
  la distancia reconstruye el mundo. La lÃ³gica de **reasignaciÃ³n de teclas**
  (`Options::rebind`) resuelve conflictos intercambiando, con tests (falta aÃºn la
  pantalla de controles para exponerla).
- **LibrerÃ­a de mundos** (`v0.42.0`): `saves/<slug>/` con `level.json` legible,
  escritura atÃ³mica, slug saneado/Ãºnico, renombrar/duplicar/eliminar e importaciÃ³n
  del `world.vf` antiguo. Semilla de texto con hash FNV-1a estable.
- **Pantallas** (`v0.42.0`, pila en `ui::screens`): tÃ­tulo, selector de mundos,
  crear mundo y pausa. `Esc` **ya no cierra el juego**; la X sigue guardando.
- **Astros texturizados** (`v0.35.0`, arte de LibreSprite), **grafo de densidad**
  (`v0.34.0`, C1), **UI creativa** (`v0.41.0`), **distancia de vista** (`v0.33.0`)
  y **cielo/atmosfera** (`v0.30`â€“`v0.31.1`).
- **Pendiente honesto**: pantalla de **controles** (reasignaciÃ³n de teclas en la
  UI), toolkit de widgets nine-slice y fuente con tildes en el render; integraciÃ³n
  del grafo en el terreno + clima (C2â€“C4); `sky_physical` opcional.
- **Paleta por fases** (`scene/sky.rs`): 7 bandas de la tabla de direccion de arte
  (noche profunda, crepusculos astronomico/nautico/civil, golden hour, manana/tarde,
  mediodia), mezcladas en **OKLab** con `smoothstep` para que recorrer 24 h no de
  saltos. `SkyState` es la **unica fuente de verdad** del color del cielo, la
  niebla, el `day_factor` y el tinte de luz; la CPU resuelve todo y el shader solo
  reconstruye el rayo de vista.
- **Sol y luna 3D, estrellas** llegan en `v0.31`.
- **Personaje y mano** (`v0.28`/`v0.29`): mano en primera persona y humanoide en
  tercera persona (`F5`).

`GENERATOR_VERSION`/`FORMAT_VERSION` intactos (16/5).

- **Overlay F3** (`v0.27.0`): pantalla de diagnostico en pantalla (dos columnas)
  con una **fuente bitmap 5x7** propia.
- **Interpolacion de render** (`v0.26.0`) y agua estilo Minecraft
  (`v0.21.0`â€“`v0.21.1`).
- **Worldgen** (`v0.17`â€“`v0.25`, FASE 4/6/7/9 + tuning). Ver
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
| `1`â€“`9` / rueda | Elige la ranura de la hotbar. |
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
| `SOLARIA_VIEW_RADIUS` | Radio de render en columnas (2â€“32; por defecto 12). |
| `SOLARIA_SIM_RADIUS` | Radio de simulacion de fluidos (1â€“12; por defecto 6). |
| `SOLARIA_FOG` | Modo de niebla: `off`, `far`, `normal` (def.), `short`. |
| `SOLARIA_INVENTORY` | Abre el inventario creativo al arrancar (capturas). |
| `SOLARIA_SEARCH` | Texto de busqueda inicial del inventario (abre el inventario). |
| `SOLARIA_TOAST` | Muestra el nombre del bloque de esa ranura sobre la hotbar. |
| `SOLARIA_HOME` | Directorio base de los mundos (`saves/` cuelga de aqui). |
| `SOLARIA_SCREEN` | Arranca en una pantalla: `title`, `worlds`, `create`, `pause`, `options`. |
| `SOLARIA_FLUID_BUDGET_CELLS` | Celdas de fluido simuladas por tick. |
| `SOLARIA_FLUID_BUDGET_MS` | Presupuesto de tiempo del autÃ³mata de fluidos. |
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

325 tests de unidad e integracion (determinismo, persistencia, meshing, luz,
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
â”œâ”€â”€ main.rs             Punto de entrada: solo llama a la libreria.
â”œâ”€â”€ lib.rs              Documentacion general y lista de modulos.
â”œâ”€â”€ engine/
â”‚   â”œâ”€â”€ app.rs          ApplicationHandler: ventana + renderer + camara, eventos.
â”‚   â”œâ”€â”€ demo.rs         Escenas de demostracion (env SOLARIA_*).
â”‚   â”œâ”€â”€ input.rs        Estado de teclado y raton (ejes, delta).
â”‚   â”œâ”€â”€ save_worker.rs  Hilo de guardado/carga del mundo (no bloquea el frame).
â”‚   â””â”€â”€ window.rs       Atributos de la ventana (tamano, titulo).
â”œâ”€â”€ render/
â”‚   â”œâ”€â”€ renderer.rs     Superficie, device, z-buffer, frame y edicion del mundo.
â”‚   â”œâ”€â”€ pipeline.rs     Pipeline de escena (shader, vertices, uniforms, atlas).
â”‚   â”œâ”€â”€ mesh.rs         Vertices + indices y su subida a la GPU.
â”‚   â”œâ”€â”€ mesh_worker.rs  Meshing en hilos de trabajo con presupuesto por frame.
â”‚   â”œâ”€â”€ highlight.rs    Resaltado wireframe del bloque apuntado.
â”‚   â”œâ”€â”€ ui.rs / gui.rs  Interfaz 2D (hotbar, inventario) e iconos.
â”‚   â”œâ”€â”€ sky.rs          Pase de cielo (triangulo a pantalla completa).
â”‚   â”œâ”€â”€ sky.wgsl        Shader del cielo (gradiente + dithering).
â”‚   â”œâ”€â”€ scene.wgsl      Shader de la escena (vertex + fragment, cutout).
â”‚   â”œâ”€â”€ highlight.wgsl  Shader del resaltado.
â”‚   â”œâ”€â”€ ui.wgsl         Shader de la interfaz 2D.
â”‚   â””â”€â”€ shaders/water.wgsl  Shader del agua (pase translucido).
â”œâ”€â”€ player/
â”‚   â””â”€â”€ controller.rs   Fisica del jugador: gravedad, suelo, salto, vuelo.
â”œâ”€â”€ scene/
â”‚   â”œâ”€â”€ camera.rs       Camara FPS (posicion, yaw/pitch, matrices).
â”‚   â”œâ”€â”€ daynight.rs     Hora del mundo y contador de dias.
â”‚   â””â”€â”€ sky.rs          Cielo/atmosfera: paleta, orbita solar y SkyState.
â”œâ”€â”€ ui/                 Estado/logica de interfaz sin GPU (i18n, inventario).
│   ├── lang.rs         Traducciones es/en.
│   ├── inventory.rs    Categorias, busqueda y filtrado del inventario.
│   ├── options.rs      Opciones persistentes (options.json) + conflictos de teclas.
│   └── screens.rs      Pila de pantallas (titulo, mundos, crear, opciones, pausa).
â”œâ”€â”€ physics.rs          Fisica AABB de entidades (gravedad, colision, flotar).
â”œâ”€â”€ math/
â”‚   â”œâ”€â”€ vec3.rs         Vector de 3 componentes.
â”‚   â”œâ”€â”€ mat4.rs         Matriz 4x4 column-major (perspectiva, look-at).
â”‚   â”œâ”€â”€ color.rs        sRGB <-> lineal y mezcla perceptual en OKLab.
â”‚   â””â”€â”€ frustum.rs      Frustum de la camara (frustum culling).
â””â”€â”€ world/
    â”œâ”€â”€ block.rs        Tipos de bloque (id) que delegan en el registro.
    â”œâ”€â”€ registry.rs     Registro central de bloques (metadata unica).
    â”œâ”€â”€ chunk.rs        Seccion 16^3 y columna 16x16x384.
    â”œâ”€â”€ atlas.rs        Atlas de texturas (carga assets/atlas.png; fallback).
    â”œâ”€â”€ terrain.rs      Generacion: geografia, clima/biomas, superficie, cuevas.
    â”œâ”€â”€ worldgen/       Motor de worldgen por etapas (config, math, cells, biomes,
    â”‚                   decoration, graph: grafo de densidad DAG).
    â”œâ”€â”€ caves.rs        Cuevas 3D (spaghetti/cheese/pillar) con densidad por Y.
    â”œâ”€â”€ mesher.rs       Meshing naive con face culling (referencia).
    â”œâ”€â”€ greedy.rs       Greedy meshing (fusiona caras; separa el agua).
    â”œâ”€â”€ fluid_mesher.rs Meshing de la superficie de agua (altura por nivel).
    â”œâ”€â”€ mesh_snapshot.rs Foto inmutable de una seccion para meshear en hilos.
    â”œâ”€â”€ raycast.rs      Raycast de voxeles (que bloque se apunta).
    â”œâ”€â”€ recipe.rs       Recetas de crafteo (rejilla 3x3 -> resultado).
    â”œâ”€â”€ water.rs        Simulacion de agua (niveles, propagacion, 10 Hz).
    â”œâ”€â”€ streaming.rs    Carga/descarga de columnas por radio (StreamChange).
    â”œâ”€â”€ view.rs         ViewSettings (radios de render/simulacion/niebla).
    â”œâ”€â”€ memory.rs       Contabilidad de memoria del mundo por categorias.
    â”œâ”€â”€ bench.rs        Benchmarks reproducibles (solo tests).
    â”œâ”€â”€ save.rs         Versionado + guardado/carga del mundo (bincode + LZ4).
    â”œâ”€â”€ library.rs      Mundos multiples: saves/<slug>/ + level.json + importar.
    â””â”€â”€ store.rs        World: columnas en memoria + streaming + luz.
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
| 0. Fundamentos | `v0.1.x`â€“`v0.3.x` | Ventana, camara, primer cubo, primer chunk, terreno | âœ… |
| 1. Mundo jugable | `v0.4.x`â€“`v0.7.x` | Romper/colocar, guardado, luz, biomas, cuevas, oceanos | âœ… |
| 2. Gameplay | `v0.8.x`â€“`v0.10.x` | Inventario, crafteo, mobs, guardado completo | parcial |
| 3. Optimizacion | `v0.11.x`â€“`v0.16.x` | Memoria, culling, timestep fijo, radio de vista | parcial |
| 4. Worldgen avanzado | `v0.17.x`â€“`v0.19.x` | Fases 1/2, 3 y 5 del generador por etapas | en curso |
| 5. Pulido | `v1.0.0` | Menus, audio, particulas, data packs, release | pendiente |

- La **Etapa 1** cerro en `v0.7.9`; las **Fases 7, 9â€“13** de la auditorÃ­a en
  `v0.16.1`. Pendiente del worldgen: decoracion por reglas (FASE 7), landforms
  (FASE 4) y tooling/metricas (FASE 9).

## Licencia

Dual: **MIT OR Apache-2.0**. Ver [`LICENSE`](./LICENSE) (y los textos completos
en [`LICENSE-MIT`](./LICENSE-MIT) y [`LICENSE-APACHE`](./LICENSE-APACHE)).
