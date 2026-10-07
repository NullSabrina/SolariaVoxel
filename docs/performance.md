# Rendimiento — Solaria Voxel

Medidas reproducibles del motor, por fase de la auditoría. Los números son de la
máquina de desarrollo (Windows, **AMD Radeon Graphics**, backend **Vulkan**) y de
una build **`dev` con `opt-level = 1`** para el crate propio (las dependencias van
a `opt-level = 3`); sirven como línea base **relativa**, no como umbral absoluto.

Cómo reproducir:

```bash
cargo test --release -- --nocapture bench_
```

(En `--release` los tiempos son menores; en `dev` son una cota superior estable.)

## Benchmarks (FASE 13)

Salida de `bench_*` (dev, `opt-level = 1`):

| benchmark | tiempo | qué mide |
|---|---|---|
| `terrain_generate_column` | 3.900 ms/columna | generar una columna 16x16x384 |
| `mesh_greedy_section` | 2.801 ms / 5 secciones | greedy + fluido de las secciones no vacías de una columna |
| `lighting_incremental` | 0.997 ms/edición | colocar/quitar antorcha (relight de bloque **incremental**) |
| `fluid_tick` (charca 16x16) | 0.027 ms/tick | un tick de agua (v0.21.0; era 0.238 ms con el modelo viejo) |
| `save_load_column` | record 0.435 ms · `save_to` 4.908 ms · `load_from` 1.539 ms | comprimir/guardar/cargar una columna |

Benchmarks heredados (`world::store::tests`):

| benchmark | tiempo | notas |
|---|---|---|
| carga de 81 columnas (`warm_streaming`) | 291.9 ms | una vez al arrancar |
| `recompute_skylight` (81 col) | 20.7 ms | recalculo **regional** de luz de cielo |
| `recompute_skylight` (3x3) | 3.9 ms | región tras una edición |
| `recompute_block_light` (frio) | 21.2 ms | primera vez (construye la cache de emisores) |
| `recompute_block_light` (global, cache) | 12.0 ms | tras cachear los emisores (era 19.03 ms, −37 %) |
| `recompute_block_light_region` (cruce, app) | **10.7 ms** | ruta real: solo limpia/reconstruye la **región** de las columnas que entran/salen |
| `update_streaming` (cruce, +9/-9) | 22.6 ms | generación síncrona en el benchmark |
| greedy 27 columnas (125 secciones) | 37.7 ms | meshing CPU de una región |

### Optimizacion v0.19.1 — greedy sin despacho dinamico

`greedy_*` recibia las consultas de bloque/luz como `&dyn Fn`, lo que hacia una
llamada **indirecta** por celda de mascara (~74 k por seccion). Pasarlas a
**genericos** (`impl Fn`) deja al compilador inlinar las closures que leen la
columna. Medido en `dev` (opt-level 1), misma maquina:

| benchmark | antes | despues | ratio |
|---|---|---|---|
| `mesh_greedy_section` (4 secciones no vacias) | 2.088 ms | 1.564 ms | **-25 %** |
| greedy 27 columnas (110 secciones) | 33.60 ms | 23.45 ms | **-30 %** |

### FASE 6 — coste de las cuevas jerarquicas

`terrain_generate_column` (dev, opt-level 1), mismo equipo:

| version | tiempo |
|---|---|
| v0.19.1 (2 campos 3D) | 3.74 ms/columna |
| v0.20.0 (FASE 6) | ~6.3 ms/columna |

El sistema jerarquico evalua `tubes_a` + `regional` siempre, y activa `cheese`,
`shaft` y `canyon` solo donde su contexto 2D (`CaveContext`) lo permite;
`tubes_b` se evalua cerca de la banda cero de `tubes_a` (donde se cruzan) y
`pillar` solo cuando algun sistema ya propone cavar. Coste ~1.7x a cambio de
cuevas mucho mas variadas. La generacion es asincrona; el `warm_streaming`
inicial (81 columnas) pasaria de ~291 ms a ~470 ms.

### v0.21.0 — agua estilo Minecraft

Al pasar de un igualador que **conservaba volumen** al modelo **fuente→distancia**
de Minecraft, la charca alcanza el **equilibrio** y sus celdas se saltan (coste 0
por tick), asi que el tick se abarata mucho:

| benchmark | antes (v0.20) | despues (v0.21) | ratio |
|---|---|---|---|
| `fluid_tick` (charca 16x16) | 0.238 ms/tick | 0.027 ms/tick | **~9x** |
| `bench_tick_agua` (100 ticks) | 4.55 ms | 0.54 ms | **~8x** |

El tick sube a 20 Hz (Minecraft usa 0.25 s por paso) para que el flujo sea agil.

### FASE 7 — decoracion por reglas

El `terrain_generate_column` queda practicamente igual (~6.1 ms/columna): la
**rejilla de muestras 18x18** anade muestreos, pero elimina el re-muestreo de 4
vecinos por cada candidato de decoracion que hacia el antiguo `slope_ok`. El
ruido 2D sigue siendo O(256) por columna (no O(256 x altura)).

### FASE 4 — landforms

El perfil de landform anade **un ruido 2D por muestra** (dentro de la rejilla), mas
una transformacion `terrace`. `terrain_generate_column` queda igual (~5.9 ms/
columna): el coste lo domina la generacion de columnas (cuevas FASE 6), no el
perfil.

## Memoria (FASE 10)

`World::memory_report()` con radio 4 (**81 columnas**), mundo de terreno:

| categoría | tamaño |
|---|---|
| bloques (`u8`/voxel) | 7.6 MB |
| luz de cielo | 7.6 MB |
| luz de bloque (dispersa) | 6.2 MB |
| fluido (disperso) | 0.0 MB |
| cabeceras `Column` | ~0.04 MB |
| **total** | **~21.5 MB** (~265 KB/columna) |

Además, **GPU**: ~24 MB de buffers de malla (capacidad reservada con holgura) para
las columnas cargadas a radio 4 (dato del overlay F3: `mundo 22.0MB gpu 24.2MB`).

Medidas tomadas: la **luz de bloque dispersa** (v0.11.0) evita reservar 98 KB por
columna sin emisores y convierte `clear_block_light` en un `None` (antes: `memset`
de 98 KB por columna en cada cambio de streaming). El bit-packing de bloques/luz
(paleta 1/2/4/8 bits) **no** se aplicó: la medición no compensa el riesgo/CPU sin
benchmarks que lo respalden.

## Draw calls y culling (FASE 11)

Vista de océano (`SOLARIA_DEMO=1 SOLARIA_OCEAN=1`), radio 4, con `SOLARIA_STATS=1`:

```
con culling por distancia:  dc=128  tri=36986  cull_frustum=178  cull_dist=164
sin culling por distancia:  dc≈292  (128 + 164)
```

El **culling por distancia** (sección totalmente dentro de la niebla) reduce los
draw calls ~56 %. El frustum ya descartaba 178 secciones. A 663 fps de render.

## Escalado del radio de vista (`SOLARIA_VIEW_RADIUS`)

La niebla y el culling por distancia se atan al radio, asi que subirlo alarga la
vista. Vista de oceano, `SOLARIA_STATS=1` (dev, opt-level 1):

| radio | columnas | memoria mundo | draw calls | triángulos | render |
|---|---|---|---|---|---|
| 4 (def.) | 81 | ~22 MB | 128 | 37 k | ~2 ms |
| 6 | 169 | ~44 MB | 340 | 132 k | ~2.4 ms |
| 8 | 289 | ~76 MB | 530 | 187 k | ~2.4 ms |

La memoria crece ~lineal con las columnas (~265 KB/columna); el coste de render
apenas cambia porque el **culling por distancia** limita lo visible a la esfera de
niebla. No hay un cuello de draw calls a estos radios (530 dc es trivial).
Por eso **no** se implemento batching/LOD: seria optimizacion prematura sin un
caso medido que lo justifique.

## Cuellos pendientes

1. **Luz de bloque en cambios de streaming** (~10.7 ms/cruce): la cache de
   emisores (v0.15.1) quitó el barrido de secciones y el recálculo **regional**
   (v0.15.2) limita la limpieza a la región afectada; lo que queda es el **BFS de
   propagación** desde los emisores (con lava por todas partes, es casi constante
   al radio). Aun así escala mejor (O(perímetro) en vez de O(área)).
2. **Luz de cielo** (~3.9 ms por edición regional): funcional, pero no es
   incremental puro.
3. **Generación de terreno** (~3.9 ms/columna, ~292 ms para 81 columnas): es
   asíncrona en la app, pero el `warm_streaming` inicial paga el coste completo.
4. **Meshing de una región** (~38 ms para 125 secciones): repartido por
   presupuesto entre frames, pero es el mayor coste durante el streaming.
5. Sin **batching por columna/material** ni **LOD**: draw calls bajarían aún más
   al subir el radio (FASE 11 pendiente).
6. Sin **render interpolado** para el timestep fijo (FASE 12 pendiente).
