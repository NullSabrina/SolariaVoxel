# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara, el meshing, la iluminacion, el guardado
versionado del mundo y mas.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.17.1` — Fix: overrun del buffer de malla

`Mesh::new` creaba los buffers GPU con el tamaño **exacto** de los datos pero
registraba `capacity = next_power_of_two()` (mayor). Al re-meshear una sección que
crecía dentro de ese rango, `Mesh::update` creía que cabía y `write_buffer` se
salía del buffer (`wgpu Validation Error`) → el juego se cerraba al caminar hacia
tierra. El nuevo worldgen (mallas de tamaño más variable) lo destapó. Ahora el
buffer se crea con la capacidad reservada; test de la invariante `capacidad >= bytes`.

## v0.17.0 — Worldgen por etapas (FASE 1/2)

Primer rediseño del generador de mundo (auditoría de worldgen). Nuevo módulo
`world/worldgen/` con:

- `WorldGenConfig` central + validación y `WORLDGEN_CONFIG_VERSION`.
- Seeds derivadas por campo (sin RNG con estado) y helpers de math.
- Muestreador **celular (Worley)** determinista con id estable por celda.
- **Continentalness** con domain warping y clasificación
  `DeepOcean..Interior`; **costas de ancho variable** por celda; altura base
  continental + relieve macro + cordilleras (máscara de rango + cresta) + valles.
- Preview offline:

```bash
cargo run --release --example worldgen_preview -- 13371 512 4
```

Medido (2048×2048 bloques): océano ~31–55 % según seed (mar abisal incluido),
tierra el resto, picos hasta el techo de mundo. El relieve ya no depende del
bioma; el bioma sigue por clima (FASE 3 pendiente). `GENERATOR_VERSION` sube a 9.

- Hereda de **v0.16.1**: persistencia end-to-end.
- Fases 7, 9–13 de la auditoría completadas. Pendiente del worldgen: bioma por
  región celular (FASE 3), hidrología/ríos (FASE 5), cuevas jerárquicas (FASE 6),
  decoración por reglas (FASE 7), previews/benchmarks (FASE 9).

Variables: `SOLARIA_DEMO`, `SOLARIA_OCEAN`, `SOLARIA_BIOMES`, `SOLARIA_STATS`,
`SOLARIA_VIEW_RADIUS`, `SOLARIA_FLUID_BUDGET_CELLS/_MS`.

Controles: `1`-`9`/rueda = ranura, `E` = inventario, `F` = volar, **`F3` =
diagnóstico**, click izq = romper, click der = colocar. Variables: `SOLARIA_DEMO`,
`SOLARIA_OCEAN`, `SOLARIA_STATS`, `SOLARIA_FLUID_BUDGET_CELLS/_MS`.

Al cerrar con **Escape** o la **X** de la ventana, el mundo se guarda en
`world.vf` (junto al ejecutable). La proxima vez se carga automaticamente.

## Requisitos

- Rust stable (probado con `1.98.1`). Instala desde <https://rustup.rs>.
- Una GPU con soporte Vulkan / Direct3D12 / Metal.

## Como ejecutar

```bash
cargo run
```

La primera compilacion tarda unos minutos (compila `wgpu` y sus dependencias).
Veras en consola la GPU y el formato de superficie elegidos.

Controles:
- **Click izquierdo** (sin captura): captura el raton.
- **Click izquierdo** (capturado): **rompe** el bloque apuntado.
- **Click derecho** (capturado): **coloca** el bloque de la ranura activa.
- **1 / 2 / ... / 9** o **rueda**: elige la ranura de la hotbar.
- **E**: abre/cierra el **inventario** (click para asignar un bloque a la ranura).
- **W / A / S / D**: andar. **Espacio**: saltar / nadar.
- **F**: alterna modo vuelo (Espacio/Shift sube/baja).
- **Escape**: cierra el inventario / libera el raton; si ya esta libre, cierra.

## Tests

```bash
cargo test
```

## Estructura del proyecto

```
src/
├── main.rs          Punto de entrada: solo llama a la libreria.
├── lib.rs           Documentacion general y lista de modulos.
├── engine/
│   ├── app.rs       ApplicationHandler: ventana + renderer + camara, eventos.
│   ├── input.rs     Estado de teclado y raton (ejes de movimiento, delta).
│   └── window.rs    Atributos de la ventana (tamano, titulo).
├── render/
│   ├── renderer.rs  Superficie, device, z-buffer, frame y edicion del mundo.
│   ├── pipeline.rs  Pipeline de escena (shader, vertices, uniforms, atlas).
│   ├── highlight.rs Pipeline del resaltado (wireframe del bloque apuntado).
│   ├── mesh.rs      Vertices + indices y su subida a la GPU.
│   ├── color.rs     Conversion sRGB -> lineal para los colores de clear.
│   ├── scene.wgsl   Shader de la escena (vertex + fragment, cutout).
│   └── highlight.wgsl Shader del resaltado (color plano).
├── player/
│   └── controller.rs Fisica del jugador: gravedad, suelo, salto, vuelo.
├── scene/
│   ├── camera.rs    Camara FPS (posicion, yaw/pitch, matrices).
│   └── daynight.rs  Hora del mundo, luz del sol y color del cielo.
├── world/
│   ├── block.rs     Tipos de bloque (id) que delegan en el registro.
│   ├── registry.rs  Registro central de bloques (metadata unica).
│   ├── memory.rs    Contabilidad de memoria del mundo por categorias.
│   ├── bench.rs     Benchmarks reproducibles (solo tests).
│   ├── chunk.rs     Seccion 16^3 y columna 16x16x384.
│   ├── atlas.rs     Atlas de texturas (carga assets/atlas.png; fallback).
│   ├── terrain.rs   Generacion: geografia, clima/biomas, superficie, cuevas.
│   ├── worldgen/    Motor de worldgen por etapas (config, math, cells, mod).
│   ├── caves.rs     Cuevas 3D (spaghetti/cheese/pillar) con densidad por Y.
│   ├── mesher.rs    Meshing naive con face culling (referencia).
│   ├── greedy.rs    Greedy meshing (fusiona caras; separa el agua).
│   ├── raycast.rs   Raycast de voxeles (que bloque se apunta).
│   ├── recipe.rs    Recetas de crafteo (rejilla 3x3 -> resultado).
│   ├── water.rs     Simulacion de agua (niveles, propagacion, 10 Hz).
│   ├── save.rs      Versionado + guardado/carga del mundo (bincode).
│   └── store.rs     World: columnas en memoria + streaming por radio.
├── physics.rs       Fisica AABB de entidades (gravedad, colision, flotar).
└── math/
    ├── vec3.rs      Vector de 3 componentes.
    ├── mat4.rs      Matriz 4x4 column-major (perspectiva, look-at).
    └── frustum.rs   Frustum de la camara (frustum culling).
```

Para el detalle de como encajan (flujo de un frame, versionado del mundo,
invariantes), ver [`ARCHITECTURE.md`](./ARCHITECTURE.md).

Principio de diseno: **el resto del motor no sabe que wgpu existe**. Solo el
modulo `render` habla con la GPU.

## Convenios

- **Ejes**: mano derecha, +X derecha, +Y arriba, **-Z al frente**.
- **Matrices**: column-major (como wgpu/Vulkan/OpenGL), sin transponer.
- **Cero comentarios de relleno**: se comenta el *porque*, no el *que*.

## Roadmap (resumen)

| Etapa | Versiones | Hito | Estado |
| ----- | --------- | ---- | ------ |
| 0. Fundamentos | `v0.1.x`–`v0.3.x` | Ventana, camara, primer cubo, primer chunk, terreno | ✅ |
| 1. Mundo jugable | `v0.4.x`–`v0.7.x` | Romper/colocar, versionado de mundo, luz, biomas | ✅ (+ cuevas, oceanos, culling, streaming) |
| 2. Gameplay | `v0.8.x`–`v0.10.x` | Inventario, crafteo, mobs, guardado completo | ⏳ siguiente |
| 3. Optimizacion | `v0.11.x`–`v0.13.x` | Bit-packing, LOD, culling, < 500 MB | parcial (culling hecho en v0.7.4) |
| 4. Multijugador | `v0.14.x`–`v0.16.x` | Cliente-servidor, QUIC, replicacion | pendiente |
| 5. Pulido | `v0.17.x`–`v1.0.0` | Menus, audio, particulas, data packs, release | pendiente |

Notas:
- La **Etapa 1** quedo cerrada en `v0.7.9` (cuevas `v0.7.5`, oceanos `v0.7.8`,
  vegetacion/arboles `v0.7.9`); el **culling** de la Etapa 3 se adelanto a
  `v0.7.4`.
- La **Etapa 2** empieza en `v0.8.x`: hotbar/inventario, crafteo, mobs y
  **guardado completo** (hoy no se guarda la posicion del jugador).

Las decisiones tecnicas se registran en [`DECISIONS.md`](./DECISIONS.md).
