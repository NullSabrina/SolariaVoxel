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
| `fluid_tick` (charca 16x16) | 0.238 ms/tick | un tick de agua |
| `save_load_column` | record 0.435 ms · `save_to` 4.908 ms · `load_from` 1.539 ms | comprimir/guardar/cargar una columna |

Benchmarks heredados (`world::store::tests`):

| benchmark | tiempo | notas |
|---|---|---|
| carga de 81 columnas (`warm_streaming`) | 291.9 ms | una vez al arrancar |
| `recompute_skylight` (81 col) | 20.7 ms | recalculo **regional** de luz de cielo |
| `recompute_skylight` (3x3) | 3.9 ms | región tras una edición |
| `recompute_block_light` (global) | 20.1 ms | **cuello pendiente**: se usa en cada cambio de streaming |
| `update_streaming` (cruce, +9/-9) | 22.6 ms | generación síncrona en el benchmark |
| greedy 27 columnas (125 secciones) | 37.7 ms | meshing CPU de una región |

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

## Cuellos pendientes

1. **Luz de bloque en cambios de streaming** (~19–20 ms/cruce): sigue siendo un
   recálculo global al entrar/salir columnas. Candidato a incrementar como la de
   cielo (regional por fronteras).
2. **Luz de cielo** (~3.9 ms por edición regional): funcional, pero no es
   incremental puro.
3. **Generación de terreno** (~3.9 ms/columna, ~292 ms para 81 columnas): es
   asíncrona en la app, pero el `warm_streaming` inicial paga el coste completo.
4. **Meshing de una región** (~38 ms para 125 secciones): repartido por
   presupuesto entre frames, pero es el mayor coste durante el streaming.
5. Sin **batching por columna/material** ni **LOD**: draw calls bajarían aún más
   al subir el radio (FASE 11 pendiente).
6. Sin **render interpolado** para el timestep fijo (FASE 12 pendiente).
