# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara, el meshing, la iluminacion, el guardado
versionado del mundo y la generacion procedural.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.23.0` — Worldgen FASE 4: landforms

El relieve deja de ser uniforme: un **perfil de landform por region** se aplica
a la altura (`LandformProfile`):

- **Plateau** (mesetas de cima plana) en zonas secas y elevadas.
- **Terraced** (terrazas geologicas) en las regiones que marca un ruido propio.
- **Cliffs** (acantilados) en las montanas.
- **Rolling** (suave) en el resto.
- La transformacion es `terrace(h, step, sharpness)`, que cuantiza la altura en
  escalones; solo actua en **tierra**, con transicion suave en la costa.

`GENERATOR_VERSION → 14`. `WORLDGEN_CONFIG_VERSION → 2`.

- **Decoracion** (`v0.22.0`): reglas (`DecorationRule`), arboles con claros
  (clusters) y rocas; rejilla de muestras con padding.
- **Agua** (`v0.21.0`–`v0.21.1`): modelo de Minecraft (fuente→distancia),
  fuentes infinitas, caida; el agua **generada se asienta sola** al cargar la
  columna y el tick va a 10 Hz.
- **Worldgen** (`v0.17`–`v0.20`): fases 1/2 (continentes, costas, cordilleras),
  3 (bioma por region celular), 5 (hidrologia: rios y lagos) y 6 (cuevas
  jerarquicas). Ver [`docs/worldgen.md`](./docs/worldgen.md).
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
| `F3` | Alterna el **diagnostico** (estadisticas en el titulo). |
| `Escape` | Cierra inventario / libera el raton; si ya esta libre, cierra y guarda. |

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
| `SOLARIA_STATS` | Muestra estadisticas (draw calls, triangulos, memoria). |
| `SOLARIA_VIEW_RADIUS` | Radio de vista en columnas (niebla y culling atados). |
| `SOLARIA_FLUID_BUDGET_CELLS` | Celdas de fluido simuladas por tick. |
| `SOLARIA_FLUID_BUDGET_MS` | Presupuesto de tiempo del autómata de fluidos. |
| `SOLARIA_TIME` | Hora inicial del ciclo dia/noche. |

Ejemplo:

```bash
SOLARIA_RIVER=1 SOLARIA_VIEW_RADIUS=8 SOLARIA_DEMO=1 cargo run
```

## Tests

```bash
cargo test
```

258 tests de unidad e integracion (determinismo, persistencia, meshing, luz,
fluidos estilo Minecraft, raycast, worldgen y cuevas). Lint:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

## Preview del worldgen (offline, PNG)

```bash
cargo run --release --example worldgen_preview -- <seed> <px> <bloques_por_px> <biome|height|continental|river>
```

Ejemplo:

```bash
cargo run --release --example worldgen_preview -- 13371 512 4 biome
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
│   ├── color.rs        Conversion sRGB -> lineal para los colores de clear.
│   ├── scene.wgsl      Shader de la escena (vertex + fragment, cutout).
│   ├── highlight.wgsl  Shader del resaltado.
│   ├── ui.wgsl         Shader de la interfaz 2D.
│   └── shaders/water.wgsl  Shader del agua (pase translucido).
├── player/
│   └── controller.rs   Fisica del jugador: gravedad, suelo, salto, vuelo.
├── scene/
│   ├── camera.rs       Camara FPS (posicion, yaw/pitch, matrices).
│   └── daynight.rs     Hora del mundo, luz del sol y color del cielo.
├── physics.rs          Fisica AABB de entidades (gravedad, colision, flotar).
├── math/
│   ├── vec3.rs         Vector de 3 componentes.
│   ├── mat4.rs         Matriz 4x4 column-major (perspectiva, look-at).
│   └── frustum.rs      Frustum de la camara (frustum culling).
└── world/
    ├── block.rs        Tipos de bloque (id) que delegan en el registro.
    ├── registry.rs     Registro central de bloques (metadata unica).
    ├── chunk.rs        Seccion 16^3 y columna 16x16x384.
    ├── atlas.rs        Atlas de texturas (carga assets/atlas.png; fallback).
    ├── terrain.rs      Generacion: geografia, clima/biomas, superficie, cuevas.
    ├── worldgen/       Motor de worldgen por etapas (config, math, cells, biomes).
    ├── caves.rs        Cuevas 3D (spaghetti/cheese/pillar) con densidad por Y.
    ├── mesher.rs       Meshing naive con face culling (referencia).
    ├── greedy.rs       Greedy meshing (fusiona caras; separa el agua).
    ├── fluid_mesher.rs Meshing de la superficie de agua (altura por nivel).
    ├── mesh_snapshot.rs Foto inmutable de una seccion para meshear en hilos.
    ├── raycast.rs      Raycast de voxeles (que bloque se apunta).
    ├── recipe.rs       Recetas de crafteo (rejilla 3x3 -> resultado).
    ├── water.rs        Simulacion de agua (niveles, propagacion, 10 Hz).
    ├── streaming.rs    Carga/descarga de columnas por radio (StreamChange).
    ├── memory.rs       Contabilidad de memoria del mundo por categorias.
    ├── bench.rs        Benchmarks reproducibles (solo tests).
    ├── save.rs         Versionado + guardado/carga del mundo (bincode + LZ4).
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
