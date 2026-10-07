# Generacion de mundo — Solaria Voxel

El worldgen vive en `src/world/worldgen/` y **no toca la GPU**. Es puramente
determinista: `(seed, x, z)` da siempre el mismo resultado, sin RNG con estado
(prerequisito para generar columnas en varios hilos, `Send + Sync`).

## Pipeline

```
WorldGen (seed)
  │  seeds derivadas por campo (no hay un RNG global)
  │  ruido: continental, macro-relieve, cordillera, valle, warp, clima, rio
  │  celular (Worley) para regiones
  │
  ├─ sample(x, z) -> TerrainSample
  │     continentalness, LandClass, celda, costa, altura base,
  │     temperatura, humedad, bioma, river_proximity, surface_water
  v
TerrainGenerator::generate_column(x, z) -> Box<Column>
  │  altura = spline(continental) + macro + cordilleras(mascara·cresta)
  │           + valles − cauce(río)     (clamp MIN_HEIGHT..MAX_HEIGHT)
  │  superficie por bioma/material; cuevas 3D; relleno de agua hasta
  │  surface_water (mar/rio/lago)
  v
Column (24 secciones de 16^3 + luz de cielo / luz de bloque)
```

## Modulos

| Modulo | Responsabilidad |
| ------ | --------------- |
| `worldgen/config.rs` | `WorldGenConfig` central + validacion + `WORLDGEN_CONFIG_VERSION`. |
| `worldgen/math.rs` | Helpers puros: `smoothstep`, `remap`, `spline`, etc. |
| `worldgen/cells.rs` | Muestreador celular (Worley) determinista; `CellSample` con id estable por celda. |
| `worldgen/biomes.rs` | `BiomeDefinition` (rangos de clima/altura, densidad) y `select` por scoring con bandas suaves. |
| `worldgen/decoration.rs` | `DecorationRule` (reglas) + `Decorator`: arboles con claros (clusters) y rocas. |
| `worldgen/mod.rs` | `WorldGen`, `TerrainSample`, `LandClass`, seeds y ensamblado de altura/clima/hidrologia. |
| `world/terrain.rs` | Convierte el `TerrainSample` a bloques; superficie, cuevas, `Biome`. |
| `world/caves.rs` | Cuevas 3D (spaghetti/cheese/pillar) con densidad por profundidad. |

## Fases (estado honesto)

| Fase | Que | Estado |
| ---- | --- | ------ |
| 1 | Fundacion: config, seeds, math. | `IMPLEMENTED` |
| 2 | Celular + continentes + costas + relieves. | `IMPLEMENTED` |
| 3 | Bioma por region celular + lapse de altitud. | `IMPLEMENTED` |
| 5 | Hidrologia (rios serpenteantes + lagos). | `IMPLEMENTED` |
| 6 | Cuevas jerarquicas (spaghetti, regionales, `cheese`, pozos, canones, pilares, entradas). | `IMPLEMENTED` |
| 7 | Decoracion por reglas: arboles con claros (clusters) y rocas, con rejilla de muestras. | `IMPLEMENTED` |
| 4 | Landforms: mesetas (`Plateau`), terrazas (`Terraced`) y acantilados (`Cliffs`). | `IMPLEMENTED` (sin overhangs 3D) |
| 9 | Tooling: previews de cuevas, seed gallery, metricas, benchmarks. | parcial (`worldgen_preview`) |

## Herramientas

Preview offline (PNG): mapas cenitales (`biome`, `height`, `continental`,
`river`, `landform`) y slices de cuevas (`cave` horizontal a `y=30`, `cave_yz`
vertical). Imprime metricas (oceanos, alturas p50/p95/p99, agua superficial,
aire subterraneo, reparto de biomas y landforms).

```bash
cargo run --release --example worldgen_preview -- <seed> <px> <bloques_por_px> <layer>
cargo run --release --example worldgen_preview -- 13371 512 6 landform
cargo run --release --example worldgen_preview -- 13371 320 4 cave
```

Galeria de semillas (mosaico de mapas de bioma de 8 seeds):

```bash
cargo run --release --example seed_gallery
```

Escenas demo del motor: `SOLARIA_OCEAN`, `SOLARIA_BIOMES`, `SOLARIA_RIVER`,
`SOLARIA_CAVE` (ver `README.md`).

## Versiones

- `GENERATOR_VERSION` (resultado del mundo) y `FORMAT_VERSION` (binario) son
  **independientes**. Cambiar el mundo sube `GENERATOR_VERSION`; cambiar el
  guardado sube `FORMAT_VERSION` con migrador + test.
- Actual: `GENERATOR_VERSION = 15`, `FORMAT_VERSION = 5`.
