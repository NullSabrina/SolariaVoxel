# Decisiones de diseño (DECISIONS.md)

Registro de decisiones tecnicas de Solaria Voxel. Cada entrada explica **que**
se decidio, **por que** y **que alternativas** se descartaron. Se anade una
entrada por cada decision relevante, no por cada linea de codigo.

Formato: `## [fecha] vX.Y.Z — Titulo`

---

## v0.1.0 — Base del proyecto

### 2026-10-04 — El motor se escribe desde cero, sin motor de juego

**Decision.** No usamos Bevy, ni Fyrox, ni Godot. Construimos el motor nosotros.

**Motivo.** Es un proyecto de aprendizaje: queremos entender y controlar cada
pieza (bucle de juego, matematica, GPU, meshing, versionado de mundo). Un motor
completo esconde justo lo que queremos aprender.

**Consecuencia.** Asumimos mas trabajo y mas codigo propio. A cambio, cada linea
esta documentada y bajo nuestro control.

### 2026-10-04 — Capa de plataforma: `winit` + `wgpu` (no OpenGL crudo)

**Decision.** Para ventana y GPU usamos `winit` (ventana/eventos) y `wgpu`
(abstraccion de GPU). No escribimos Vulkan/D3D12 directamente.

**Motivo.**
- Escribir el binding nativo de Vulkan/D3D12/Metal desde cero seria meses de
  trabajo ajeno al objetivo (el motor de voxeles), no mas aprendizaje del motor.
- `wgpu` es *una capa de bajo nivel*, no un motor: nos da buffers, pipelines y
  shaders, y nos obliga a escribir el renderizado a mano. Sigue siendo
  "desde cero" a nivel de motor.
- Multiplataforma (Vulkan en Windows/Linux, Metal en macOS) con un solo codigo.

**Alternativas descartadas.** `softbuffer` + rasterizador propio (demasiado
lento y no representa lo que hara un motor real); OpenGL crudo con `glow`
(API antigua, peor encaje futuro con WebGPU).

### 2026-10-04 — Separacion por modulos `engine` / `render` / `scene` / `math`

**Decision.** El codigo se organiza en cuatro modulos con responsabilidades
claras, y el renderer es una **envoltura fina** sobre wgpu.

**Motivo.** Que la logica del juego no sepa de wgpu. Si algun dia cambiamos de
API grafica, solo cambia `render`. Facilita testear `math` y `scene` sin GPU.

### 2026-10-04 — Matematica propia en lugar de `glam`

**Decision.** Implementamos `Vec3` y `Mat4` nosotros, en `src/math`.

**Motivo.** Entender de verdad matrices, proyeccion y camara. Es la base que
usaremos para transformar chunks y para el frustum culling (v0.11.2).

**Nota.** Si el rendimiento de nuestra matematica se vuelve un problema, se
puede sustituir por `glam` mas adelante sin tocar el resto del motor: los
modulos solo dependen de la interfaz de `math`.

### 2026-10-04 — Convenio de ejes: mano derecha, -Z al frente, column-major

**Decision.**
- Sistema de mano derecha: +X derecha, +Y arriba, **-Z al frente**.
- Matrices en **column-major**, listas para subir a la GPU sin transponer.

**Motivo.** Encaja con Minecraft, con la convencion de wgpu (profundidad en
`[0,1]`) y con `raw-window-handle`. Documentado en el modulo `math` para no
volver a decidirlo.

### 2026-10-04 — Color de cielo definido en sRGB y convertido a lineal

**Decision.** Elegimos el azul cielo en espacio sRGB (como se ve en pantalla) y
lo convertimos a espacio lineal antes de pasarlo a `LoadOp::Clear`.

**Motivo.** Preferimos un formato de superficie sRGB (lo forzamos si esta
disponible). En ese caso la GPU espera colores lineales y aplica ella la
correccion de gamma; pasar valores sRGB directos daria un color mas claro del
intencionado.

### 2026-10-04 — Perfiles de compilacion: dependencias optimizadas en `dev`

**Decision.** En `[profile.dev]` ponemos `opt-level = 1` para nuestro codigo, y
`[profile.dev.package."*"] opt-level = 3` para las dependencias.

**Motivo.** Una build de `wgpu` sin optimizar va a pocos FPS y hace que el
desarrollo se sienta roto. Optimizar solo las dependencias da fluidez sin
sacrificar tiempos de compilacion de nuestro crate.

---

## v0.1.1 — Camara FPS basica

### 2026-10-04 — El input se desacopla de la camara

**Decision.** La camara no lee eventos de winit. Un modulo `engine::input`
mantiene el estado (teclas pulsadas, delta del raton) y la camara solo recibe
valores ya resueltos: `add_look(dx, dy)` y `walk(forward, right, up, dt)`.

**Motivo.** Testear la camara sin arrancar una ventana, y poder cambiar
"raton+teclado" por otra fuente de input (mando, red) sin tocar la camara.

### 2026-10-04 — Pointer lock opt-in, liberado con Escape y al perder foco

**Decision.** El cursor se captura solo al hacer click, y se libera con
`Escape` o cuando la ventana pierde el foco (alt-tab). Hacer click otra vez
recaptura.

**Motivo.** Secuestrar el raton nada mas abrir es hostil (no puedes ni mover la
ventana). Liberar al perder foco evita dejar el cursor atrapado al alt-tab.

### 2026-10-04 — Movimiento por delta time, con `dt` limitado a 0.1 s

**Decision.** El desplazamiento es `velocidad * dt` (no por frame). El `dt` se
limita a 0.1 s como maximo.

**Motivo.** El mismo comportamiento a 30 o 144 FPS. El limite evita el
"teletransporte" cuando el proceso se congela (arrastrar la ventana, un
breakpoint) y vuelve con un `dt` enorme.

### 2026-10-04 — Teclas por codigo fisico (`PhysicalKey`)

**Decision.** WASD se detecta por la posicion fisica de la tecla, no por la
letra.

**Motivo.** Que la disposicion del teclado (AZERTY, Dvorak) no rompa los
controles. `KeyCode::KeyW` significa "la tecla que esta donde la W", no la letra.

---

## v0.1.2 — Primer cubo

### 2026-10-04 — `bytemuck` para convertir structs a bytes

**Decision.** Anadimos `bytemuck` (con `derive`) y marcamos `Vertex`/`Uniforms`
como `Pod` con `#[repr(C)]`.

**Motivo.** Subir vertices y uniforms a la GPU exige reinterpretar memoria como
bytes. `bytemuck` lo hace sin `unsafe` manual y falla en compilacion si el
struct tiene padding inesperado. Escribirlo a mano seria mas fragil.

### 2026-10-04 — Z-buffer desde el principio

**Decision.** El primer pipeline ya incluye una textura de profundidad
(`Depth32Float`) y `DepthStencilState`.

**Motivo.** Sin z-buffer, las caras traseras del cubo se dibujarian encima de
las delanteras y el resultado seria incorrecto. Es mas barato meterlo ahora que
retrofitearlo cuando ya hay muchos objetos.

### 2026-10-04 — Back-face culling desactivado (por ahora)

**Decision.** `cull_mode: None`; no descartamos caras traseras todavia.

**Motivo.** El z-buffer ya resuelve la oclusion, y asi no dependemos del orden
(CCW/CW) de los vertices, que es una fuente clasica de errores. Lo activaremos
al hacer meshing de chunks, donde el ahorro si importa.

### 2026-10-04 — MVP en un solo uniform; el renderer es duena del modelo

**Decision.** El uniform contiene una unica matriz `mvp`. La `App` pasa al
renderer la `view_projection` de la camara; el renderer la multiplica por su
propia matriz `model` del cubo.

**Motivo.** Mantiene la camara fuera del renderer (la `scene`/`App` manda la
vista) y, a la vez, cada objeto puede tener su `model`. Cuando haya muchos
objetos moveremos el `model` a datos por instancia; el `mvp` es suficiente para
un objeto.

---

## v0.2.0 — Primer chunk estatico

### 2026-10-04 — Bloques como `u8`, chunk de 4096 bytes

**Decision.** Cada bloque es un `enum` con `#[repr(u8)]`; un chunk guarda
4096 de ellos en un array plano.

**Motivo.** La memoria es un objetivo del proyecto (< 500 MB). 1 byte/bloque es
el minimo razonable y deja claro el coste. En v0.11.0 anadiremos una paleta
(por si un chunk usa pocos tipos) para bajar de 1 byte/bloque.

### 2026-10-04 — Face culling en el mesher (en vez de un cubo por voxel)

**Decision.** El mesher no emite los 6 cubos por bloque: solo emite las caras
que dan al aire.

**Motivo.** La guia describia "1 cubo por voxel visible" como paso naive, pero
descartar caras ocultas es igual de sencillo y reduce la geometria a una
fraccion (un chunk de terreno pasa de ~147k a ~3.2k triangulos). No tiene
sentido generar lo que nunca se ve.

### 2026-10-04 — Atlas de texturas generado por codigo (sin assets)

**Decision.** El atlas (8 tiles de 16x16) se genera por codigo como una rejilla
con ruido determinista; no cargamos imagenes de disco.

**Motivo.** Cero dependencias de assets, resultados reproducibles y todo bajo
control de versiones como codigo. Cuando haya texturas hechas a mano, se
sustituira el generador por un cargador de PNG sin cambiar el mesher.

### 2026-10-04 — Fuera del chunk = aire

**Decision.** `Chunk::get_or_air` devuelve aire para coordenadas fuera del
chunk, asi que el mesher dibuja la cara exterior.

**Motivo.** Con un solo chunk es lo correcto y lo mas simple. Cuando haya
varios chunks (v0.3.1), esta funcion pasara a consultar el chunk vecino.

### 2026-10-04 — Terreno de ejemplo deterministico (placeholder)

**Decision.** `Chunk::generate_demo` crea una colina con senos/cosenos y un
arbol. No es generacion procedural "de verdad".

**Motivo.** Meter el ruido Perlin y los biomas es el objetivo de v0.3.0. Este
placeholder solo asegura que haya algo interesante que mirar y probar en v0.2.0.

### 2026-10-04 — `world` depende de `render::mesh::Vertex`

**Decision.** El mesher (en `world`) construye `render::mesh::Vertex` y `mesh`
es `pub(crate)`.

**Motivo.** Tener DOS tipos de vertice (uno de mundo y otro de GPU) obligaria a
convertir en cada frame. Aceptamos que `world` conozca el tipo de vertice;
`render` sigue sin conocer la logica del mundo. Es un acoplamiento pequeno y
consciente.

---

## v0.3.0 — Generacion de terreno

### 2026-10-04 — Ruido Perlin con dos octavas manuales (crate `noise`)

**Decision.** La altura es `64 + base*20 + detalle*4`, con dos capas Perlin a
distinta frecuencia (0.010 y 0.045). No usamos `Fbm`.

**Motivo.** Dos capas simples dan relieve continuo (colinas + detalle) sin la
complejidad de configurar fractales. Es facil de leer y de ajustar. Si mas
adelante queremos mas octavas, se sustituye sin cambiar la interfaz publica.

### 2026-10-04 — El generador es un tipo con semilla

**Decision.** `TerrainGenerator::new(seed)` guarda la semilla y dos `Perlin`.

**Motivo.** Determinismo (misma semilla = mismo mundo) y encaja con el
versionado de generador que pide la guia para v0.5.0: podremos reconstruir el
generador a partir de `(version, seed)` guardados en el header del mundo.

### 2026-10-04 — Altura en `48..96` (dentro de la seccion 3)

**Decision.** La altura queda en `48..96`, con la superficie cerca de `y=64`.

**Motivo.** Mantiene el terreno bajo (menos secciones con geometria = menos
dibujado) y dentro de la seccion 3, que es donde caera la camara. Cuando
lleguen oceanos/cuevas subiremos el rango.

### 2026-10-04 — `generate_demo` se elimina

**Decision.** El terreno de ejemplo de v0.2.x se sustituye por el generador
Perlin; la columna se genera con `TerrainGenerator::generate_column(0, 0)`.

**Motivo.** Ya hay generacion "de verdad". En v0.3.1 esto pasara a generar
varias columnas con coordenadas globales, y el mundo empezara a extenderse.

---

## v0.3.1 — Mundo infinito (visual)

### 2026-10-04 — Streaming por columnas disparado por el jugador

**Decision.** El `Renderer` guarda el centro de la rejilla cargada; cuando el
jugador cruza a otra columna (`posicion / 16`), regenera toda la rejilla 3x3.
Sin cache todavia.

**Motivo.** Version "visual" minima que demuestra el sistema de streaming sin
complicar el renderer con un pool de meshes y generacion en hilos (eso es
v0.5.1). Al alejarse, el terreno se genera en la nueva zona.

### 2026-10-04 — Posiciones en coordenadas de mundo (sin modelo por columna)

**Decision.** El mesher recibe un `origin` y escribe las posiciones ya en
coordenadas de mundo; todas las secciones comparten la misma matriz `view_projection`.

**Motivo.** Evita una matriz `model` por malla (que exigiria un buffer de
instancias o reescribir uniform por draw). Para voxeles, que apenas se mueven,
posicionar la geometria en el mundo es lo mas simple y rapido.

### 2026-10-04 — Rejilla 3x3 con regeneracion completa (sin cache)

**Decision.** Al cambiar de columna se descartan las mallas antiguas y se
regeneran las 9. No hay cache de columnas ya generadas.

**Motivo.** Es el paso "visual" de la guia; la cache y el pool de chunks llegan
en v0.5.1. Regenerar 9 columnas es rapido hoy (unos cientos de ms) y mantiene el
codigo legible.

---

## Plantilla para futuras entradas

```
### [fecha] vX.Y.Z — Titulo
**Decision.** ...
**Motivo.** ...
**Alternativas descartadas.** ...
**Consecuencia.** ...
```
