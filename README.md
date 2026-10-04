# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara y (en el futuro) el meshing, la iluminacion y el
guardado versionado del mundo.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.4.1` — Greedy meshing

- **Greedy meshing**: las caras contiguas del mismo material se fusionan en
  rectangulos. Una capa plana de 16x16 pasa de 256 caras a **1**. Las 48
  columnas vecinas quedan en ~1 344 triangulos.
- Al editar un bloque se regenera en greedy solo la **seccion** afectada.
- Sobre v0.4.0: raycast + resaltado naranja + romper/colocar, colisiones, vuelo.
- Suite de tests (53 tests).

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
- **Click derecho** (capturado): **coloca** un bloque al lado del apuntado.
- **W / A / S / D**: andar. **Espacio**: saltar.
- **F**: alterna modo vuelo (Espacio/Shift sube/baja).
- **Escape**: libera el raton; si ya esta libre, cierra la aplicacion.

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
│   ├── scene.wgsl   Shader de la escena (vertex + fragment).
│   └── highlight.wgsl Shader del resaltado (color plano).
├── player/
│   └── controller.rs Fisica del jugador: gravedad, suelo, salto, vuelo.
├── scene/
│   └── camera.rs    Camara FPS (posicion, yaw/pitch, matrices).
├── world/
│   ├── block.rs     Tipos de bloque y su tile del atlas.
│   ├── chunk.rs     Seccion 16^3 y columna 16x16x384.
│   ├── atlas.rs     Atlas de texturas procedural (pixels por codigo).
│   ├── terrain.rs   Generacion de altura con ruido Perlin.
│   ├── mesher.rs    Meshing naive con face culling (referencia).
│   ├── greedy.rs    Greedy meshing (fusiona caras; el que se usa).
│   └── raycast.rs   Raycast de voxeles (que bloque se apunta).
└── math/
    ├── vec3.rs      Vector de 3 componentes.
    └── mat4.rs      Matriz 4x4 column-major (perspectiva, look-at).
```

Principio de diseno: **el resto del motor no sabe que wgpu existe**. Solo el
modulo `render` habla con la GPU.

## Convenios

- **Ejes**: mano derecha, +X derecha, +Y arriba, **-Z al frente**.
- **Matrices**: column-major (como wgpu/Vulkan/OpenGL), sin transponer.
- **Cero comentarios de relleno**: se comenta el *porque*, no el *que*.

## Roadmap (resumen)

| Etapa | Versiones | Hito |
| ----- | --------- | ---- |
| 0. Fundamentos | `v0.1.x`–`v0.3.x` | Ventana, camara, primer cubo, primer chunk, terreno |
| 1. Mundo jugable | `v0.4.x`–`v0.7.x` | Romper/colocar, versionado de mundo, luz, biomas |
| 2. Gameplay | `v0.8.x`–`v0.10.x` | Inventario, crafteo, mobs, guardado completo |
| 3. Optimizacion | `v0.11.x`–`v0.13.x` | Bit-packing, LOD, culling, < 500 MB |
| 4. Multijugador | `v0.14.x`–`v0.16.x` | Cliente-servidor, QUIC, replicacion |
| 5. Pulido | `v0.17.x`–`v1.0.0` | Menus, audio, particulas, data packs, release |

Las decisiones tecnicas se registran en [`DECISIONS.md`](./DECISIONS.md).
