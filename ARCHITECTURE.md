# Arquitectura de Solaria Voxel

Este documento explica **como esta organizado el motor por dentro**: que hace
cada modulo, como fluyen los datos en un frame, y que invariantes se asumen. Es
el companero tecnico del [`README.md`](./README.md) (que es la vista de usuario)
y de [`DECISIONS.md`](./DECISIONS.md) (el registro historico de por que se eligio
cada cosa).

## Principio rector

**El resto del motor no sabe que `wgpu` existe.** Solo el modulo `render` habla
con la GPU. Si algun dia se cambia de API grafica, es el unico modulo que se
reescribe. Lo mismo con `winit` (input/ventana): vive aislado en `engine`.

La unica dependencia de plataforma que se filtra a proposito es el tipo
`Vertex` (en `render::mesh`), porque el mesher del mundo lo construye. Es un
`struct` de datos planos (`#[repr(C)]`), no una abstraccion de GPU.

## Mapa de modulos

```
main.rs ──> lib.rs ──> engine::run()
                        │
        ┌───────────────┼───────────────────────────┐
        v               v                           v
     engine          scene (Camera)             render
   winit/eventos     posicion + yaw/pitch      wgpu: superficies,
        │            matrices view/proy        pipelines, mallas
        │               │                           │
        └──────┬────────┴───────────┬───────────────┘
               v                    v
             player               world
        gravedad/suelo      bloques, chunks, meshing,
        colision jugador    raycast, guardado
               │                    │
               └────────┬───────────┘
                        v
                      math
              Vec3, Mat4 (propios, sin glam)
```

| Modulo | Responsabilidad | Depende de |
| ------ | --------------- | ---------- |
| `engine` | Ciclo de vida de la app, eventos de winit, input, ventana. | `render`, `scene`, `player`, `world`, `math` |
| `render` | Todo lo que toca `wgpu`: superficie, pipelines, mallas, shaders e **interfaz 2D** (hotbar/inventario). | `world` (para meshear), `scene`, `math` |
| `scene` | Que hay en la escena: la camara FPS, el ciclo dia/noche y el **cielo** (paleta, orbita solar y `SkyState`, todo puro y testeable). | `math` |
| `ui` | Estado y **logica de interfaz sin GPU**: i18n (`lang`) y reglas del inventario creativo (categorias, busqueda, filtrado). | `world` (tipos) |
| `player` | Fisica del jugador: vertical (gravedad/salto/vuelo), colision horizontal y test de solape bloque/jugador. | `scene`, `world` (tipos), `math` |
| `world` | Datos del mundo: bloques, columnas, meshing, raycast, guardado. | `render::mesh` (el tipo `Vertex`), `math` |
| `math` | Matematica 3D propia (`Vec3`, `Mat4`). | ninguna |

## Flujo de un frame

Todo arranca en `engine::app::App`, que implementa `ApplicationHandler` de winit
0.30. No hay un `while`: winit llama a los callbacks y el bucle vive dentro de
`event_loop.run_app`.

```
resumed()                 window_event(RedrawRequested)
  crear ventana             │
  crear Renderer            ├─ App::update(dt)      fisica del jugador + camara
  cargar/migrar mundo       │   + DayCycle::advance  (hora del mundo)
  posar jugador (settle)    │   + move_horizontal     (colision en X-Z)
  [modo demo] escena fija   ├─ App::update_selection() raycast -> resaltado
  [modo demo] escena fija   └─ camera.view_projection()
                                let sky = SkyState::at(&day_cycle, &params)  (CPU)
                                renderer.set_sky(&sky)
                                renderer.sync_streaming(camara)
                                renderer.render(&view_projection, camara, &sky_basis, ...)
                                      │
                                      ├─ world.update_streaming() carga/descarga columnas
                                      ├─ reconstruir mallas de columnas nuevas
                                      ├─ escribir uniform (MVP + day_factor + niebla)
                                      └─ render pass: limpiar + CIELO + mallas +
                                         resaltado + mano/personaje + UI + present
```

Puntos clave:

- **Streaming**: `World::update_streaming` devuelve un `StreamChange` con las
  columnas que entran y salen. Se encolan las mallas de esas columnas **y las de
  sus vecinas de borde** (`columns_to_remesh`) y se meshean con un **presupuesto
  de tiempo por frame** (`pump_meshing`, ~6 ms), empezando por las cercanas al
  jugador: asi descubrir chunks no da un tiron (antes se mesheaba todo de golpe y
  el pico era ~80 ms). Las caras de borde dependen de si el vecino esta cargado,
  por eso al entrar/salir una columna hay que re-meshear las colindantes.
- **Meshing rapido**: el greedy usa una mascara plana reutilizada y lee los
  bloques de la propia columna directamente (solo los bordes van al chunk vecino),
  evitando `HashMap` y reservas por capa.
- **Una malla por (columna, seccion)**: una columna tiene 24 secciones; la
  mayoria estan vacias y se **saltan sin meshear** (`World::section_is_empty`);
  solo se guardan las que tienen geometria (`ColumnMeshes = [Option<Mesh>; 24]`).
- **Dos luces**: la **de cielo** se calcula en `World::recompute_skylight`: base
  **columnar** (15 hasta el primer solido) + propagacion **lateral** (BFS a nivel
  de mundo) para el aire bajo un techo (cuevas/voladizos); se recalcula por
  **region** (columnas sucias + anillo 3x3). La **de bloque**
  (`World::recompute_block_light`) es un BFS de antorchas que **cruza chunks**. El
  vertice lleva ambas por separado; el shader dibuja
  `max(cielo * day_factor, bloque)`, de modo que la noche apaga el sol pero no las
  antorchas.
- **Culling**: el pipeline usa **back-face culling** (la geometria mira hacia
  fuera; las antorchas emiten sus dos orientaciones) y en `render` se descartan
  las secciones cuyo AABB queda fuera del **frustum** (`math::Frustum`).
- **Niebla**: el fragment shader funde con el color del cielo por distancia
  (`fog_start`/`fog_end`), lo que da profundidad y disimula el borde del area
  cargada.
- **Cielo**: una sola fuente de verdad, `scene::SkyState`, resuelve en CPU el
  gradiente, la luz, la niebla y los astros a partir de la hora. El **pase de
  cielo** (`render/sky.rs` + `sky.wgsl`) es un triangulo a pantalla completa que
  se dibuja **antes** del mundo, sin escribir profundidad; reconstruye el rayo de
  vista desde la base de la camara (sin invertir matrices) y no lleva tablas de
  keyframes en WGSL.

## El pipeline de datos del mundo

```
WorldGen (semilla)  ->  config + seeds derivadas + ruido (continental, macro,
      │                  cordillera, valle, warp) + celular (Worley)
      │  sample(x,z) -> TerrainSample (continentalness, LandClass, celda,
      │                  costa, altura base)
      v
TerrainGenerator
      │  genera_column(x,z) -> altura del TerrainSample + clima/bioma + superficie
      │                          + cuevas (Perlin 3D, iso-superficie) + acuifero
      v
   Column (24 x Chunk de 16^3, + arrays de luz cielo/bloque)
      │  greedy::greedy_section_query(query, light, section, origin)
      │  (face culling + fusion de caras contiguas del mismo tile y luz)
      v
   Vec<Vertex> + Vec<u32>  ──>  render::mesh::Mesh  ──>  GPU
```

- `query(x,y,z)` y `light(x,y,z)` reciben **coordenadas locales** de la columna
  que pueden salirse a -1 o 16: asi las caras de borde consultan la columna
  vecina y no aparecen muros internos entre chunks.
- El **greedy** fusiona caras contiguas del mismo `(block, face, light)`. La
  antorcha no entra en el greedy: se emite aparte como dos quads cruzados
  (`mesher::emit_torch_cross`), porque es geometria no cubica.
- El shader hace **cutout** (descarta `alpha < 0.5`): asi la antorcha y las
  hojas muestran su fondo transparente sin blending ni ordenar triangulos.

## Versionado del mundo

`world::save` implementa el esquema que pide la guia (seccion 5):

- **`WorldHeader`**: `magic` (`VFWD`), `format_version`, `generator_version`,
  `engine_version`, `seed`, `created_at`. Es lo primero que se lee y decide si
  hay que migrar.
- **`ChunkRecord`**: los bloques de un chunk editado, con su propia
  `format_version` y `generator_version`. Desde v2 los bytes van comprimidos con
  LZ4.
- **`WorldSave`**: la cabecera + los chunks editados + la **posicion del jugador**
  (desde el formato v3; los mundos v2 se leen con la posicion por defecto via el
  espejo `WorldSaveV2`).
- **`FORMAT_VERSION`** (layout binario) y **`GENERATOR_VERSION`** (algoritmo de
  terreno) son **independientes**: un mundo viejo puede seguir generando igual
  aunque cambie el formato de archivo.
- **`MigrationChain`**: aplica migradores en cadena (`v1->v2`, ...) hasta el
  formato actual. Un migrador solo nace cuando el formato cambia de verdad.

Regla: **los datos que el jugador creo (bloques editados) nunca se pierden en
una migracion**; hay tests que lo verifican.

## Invariantes y convenios

- **Ejes**: mano derecha. +X derecha, +Y arriba, **-Z al frente**. "Mirar al
  frente" es `(0, 0, -1)`.
- **Matrices**: column-major (como wgpu/Vulkan/OpenGL); `to_cols_array()` va
  directo a la GPU sin transponer.
- **Un bloque = un `u8`** en el chunk (1 byte/voxel; 16^3 = 4096 bytes).
- **`is_solid` vs `is_visible`**: `is_solid` = colisiona y ocluye caras;
  `is_visible` = se dibuja pero no bloquea (antorcha, **liquidos** y **hojas**). El
  mesher dibuja `is_solid || is_visible` y solo oculta una cara si el vecino es
  solido. Las hojas, ademas, son **no solidas** y con **cutout** (huecos de alfa
  0): se atraviesan y se ven por sus huecos.
- **Agua translucida**: el agua es visible no solida y, ademas, se **separa** en
  el greedy a su propio buffer; el renderer la dibuja en un **pase aparte** con
  blending alfa y sin escritura de z (`ScenePipeline::water_pipeline`). Los oceanos
  se generan rellenando de agua el aire bajo el **nivel del mar** (`SEA_LEVEL`),
  con playas de arena en las columnas a ras de agua.
- **El raycast** golpea `is_solid || is_visible` (se puede apuntar la antorcha).
- **Luz**: `u8` por celda (0..15) para cielo y para bloque, por separado. En el
  vertice van normalizadas a 0..1 (`sky`, `block`); el `day_factor` (0..1) las
  combina en el shader.
- **Colision del jugador**: cilindro vertical de radio `PLAYER_RADIUS` y alto
  `PLAYER_HEIGHT`; `player::block_overlaps_player` decide si un bloque lo ocupa.
  La fisica vertical sondea la **huella completa** (`footprint_columns`), no un
  punto, y hay **auto-escalon** (`STEP_HEIGHT`) para subir pasos de 1 bloque.
- **Colision del jugador (horizontal)**: eje a eje; si un eje choca, se cancela y
  el otro desliza. Con auto-escalon, un escalon de <= 1 bloque se sube andando.
- **Cero comentarios de relleno**: se documenta el *porque*, no el *que*.

## Como se prueba

- `cargo test`: tests de unidad por modulo (logica pura, sin GPU).
- `cargo clippy --all-targets`: cero warnings.
- `cargo fmt`.
- Verificacion visual: `SOLARIA_DEMO=1` + `tools/screenshot.ps1` (ver README).

Lo que **no** se testea automaticamente (requiere GPU/ventana) se verifica con
capturas: el render, el pipeline y la integracion de eventos.

## Donde iria cada cosa nueva

- Logica de bloques/chunks/meshing/raycast -> `world`.
- Estado del jugador (inventario, salud, colision) -> `player`.
- Camara, entidades, iluminacion de escena -> `scene`.
- **Cielo y atmosfera** (paleta, orbita solar/lunar, fases, estrellas, clima
  celeste) -> `scene` (estado puro, testeable sin GPU); su **pase** de dibujo ->
  `render` (`render/sky.rs` + `sky.wgsl`). Conversion de color -> `math/color.rs`.
- **Mundos en disco** (crear/listar/renombrar/duplicar/eliminar, `level.json`,
  importar el `world.vf` antiguo) -> `world::library`. **Flujo de pantallas**
  (titulo/seleccion/crear/pausa) -> `ui::screens` (logica pura) + `engine::app`
  (orquestacion) + `render` (dibujo).
- Interfaz 2D (HUD, hotbar, inventario, mesa, overlay F3) -> `render`
  (`render::ui` + `render::gui` + `render::font`); el **estado/logica** de UI
  (idioma, categorias, busqueda, reglas del inventario) -> `ui` (sin GPU,
  testeable), y las recetas en `world::recipe`.
- Nuevos efectos visuales (particulas) -> `render` (o un `render::vfx`).
- Un sistema de juego (crafteo, IA) -> un modulo nuevo al mismo nivel.
- Cualquier cosa que necesite `wgpu` -> solo dentro de `render`.
