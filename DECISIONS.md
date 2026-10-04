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

## v0.3.2 — Colisiones basicas

### 2026-10-04 — El controlador recibe `is_solid` como parametro

**Decision.** `PlayerController::update(..., is_solid: impl Fn(Vec3) -> bool, ...)`
recibe la consulta del mundo en cada llamada, en lugar de guardarla dentro.

**Motivo.** El mundo (el `Renderer`, que tiene el chunk central) se presta desde
la `App`. Guardar una closure con captura dentro del controlador obligaria a
`Box<dyn Fn>` y a perder `Copy`. Pasarla por parametro mantiene el controlador
trivial, testeable con una funcion plana (`|p| p.y < 4.0`) y sin acoplarlo a wgpu.

### 2026-10-04 — Fisica por subpasos (no un solo paso)

**Decision.** Al integrar la velocidad vertical dividimos el desplazamiento en
subpasos de como maximo 0.5 bloques.

**Motivo.** Un `dt` grande con una caida rapida (hasta 50 bloques/s) puede saltar
por encima de un bloque fino en un solo paso y atravesarlo. Los subpasos lo
evitan con coste minimo.

### 2026-10-04 — Streaming ampliado a 7x7 y "settle" al arrancar

**Decision.** El radio de columnas pasa de 1 (3x3) a 3 (7x7 = 112x112 bloques) y
la camara se posa sobre el primer bloque solido al arrancar.

**Motivo.** v0.3.1 regeneraba el mundo "de golpe" al cruzar de chunk; eso era
incompatible con andar a ras de suelo (te teletransportabas al regenerar). Un
radio mayor cubre bastante terreno para caminar. La regeneracion en hilos/cache
llega en v0.5.1; entonces el radio podra volver a ser pequeno.

### 2026-10-04 — Modo vuelo con `F`

**Decision.** `F` alterna volar (sin gravedad, Espacio/Shift vertical).

**Motivo.** Muy util para depurar y explorar el mundo mientras las colisiones son
todavia basicas. Es tambien el embrión del modo creativo.

---

## v0.4.0 — Romper y colocar bloques

### 2026-10-04 — Raycast con el algoritmo de Amanatides-Woo (DDA 3D)

**Decision.** Recorremos la rejilla de voxeles eje a eje (DDA) en vez de dar
pasitos finos muestreando puntos.

**Motivo.** El DDA visita exactamente las celdas que el rayo atraviesa (sin
saltarse ninguna ni repetir), es rapido y devuelve con precision la cara de
entrada. Un muestreo a pasos fijos puede atravesar esquinas y es mas lento.

### 2026-10-04 — El raycast y el mesher reciben `is_solid`/consultan la Column

**Decision.** La funcion `raycast` recibe una closure `is_solid`; el `Renderer`
la construye para el chunk central.

**Motivo.** Igual que en el controlador: mantiene el algoritmo puro y testeable
con un "suelo" plano, sin depender del mundo ni de wgpu.

### 2026-10-04 — Solo se edita el chunk central

**Decision.** Las 48 columnas vecinas son de solo lectura; el jugador solo puede
romper/colocar en su chunk central (16x16).

**Motivo.** Es el minimo que demuestra la interaccion. Editar cualquier columna
exige que el `Renderer` guarde todas las columnas y sus mallas indexadas por
seccion (v0.5.1, con el pool de chunks). El chunk central siempre esta donde el
jugador, que es donde va a tocar.

### 2026-10-04 — Resaltado con `LineList` y color plano (sin textura)

**Decision.** El bloque apuntado se dibuja como 12 aristas (`PrimitiveTopology::
LineList`) con un pipeline propio que comparte el bind group de la escena
(misma matriz mvp) y pinta naranja.

**Motivo.** Un wireframe de 24 indices es la forma mas barata y clara de resaltar
una celda. Compartir el layout del pipeline evita duplicar el uniform y el bind
group. `depth_write_enabled=false` + un poco de *bias* evitan el z-fighting con
las caras del propio bloque.

### 2026-10-04 — Regenerar solo la seccion afectada

**Decision.** Al romper/colocar, `mesh_section` regenera unicamente la seccion
editada (y la contigua si el bloque estaba en su borde).

**Motivo.** Regenerar una seccion (4096 bloques) es milisegundos; regenerar las
24 no. Es el primer paso hacia el meshing incremental de v0.12.2.

---

## v0.4.1 — Greedy meshing

### 2026-10-04 — Greedy propio (en vez de la crate `block-mesh`)

**Decision.** Implementamos el greedy meshing nosotros en `world/greedy.rs` en
lugar de integrar la crate `block-mesh`.

**Motivo.** Encaja con la filosofia del proyecto (entenderlo todo) y no atamos la
geometria a una API externa. `mesh_column` (naive) se mantiene como referencia y
para comparar en tests.

### 2026-10-04 — La mascara del plano es dinamica (16 x v_hi)

**Decision.** En lugar de una mascara fija 16x16, cada plano usa una mascara de
16 x `v_hi`, donde `v_hi` es el alto del plano (16 por seccion, 384 si es una
columna entera).

**Motivo.** Las caras verticales (X/Z) barrian 384 bloques de alto en una columna
completa; una mascara 16x16 se salia de rango. Dimensionarla segun el rango
mantiene el algoritmo general (sirve para seccion y para columna entera).

### 2026-10-04 — No se fusionan caras de distinto `(bloque, cara)`

**Decision.** La clave de fusion es `(block_id, face)`, no solo el tile.

**Motivo.** Aunque dos caras compartan tile, fusionarlas mezclaria materiales y
dificultaria el raycast/edicion despues. Con `(bloque, cara)` el resultado sigue
siendo por-material, que es lo correcto.

### 2026-10-04 — Resultado medido

**Dato.** Rejilla 7x7: las 48 columnas vecinas pasan a ~1344 triangulos (antes,
decenas de miles). Una capa plana de 16x16: de 256 caras (512 triangulos) a 1
cara (2 triangulos).

**Nota.** Quedan ~1-2 px de costura visible entre rectangulos grandes por el
medio texel de inset del atlas; se afinara con *texture arrays* o padding real
cuando lleguen mas bloques. No es un fallo de geometria, es del atlas.

---

## v0.5.0 — Versionado y guardado del mundo

### 2026-10-04 — bincode 2 con derive nativo (no serde)

**Decision.** Usamos `bincode 2` con sus derives `Encode`/`Decode` en lugar de
serde.

**Motivo.** bincode 2 tiene un derive propio mas directo y nos ahorra el paso
por serde. Ademas, la version 3.0.0 de bincode en crates.io es una broma (solo
contiene un `compile_error!`); fijamos la 2.x estable.

### 2026-10-04 — Tres versiones distintas y desacopladas

**Decision.** `FORMAT_VERSION` (layout binario), `GENERATOR_VERSION` (algoritmo
de terreno) y `ENGINE_VERSION` (motor) se guardan por separado en el header, y
cada `ChunkRecord` lleva su propia `format_version`.

**Motivo.** La guia insiste en ello: un mundo puede seguir siendo legible aunque
el generador cambie, y viceversa. Versionar por chunk permite migrar solo los
que hagan falta.

### 2026-10-04 — Guardar el chunk completo, no un diff

**Decision.** Cada `ChunkRecord` guarda los 4096 bloques enteros (4 KB), no solo
lo que cambio.

**Motivo.** Simplicidad y robustez. Un diff ahorraria disco pero complica el
formato y la migracion. El ahorro de espacio llega en v0.5.2 (LZ4) y v0.11.0
(paleta).

### 2026-10-04 — Solo se persiste el chunk central

**Decision.** En v0.5.0 se guarda/carga unicamente el chunk central (la seccion
de terreno) porque es el unico editable.

**Motivo.** Es coherente con lo que ya existe. Cuando el mundo entero este en
memoria (v0.5.1) y se pueda editar en cualquier columna, se guardaran todos los
chunks modificados. El formato (`HashMap<ChunkPos, ChunkRecord>`) ya lo soporta
sin cambios.

### 2026-10-04 — `exiting` como red de seguridad, con flag anti-doble-guardado

**Decision.** Se guarda en `CloseRequested`/Escape y tambien en `exiting`, pero
con un flag `world_saved` para no escribir dos veces.

**Motivo.** `exiting` se llama siempre al cerrar limpiamente, pero si el sistema
cierra la ventana sin pasar por `CloseRequested`, perderiamos el mundo. El flag
evita la doble escritura que vimos en la prueba.

---

## v0.5.1 — Streaming de chunks

### 2026-10-04 — `World` con cache de columnas y streaming por radio

**Decision.** Introducimos `world::store::World`: `HashMap<ChunkPos, Column>`
con las columnas cargadas, mas `modified: HashMap<ChunkPos, ChunkRecord>`. El
streaming (`update_streaming`) carga/descarga segun el radio. No se regenera
nada que ya este cargado.

**Motivo.** Resuelve los dos problemas de v0.3.x: (1) al cruzar de chunk ya no
se regenera todo; (2) las columnas existen, asi que el mesher puede consultar
vecinos. Las ediciones se guardan en `modified`, que tambien hace de cache.

### 2026-10-04 — El mesher recibe una consulta de bloque (`greedy_section_query`)

**Decision.** `greedy_range`/`mask_value` reciben un `query(x,y,z) -> Block` en
coordenadas locales, **pero que puede mirar fuera** de la columna.

**Motivo.** Es lo que elimina los muros internos: al calcular la cara de un voxel
en el borde, la consulta devuelve el bloque del chunk vecino y oculta la cara si
tambien es solido. Mantiene el greedy puro y testeable.

### 2026-10-04 — El renderer cachea mallas por (columna, seccion)

**Decision.** `meshes: HashMap<ChunkPos, [Option<Mesh>; 24]>`. Al entrar/salir
columnas se construyen/liberan solo esas mallas.

**Motivo.** Meshear 81 columnas cada frame seria inviable. Con la cache, el
coste solo aparece al cruzar de chunk. De paso, editar un bloque regenera la
columna y sus 4 vecinas (su cara de borde tambien cambia).

### 2026-10-04 — Generacion sincrona todavia

**Decision.** La generacion de columnas sigue en el hilo principal; no usamos
`rayon` aun.

**Motivo.** Con radio 4 (81 columnas) la generacion inicial tarda decimas de
segundo y luego solo se generan las nuevas al cruzar de chunk. La generacion en
hilos + cola de peticiones (lo que pedia la guia) queda para cuando el radio sea
mayor; la estructura (`StreamChange`) ya esta preparada.

---

## v0.5.2 — Compresion de chunks (LZ4)

### 2026-10-04 — LZ4 (`lz4_flex`) en el propio `ChunkRecord`, formato v2

**Decision.** Los 4096 bloques se guardan comprimidos con LZ4 dentro del
`ChunkRecord` (`compressed: bool` + `blocks`), y `FORMAT_VERSION` sube a 2.

**Motivo.** LZ4 es rapidisimo y sin dependencias nativas, ideal para este caso.
Comprimir dentro del registro (no todo el archivo) permite migrar chunk a chunk
y leer uno sin descomprimir los demas.

**Medida.** Un chunk de terreno real: **4096 -> 31 bytes (x132)**. El aire y las
zonas uniformes comprimen casi a cero.

### 2026-10-04 — Migrador v1 -> v2 incluido en la cadena por defecto

**Decision.** `MigrationChain::with_builtins()` registra `V1ToV2`, que comprime
los chunks que vienen sin comprimir.

**Motivo.** Es el primer migrador real y valida el sistema de versionado: un
mundo v1 se carga, se migra a v2 y se puede seguir usando.

**Limitacion honesta.** Como bincode es posicional, un archivo v1 **binario**
real no deserializa directamente en el struct v2 (le falta el campo
`compressed`). El migrador funciona sobre registros ya deserializados; para
leer archivos v1 reales haria falta un struct `ChunkRecordV1` espejo y una
funcion de conversion. Como v1 solo existio minutos en desarrollo, no lo
implementamos, pero queda anotado para el futuro.

---

## v0.6.0 — Iluminacion basica

### 2026-10-04 — Skylight "columnar" simple, no flood-fill 3D todavia

**Decision.** `Column::compute_skylight` marca 15 las celdas a cielo abierto
(por encima de la primera cosa solida de su columna vertical) y 0 el resto. No
propaga la luz lateralmente.

**Motivo.** Da el 90% del efecto con 10% del codigo: superficie iluminada,
subsuelo oscuro. El flood fill 3D (que iluminaria cuevas cercanas a la
superficie y suavizaria bordes) es v0.6.1/v0.6.2; el metodo esta aislado para
sustituirlo sin tocar el mesher ni el shader.

### 2026-10-04 — Luz por vertice, normalizada 0..1

**Decision.** `Vertex` gana `light: f32` (0..1). El mesher la calcula desde la
celda de aire frente a la cara y la incluye en `FaceKey` (para no fusionar caras
con distinta luz). El shader aplica `ambient + (1-ambient)*light`.

**Motivo.** Iluminacion barata (sin lighting de pantalla), suave entre caras por
interpolacion, y sin coste de memoria por bloque en la GPU. El mínimo ambiental
(0.15) evita que la oscuridad deje zonas ilegibles.

### 2026-10-04 — La luz se recomputa al editar

**Decision.** `World::set_block` recalcula la skylight de la columna editada.

**Motivo.** Si rompes el techo de una cueva, tiene que entrar luz. Recomputar una
columna (16x16x384) es barato. La propagacion incremental (solo lo afectado) es
v0.12.1; por ahora recomputar la columna entera es correcto y simple.

### 2026-10-04 — Memoria de la luz: 1 byte por celda (de momento)

**Decision.** La luz se guarda como `Vec<u8>` de 16x16x384 = ~98 KB por columna.

**Motivo.** Simple y suficiente con 81 columnas (~8 MB). En v0.11.x/v0.12.x
pasara a 4 bits por celda (mitad) o a una textura de luz.

---

## v0.6.1 — Block light (antorchas)

### 2026-10-04 — Flood-fill BFS para la luz de bloque

**Decision.** `compute_block_light` usa una cola (BFS) desde cada emisor; la luz
pierde 1 por paso y no atraviesa solidos.

**Motivo.** BFS garantiza que cada celda se visita con su nivel **mas alto** la
primera vez (a diferencia de DFS, que podria fijar un nivel bajo antes de
encontrar un camino mejor). Es el algoritmo clasico de luz de Minecraft.

### 2026-10-04 — Luz final = max(cielo, bloque)

**Decision.** `combined_light` devuelve el maximo de las dos luces, y es lo que
se manda al shader.

**Motivo.** Coincide con Minecraft: una antorcha ilumina una cueva (bloque alto,
cielo 0), pero no oscurece una zona ya iluminada por el sol.

### 2026-10-04 — La antorcha es "visible no solida"

**Decision.** `Block::is_solid` es false para la antorcha, pero existe
`is_visible`; el mesher dibuja `is_solid || is_visible` y solo oculta caras
contra vecinos **solidos**.

**Motivo.** La antorcha no debe bloquear el movimiento ni tapar caras de los
bloques vecinos, pero si debe dibujarse. Separar "solido" (colisiona/oculta) de
"visible" (se dibuja) es lo que lo hace limpio.

### 2026-10-04 — Antorcha como bloque completo (v0.6.1), no cruz de planos

**Decision.** De momento la antorcha es un bloque de 1x1x1 con su tile; no una
cruz de dos planos (el aspecto clasico).

**Motivo.** Mantiene el mesher y el atlas simples. La representacion como cruz
necesitaria geometria no cubica y un pase de transparencia. Queda para pulido.

### 2026-10-04 — Modo demo por variable de entorno

**Decision.** `SOLARIA_DEMO=1` coloca antorchas cerca del jugador al arrancar.

**Motivo.** Permite capturar el efecto de la luz sin interactuar (yo no puedo
hacer click en la app). No afecta al juego normal si la variable no esta.

---

## Plantilla para futuras entradas

```
### [fecha] vX.Y.Z — Titulo
**Decision.** ...
**Motivo.** ...
**Alternativas descartadas.** ...
**Consecuencia.** ...
```
