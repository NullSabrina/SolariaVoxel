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
| `worldgen/decoration.rs` | `DecorationRule` (reglas) + `Decorator`: **rocas** en laderas altas. |
| `worldgen/trees.rs` | `TreePlacer` (Fase D): **4 especies** con copa procedimental y colocacion por **coordenada global** con margen. |
| `worldgen/mod.rs` | `WorldGen`, `TerrainSample`, `LandClass`, seeds y ensamblado de altura/clima/hidrologia. |
| `worldgen/larion/` | **Generador Larion** (MEGA PROMPT 4): pipeline multi-capa de escala monumental. Ver seccion propia. |
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
| 7 | Decoracion: **rocas** por reglas; **arboles** procedimentales (`trees.rs`, 4 especies). | `IMPLEMENTED` |
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

Auditoria de arboles/hojas (Fase A): cuenta arboles/km2, alturas de tronco,
hojas embebidas, arboles en bordes y repeticion por chunk, y exporta un mapa
cenital (`screenshots/tree_audit_<seed>.png`):

```bash
cargo run --release --example tree_audit -- 24
```

Escenas demo del motor: `SOLARIA_OCEAN`, `SOLARIA_BIOMES`, `SOLARIA_RIVER`,
`SOLARIA_CAVE` (ver `README.md`).

## Versiones

- `GENERATOR_VERSION` (resultado del mundo) y `FORMAT_VERSION` (binario) son
  **independientes**. Cambiar el mundo sube `GENERATOR_VERSION`; cambiar el
  guardado sube `FORMAT_VERSION` con migrador + test.
- Actual: `GENERATOR_VERSION = 21`, `FORMAT_VERSION = 6`,
  `WORLDGEN_CONFIG_VERSION = 4`, `LARION_CONFIG_VERSION = 1`.

## Generador Larion (MEGA PROMPT 4)

Tercer camino de generacion (`GeneratorKind::Larion`, `level.json`), que **no
toca** `Legacy16` ni `Graph`. Coordenadas de mundo → `LarionSample` es una
funcion **pura** de `(seed, x, z)`. Modulos en `world/worldgen/larion/`:

| Modulo | Responsabilidad |
| ------ | --------------- |
| `larion/noise.rs` | `ScalarField2D` (trait), `Fractal2D`, `Ridged2D`, `Fractal3D`, `Warp2D` sobre `OpenSimplex` gradiente. |
| `larion/spline.rs` | `Spline` monotona (Fritsch-Carlson), exacta en nodos y con **extrapolacion constante**. |
| `larion/config.rs` | `LarionConfig` versionada + validacion + `LARION_CONFIG_VERSION`. |
| `larion/climate.rs` | `ClimatePoint` (vector de 5 parametros). |
| `larion/erosion.rs` | Campo de erosion -> amplitud de relieve, curvas de detalle y de crestas. |
| `larion/height.rs` | `compose_height` (puro) y `LarionSample`. |
| `larion/density.rs` | Densidad 3D **en banda** (`|y-H| < band`) para voladizos. |
| `larion/biome.rs` | `BiomeSelector` multi-parametrico + `BiomeBlend`. |
| `larion/rivers.rs` | Cauces sinuosos; profundidad creciente en montana. |

Pipeline (seccion 4 del prompt):

1. **Domain warping horizontal** (solo X/Z) sobre las coordenadas.
2. **Continentalidad** (`Fbm`, ~1/4000) -> spline `[-1,1] -> altura`.
3. **Erosion** (campo continuo) escala `relief_amplitude = lerp(MAX, MIN, erosion)`.
4. **Crestas** `RidgedMulti` (curva de normalizacion propia) + macro + valles `(1-|n|)^p`.
5. **Densidad 3D en banda** alrededor de `H` (voladizos solo en montana joven).
6. **Clima en bandas**: temperatura latitudinal (eje Z) + ruido + lapse; humedad con
   sesgo costero simetrico.
7. **Biomas** por distancia ponderada a 7 nodos (continuo, sin Voronoi).
8. **Rios** con warp propio; `depth = base * lerp(1, 2.5, mountain)`.

Materiales (seccion 7): roca en laderas (`rock_slope`) y cumbres `> 200`
(`Stone`/`Snow` segun temperatura); transicion de bioma por `BiomeBlend` + hash
global. Techos: `LARION_MAX_HEIGHT = 300` (el legacy conserva 200).

Medido (`examples/larion_preview.rs`, area de 20480 bloques): rango
`p99-p01 = 176..192`, cumbres `> 200` (hasta ~294), 7 biomas y ninguno > 38 %.
Coste: ~1.44x legacy por columna (`cargo test --release -- --ignored
el_coste_por_columna`).

```bash
cargo run --release --example larion_preview -- <seed> <px> <bloques_por_px> <layer>
cargo run --release --example larion_preview -- 13371 512 40 height
cargo run --release --example larion_preview -- 13371 512 6 density
```

## Bioma unico y superficie (MEGA PROMPT 1, Fases B/C)

- **Un solo `biome_at`**: ambos caminos (legacy y grafo) usan `WorldGen`
  (`terrain::biome_at`). El canal de **clima del grafo** (C4) se retiro.
- **Ecotono**: en el borde de la celda de bioma (`cell_edge`) la capa superior
  mezcla un sustrato de transicion dithered por ruido (no un parche cuadrado).
- **Superficie por pendiente** (`config.rs`): `rock_slope` aflora roca en laderas
  y `sediment_height`/`sediment_chance` ponen sedimento en los valles. El camino
  grafo calcula la pendiente real de una rejilla de altura con padding.

## Arboles procedimentales (MEGA PROMPT 1, Fase D)

`worldgen/trees.rs` define 4 especies (roble, picea, abedul, acacia) elegidas por
bioma y ruido regional. La copa es un **elipsoide** (o cono, para la picea) con el
radio perturbado por un Perlin 3D y recorte por distancia al tronco.

- **Decision pura global**: `TreePlacer::plan(wx, wz, ...)`; todos los hash usan
  `(wx, wz)` globales.
- **Colocacion con margen (camino A)**: cada chunk recorre `-MARGIN..16+MARGIN`
  (`MARGIN = 4`) y dibuja solo su parte; un arbol que cruza una frontera se
  genera entero en ambos chunks. El tronco tiene prioridad sobre las hojas.
- **Contrato de hojas**: `visible && !solid` (se atraviesan) pero
  `blocks_fluid() == true` (el agua no las borra) y el mesher solo emite sus caras
  contra aire.
- **Deuda**: tronco inclinado de la acacia y variantes de tile de hoja (exigen
  atlas/registro).

## Grafo de densidad (Parte C)

Desde v0.34/v0.44 el worldgen puede definirse como **datos** (un grafo DAG) en vez
de codigo cableado:

- `world/worldgen/graph.rs`: `Node` (const, ruido fBm 2D/3D, add/mul/min/max,
  clamp, abs, spline, `YGradient`, `Warp`, `Cache2D`), arena `Graph`,
  `validate()`/`compile()` (orden topologico) y evaluador determinista. Se
  serializa a **JSON** (`Graph::to_json`/`from_json`).
- `GeneratorKind::{Legacy16, Graph}` (en `level.json`): coexisten sin migrar el
  binario. Mundos nuevos con `SOLARIA_GENERATOR=graph`.
- Camino `Graph`: la **densidad 3D** (`default_density_graph`:
  `superficie - y + cueva`) se evalua en una **retícula gruesa 4x4x4** e interpola
  trilinealmente (`terrain::sample_density`). El **bioma ya no sale del grafo**:
  lo decide `WorldGen` (`biome_at`), la misma fuente que el legacy (Fase B).
- Preview offline: `cargo run --example graph_preview -- <seed> <px> <bpp>`.
- Pendiente: rios/acuifero del grafo; `Cache2D` y evaluacion en retícula del resto
  de canales.
