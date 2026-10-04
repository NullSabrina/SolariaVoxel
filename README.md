# Solaria Voxel

Motor de voxeles escrito en Rust **desde cero**, sin motor de juego. Sobre una
capa de plataforma minima (`winit` + `wgpu`) construimos nosotros el bucle de
juego, la matematica, la camara y (en el futuro) el meshing, la iluminacion y el
guardado versionado del mundo.

> Objetivo a largo plazo: un mundo de voxeles jugable que consuma **< 500 MB de
> RAM**, construido en micro-versiones pequenas (cada una jugable y commiteada).

## Estado actual: `v0.1.2` — Primer cubo

- Camara FPS movible: WASD + raton (pointer lock), Espacio/Shift vertical.
- Un **cubo 3D** con color por cara, colocado delante de la camara.
- Pipeline de wgpu: shader WGSL, uniforms (matriz MVP) y **z-buffer**.
- Geometria generada a mano (24 vertices / 36 indices) y subida a la GPU.
- Suite de tests de matematica, camara e input (20 tests).

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
- **Click izquierdo**: captura el raton (mira con el raton).
- **W / A / S / D**: andar. **Espacio / Shift**: subir / bajar.
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
│   ├── renderer.rs  Envoltura sobre wgpu: superficie, device, z-buffer, frame.
│   ├── pipeline.rs  Pipeline: shader WGSL, layout de vertices, uniforms MVP.
│   ├── mesh.rs      Vertices + indices y su subida a la GPU (cubo).
│   └── scene.wgsl   Shader del cubo (vertex + fragment).
├── scene/
│   └── camera.rs    Camara FPS (posicion, yaw/pitch, matrices).
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
