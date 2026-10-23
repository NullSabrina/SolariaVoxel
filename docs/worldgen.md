# Generacion de mundo — Solaria Voxel

El worldgen vive en `src/world/worldgen/` y **no toca la GPU**. Es puramente
determinista: `(seed, x, z)` da siempre el mismo resultado, sin RNG con estado
(prerequisito para generar columnas en varios hilos, `Send + Sync`).

## Generadores

| `generator_kind` | Implementacion | Version | Uso / compatibilidad |
|---|---|---|---|
| `legacy16` | `worldgen/` por etapas + `terrain.rs` | 21 | Por defecto. Mundos existentes creados antes de `graph`/`larion`. |
| `graph` | `worldgen/graph.rs` (DAG de densidad) | 21 | Seleccionable al crear (`SOLARIA_GENERATOR=graph`). Rios/acuifero del camino graph siguen pendientes de validacion visual. |
| `larion` | `worldgen/larion/` (pipeline multiescala) | 21 | Seleccionable al crear (`SOLARIA_GENERATOR=larion`). Biomas multi-parametricos y rios sinuosos. |

El tipo se guarda en `saves/<slug>/level.json` (`generator_kind`) y **no cambia**
al reabrir el mundo: un mundo existente conserva su generador. La version del
**pipeline** Larion vive en `generator_config` (1 = historico, 2 = hidrologia
P0.1); los `level.json` antiguos sin el campo se leen como 1. `GENERATOR_VERSION`
es global (`22`) y sube cuando cambia el terreno de cualquier camino; los mundos
antiguos se siguen leyendo y el terreno ya generado no se regenera. Cambiar de
generador solo afecta a zonas nuevas.

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
- Actual: `GENERATOR_VERSION = 22`, `FORMAT_VERSION = 7`,
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
- **Contrato de hojas**: `solid == true` (no se atraviesan) pero
  `occludes() == false` (cutout, no ocultan la cara vecina) y
  `blocks_fluid() == true` (el agua no las borra); el mesher solo emite sus caras
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

## Agua superficial y biomas (config 4, v0.53)

`generator_config = 4` separa el **origen** del agua superficial de su cota. Antes
un unico `surface_water: f32` mezclaba mar, rio, lago y tierra seca; ahora
`LarionSample::water_kind` distingue:

- `Ocean`: conectado a la costa, cota de mar.
- `River`: dentro del cauce (`dist <= width`), cota ligada al relieve final.
- `Lake`: cuerpo embalsado por priority-flood en clima humedo/frio o altura.
- `Wetland`: agua somera de pantano.
- `None`: tierra seca (se fuerza `surface_water = 0`).

Invariantes que el escritor de bloques y `water_audit` comprueban:

1. Una columna `None` no recibe agua aunque la hidrologia tenga una cota cercana.
2. Un `Ocean` solo aparece si `landness < 0.25` y la altura esta en/bajo el mar.
3. Un `River` mantiene la lamina por encima del lecho; si el bioma baja el
   relieve, la cota del rio baja con el.
4. Un `Lake` exige agua por encima del terreno y un clima/altura que lo justifique.
5. El agua de cuevas/acuiferos se escribe en su propio dominio y no se mezcla con
   la clasificacion superficial.

Ademas, config 4 descarta componentes de agua de **menos de 24 celdas**
(~384 m²) para que un charco de priority-flood no cuente como lago.

El **relieve por bioma** (`biome::height_modifier`) se aplica despues de elegir
el bioma y antes de fijar la cota del rio; los lagos conservan su cota de
priority-flood (un pantano mas bajo tiene agua mas profunda, no una lamina
flotante).

Contenido por bioma:

- `Block::Flower`: cutout, no solida, dos planos cruzados (tile 21). Se coloca
  deterministicamente en llanura/bosque/pantano, sobre hierba/podzol y pendiente
  suave, solo en tierra seca.
- El agua usa el tile del bioma (10 normal, 19 pantano, 20 frio). La eleccion se
  guarda en `Column::water_tile` al generar y el snapshot de meshing la lee sin
  volver a muestrear el mundo.

Herramientas:

```bash
cargo run --release --example larion_preview -- 13371 512 6 water 4
cargo run --release --example water_audit -- 13371 2048 32 4
cargo run --release --example water_audit -- 13371 512 32 4 3d
```

`water_audit` imprime el reparto por origen, el agua interior y las anomalias
(`seco_con_cota`, `rio_bajo_lecho`, `oceano_alto`, `no_finito`). Con `3d` genera
columnas reales y comprueba que el agua escrita coincide con `WaterKind`. En la
semilla 13371, config 4 paso de ~34% de agua interior a ~9,5% (muestra 8192/64).
