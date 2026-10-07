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

### 2026-10-04 (v0.6.2) — La antorcha se dibuja como cruz de dos planos, no cubo

**Decision.** El mesher deja de emitir el cubo de 6 caras para `Block::Torch` y
emite **dos quads verticales cruzados** (planos `X = centro` y `Z = centro` del
voxel), ambos con el tile 8 completo. El mesher naive gana `emit_torch_cross`;
el greedy lo llama desde una pasada aparte (la antorcha no entra en el greedy,
porque es geometria propia, no una cara de cubo).

**Motivo.** Es el aspecto clasico de la antorcha (fina, con la llama). El shader
ya descarta alfa < 0.5 (cutout), asi que el fondo transparente del tile
desaparece solo y no hace falta blending ni ordenar triangulos. El pipeline se
dibuja sin *culling* (`cull_mode: None`), de modo que cada plano se ve por sus
dos caras emitiendose una sola vez.

**Alternativas descartadas.** (a) Mantener el cubo: daba una caja opaca negra a
sombra (v0.6.1). (b) Emitir los planos como caras dobles explicitas: innecesario
sin culling.

### 2026-10-04 (v0.6.2) — El raycast golpea bloques visibles no solidos

**Decision.** El predicado del raycast pasa de `is_solid` a `is_solid ||
is_visible` (en `Renderer::raycast`). El algoritmo ya no habla de "solido": es
un predicado `is_hit` generico.

**Motivo.** Antes la antorcha no se podia apuntar ni romper (no era "solida").
Es coherente con Minecraft: se apunta a cualquier bloque interactuable. Se
mantiene la cara de entrada para colocar el bloque nuevo al lado.

**Consecuencia.** La antorcha tambien se resalta con el wireframe (el resaltado
usa el mismo `RayHit`).

### 2026-10-04 (v0.6.2) — Pendiente: palo 3D y antorcha de pared

**Decision (para una version posterior).** El `.bbmodel` de Blockbench tiene el
palo como **cubo** `7,0,7→9,10,9` ademas de los dos planos. En v0.6.2 solo se
emiten los dos planos (el palo va dibujado dentro de la textura). Tampoco hay
aun **antorcha de pared** (inclinada 22.5 grados sobre una cara vertical).

**Motivo.** Para no duplicar el palo (cubo 3D + dibujado en la textura) habria
que separar la textura de la llama de la del palo; es mas limpio hacerlo en una
version dedicada al modelo completo, junto con la orientacion en pared.

### 2026-10-04 (v0.6.3) — Version de consolidacion, no de features

**Decision.** Antes de seguir avanzando por el roadmap (biomas, cuevas,
gameplay), dedicamos una version a reforzar calidad: tests, documentacion de
arquitectura y limpieza. No se anade ninguna funcionalidad visible.

**Motivo.** La documentacion y los tests deberian haber ido desde el principio,
no al final. Con el motor ya en ~4.900 LOC y 72 tests, era el momento de tapar
los huecos antes de que crezcan: la logica de interaccion no tenia tests y no
existia un documento de arquitectura.

**Alternativas descartadas.** (a) Seguir con features (ciclo dia/noche,
biomas): habria acumulado mas codigo sin cobertura. (b) Adoptar `clippy::pedantic`
entero: demasiado ruido (cientos de avisos de casts `usize`/`f32` que son
intencionales en un motor de voxeles). Nos quedamos con el clippy por defecto en
cero warnings.

**Consecuencia.** 83 tests. `block_overlaps_player` pasa de `app.rs` a `player`
(es geometria del jugador) y gana tests. La escena demo se extrae a
`engine::demo`. Nuevo `ARCHITECTURE.md`.

### 2026-10-04 (v0.6.3) — El atlas procedural tambien hace cutout

**Decision.** El `build_pixels` (fallback cuando no existe `assets/atlas.png`)
genera el tile de la antorcha con **fondo transparente** (alfa 0), igual que el
atlas pintado a mano.

**Motivo.** Antes el fallback pintaba el fondo opaco (`[40,40,45]`): quien
clonara el repo sin assets veria la antorcha como un cuadrado oscuro (el mismo
bug que resolvimos en v0.6.2 para el atlas real). Se detecto al escribir el test
`el_tile_de_la_antorcha_tiene_transparencia`.

**Consecuencia.** `tile_color` pasa a devolver RGBA en lugar de RGB. Nuevos
tests: transparencia del tile 8 y que las capas del atlas no mezclan vecinos.

### 2026-10-04 (v0.6.3) — Relacion con la guia: stack propio y desfase de versiones

**Decision.** Registramos explicitamente la relacion entre este proyecto y la
guia de referencia (guia iterativa de motor de voxeles) para que no se pierda:

* **Stack:** la guia propone Bevy (ECS + motor completo). Solaria Voxel usa
  **solo `winit` + `wgpu`** y escribe el resto desde cero (math, camara, meshing,
  mundo, guardado, fisica). Es una decision deliberada: control total y
  entendimiento a bajo nivel, a cambio de mas trabajo.
* **Desfase de versiones:** la guia situa el **ciclo dia/noche** en `v0.6.2`.
  Nosotros usamos `v0.6.2` para la antorcha como cruz fina (pulido, fuera de la
  guia). El ciclo dia/noche sigue **pendiente**. La guia situa `v0.7.0` en
  **biomas simples**, `v0.7.1` en cuevas y `v0.7.2` en oceanos.
* **Lo que si adoptamos de la guia:** micro-versiones con tag por version,
  `DECISIONS.md` desde el principio, SemVer del motor, y el **versionado del
  formato de mundo con migradores** (`WorldHeader`/`ChunkRecord`/
  `MigrationChain`, ver `ARCHITECTURE.md`).

**Motivo.** El desfase es real y conviene documentarlo antes de retomar el
roadmap, para decidir con criterio si se prioriza el ciclo dia/noche (hueco con
la guia) o el pulido del modelo (palo 3D / antorcha de pared).

### 2026-10-04 (v0.6.4) — Ciclo dia/noche con luz de cielo y de bloque separadas

**Decision.** Anadimos `scene::DayCycle` (hora del mundo) y separamos la luz del
vertice en **`sky`** (cielo) y **`block`** (antorchas). El shader dibuja
`max(sky * day_factor, block)`. El color del cielo (sRGB) se interpola en CPU
entre noche/amanecer/dia y se sube como color de clear.

**Motivo.** Para que al anochecer se oscurezca el terreno pero **las antorchas
sigan iluminando** hace falta saber que parte de la luz es del sol y que parte de
bloque. Con la luz combinada (`max(cielo, bloque)`) no se puede apagar solo el
sol. Ademas, es el hueco de la guia (que situa el ciclo dia/noche en v0.6.2).

**Alternativas descartadas.** (a) Oscurecer todo por igual de noche: apagaria
tambien las antorchas y dejaria las cuevas negras. (b) Calcular el color del
cielo en el shader: mas simple en CPU y testeable (interpolacion pura).

**Consecuencia.** `Vertex` pasa de un `light` a `sky` + `block`; `FaceKey`,
greedy, mesher y `World` exponen las dos luces. La banda naranja del cielo es
estrecha a proposito (evita un rosa desaturado a media manana). `Renderer`
gana `set_environment` y `set_blocks` (aplicar muchos bloques en un lote).
`SOLARIA_TIME` fija la hora de las capturas.

### 2026-10-04 (v0.6.4) — `Renderer::set_blocks` para editar en lote

**Decision.** Nuevo metodo para aplicar **muchos** cambios de bloque y regenerar
las mallas afectadas **una sola vez**, en lugar de una vez por bloque.

**Motivo.** La escena demo alisaba una parcela con ~560 `set_block`; cada uno
reconstruia las mallas de 5 columnas (24 secciones cada una) y tardaba ~14 s.
Con el lote baja a menos de un segundo.

**Consecuencia.** `demo::build` reune las ediciones en un `Vec` y llama a
`set_blocks` una vez.

### 2026-10-04 (v0.6.5) — Colision horizontal del jugador

**Decision.** El movimiento horizontal deja de ser `Camera::walk` (sin colision)
y pasa por `PlayerController::move_horizontal`: el jugador es una **caja** (radio
`PLAYER_RADIUS`, alto `PLAYER_HEIGHT`) y se mueve **eje a eje** (primero X, luego
Z). Si un eje choca con un bloque solido, ese eje se cancela y el otro sigue
(deslizamiento).

**Motivo.** Solo existia fisica **vertical** (gravedad/suelo); en horizontal el
jugador atravesaba paredes y podia quedar dentro del terreno (la camara "entraba
en los chunks"). Era el bug mas visible de la etapa "mundo jugable".

**Alternativas descartadas.** (a) Resolver los dos ejes a la vez: bloquea el
movimiento en diagonal contra una pared (no se desliza). (b) Caja de colision
exacta (AABB por vertices): mas cara y no aporta en voxeles alineados a ejes.

**Consecuencia.** Nuevo `PlayerController::move_horizontal` + `collides` (AABB
contra voxeles). `App::update` lo llama antes de la fisica vertical. 4 tests
nuevos (pared, deslizamiento, espacio libre, no chocar con el suelo). Demo
`SOLARIA_COLLIDE=1` empuja al jugador contra un muro para verificar en captura
que se detiene delante (z=18.37 con el muro en 18).

### 2026-10-05 (v0.7.0) — Biomas con ruido Worley

**Decision.** El generador reparte el mundo en tres biomas (desierto, bosque,
nieve) con un ruido **Worley** (cellular) de baja frecuencia (`0.02`, celdas de
~50 bloques). El bioma decide el bloque de superficie: arena / hierba / nieve.
Nuevo bloque `Snow` (id 8, tile 9) pintado en `assets/atlas.png` con LibreSprite.

**Motivo.** Es el hito `v0.7.0` de la guia ("Biomas Simples"). Worley da regiones
compactas (mejor que umbrales de Perlin, que dan franjas).

**Nota (correccion).** Aqui se dijo que "cierra la etapa 1", pero eso era
prematuro: la etapa 1 no quedo cerrada hasta `v0.7.8`, con las **cuevas**
(`v0.7.5`) y los **oceanos** (`v0.7.8`). Queda pendiente ademas la **vegetacion**
(arboles).

**Alternativas descartadas.** Dos campos Perlin (temperatura/humedad) con
umbrales: la guia pide Worley y las regiones de Worley son mas "bioma".

**Consecuencia.** `Biome` y `TerrainGenerator::biome_at`. `GENERATOR_VERSION`
sube a 1 -> 2 (cambia la generacion). Los mundos guardados siguen cargando: los
bloques se guardan por id y `Snow` es un id nuevo sin colision. `TILES` pasa de
9 a 10; el tile 9 ya cabia en el atlas 64x48.

### 2026-10-05 (v0.7.1) — Altura y bioma por bloque (terreno suave)

**Decision.** `generate_column` calcula `height(world_x + x, world_z + z)` y
`biome_at(world_x + x, world_z + z)` **para cada bloque** de la columna, en
lugar de consultar una sola altura/bioma en el origen del chunk y repetirla en
los 16x16. `GENERATOR_VERSION` sube de 2 a 3.

**Motivo.** Con una altura por chunk, cada columna de 16x16 era una **meseta
plana** y el limite entre chunks producia **escalones de 16 bloques** (visibles
en `screenshots/terreno_actual.png`). El ruido Perlin ya era continuo: el fallo
no estaba en el ruido sino en *donde* se muestreaba. Muestrear por bloque cuesta
la misma generacion (16x16 = 256 llamadas a Perlin/Worley por columna, que es
despreciable) y da **colinas suaves** de escalones de 1 bloque
(`screenshots/terreno_suave.png` / `v0.7.1_terreno.png`).

**Alternativas descartadas.** (a) Interpolar la altura entre esquinas del chunk:
suavizaria dentro del chunk pero no arreglaria los saltos entre chunks vecinos.
(b) Subir la frecuencia del ruido: no elimina las mesetas, solo las hace mas
pequenas. La causa era el muestreo, no la escala.

**Nota.** El bioma tambien pasa a ser por bloque. En la practica Worley varía
lento (celdas de ~50 bloques), asi que el cambio apenas se nota, pero es
coherente: cada bloque pregunta su propio bioma.

**Desplazamiento de roadmap.** La guia situaba las **cuevas en v0.7.1** y los
**oceanos en v0.7.2**. Como v0.7.1 se dedica a cerrar este bug de terreno (que
la etapa "mundo jugable" dejaba visible), cuevas pasa a **v0.7.2** y oceanos a
**v0.7.3**. No se adelanta ninguna feature nueva: solo se corre el calendario.

**Consecuencia.** `TerrainGenerator::generate_column` mueve `height`/`biome` al
bucle interno. Los tests siguen verdes (98). `GENERATOR_VERSION = 3`; los mundos
v3 se reproducen igual, los v2 conservan sus ediciones guardadas (los bloques se
guardan por id).

### 2026-10-05 (v0.7.2) — Fisica vertical por huella + auto-escalon

**Decision.** La fisica vertical (`PlayerController::update`) y el `settle` dejan
de sondear un unico punto (el centro de los pies) y pasan a mirar la **huella
completa** del jugador (las columnas que cubre su caja). Ademas se anade
**auto-escalon**: al caminar contra un escalon de <= `STEP_HEIGHT` (1.0 bloque),
el jugador sube y avanza; un muro de 2 bloques sigue exigiendo salto.

**Motivo.** Con el terreno **por bloque** de v0.7.1 los escalones de 1 bloque son
continuos, y aparecio un bug: parado sobre un borde, el centro de los pies caia
sobre la columna vecina (mas baja), el jugador empezaba a hundirse y su **caja**
seguia solapando el bloque del escalon. A partir de ahi `move_horizontal` lo
rechazaba todo y el jugador quedaba **embebido** ("la camara se buguea en los
bloques"). El sondeo por punto era correcto en el terreno de mesetas de v0.7.0
(planas) pero no en el nuevo. Se reprodujo en un test de simulacion: fallaba en
el frame 33. Sin auto-escalon, ademas, cada subida de 1 bloque bloqueaba el paso
(el jugador se sentia "atascado" en cualquier colina).

**Alternativas descartadas.** (a) Subir `STEP_HEIGHT` a 0.6 (como Minecraft):
insuficiente para pasos de 1 bloque, que son la norma aqui. (b) Resolver el
embebido empujando al jugador fuera del bloque a posteriori: parche, no ataca la
causa (el sondeo por punto). (c) Snake/deslizar la caja verticalmente con la
huella tambien al subir: lo hicimos (`ceiling_hits` por huella) para no clipar
techos.

**Consecuencia.** `landing_surface`, `ceiling_hits` y `top_surface` (nuevos)
operan sobre la huella (`footprint_columns`). `settle` posa al jugador sobre la
superficie mas alta bajo su huella (antes, solo su columna). Nuevo
`try_step_up` + `STEP_HEIGHT`. Test de regresion
`caminata_por_terreno_real_no_queda_embebido` (20.000 frames de paseo aleatorio
sobre el terreno real de la semilla 13371). 101 tests.

**Desplazamiento de roadmap (otra vez).** El fix consume el numero `v0.7.2`, que
v0.7.1 habia asignado a las cuevas: las **cuevas** pasan a **v0.7.3** y los
**oceanos** a **v0.7.4**. No se adelanta ninguna feature; solo corre el
calendario. `GENERATOR_VERSION` **no** cambia (la generacion de terreno es
identica; esto es fisica).

### 2026-10-05 (v0.7.3) — Re-mesheo de vecinas al hacer streaming

**Decision.** Al cargar o descargar columnas por streaming, ademas de meshear las
que entran/salen, se **reconstruyen las mallas de sus 4-vecinas** ya cargadas
(`columns_to_remesh`). Y `build_column_meshes` **salta las secciones vacias**
para que ese re-mesheo extra salga barato.

**Motivo.** Las **caras de borde** de una columna se calculan consultando el
bloque del vecino (`greedy_section_query` mira fuera de la columna). Por tanto el
resultado depende de *que columnas estaban cargadas al meshear*. Los muros
internos entre chunks ya no aparecen... pero solo si el vecino estaba cargado en
ese momento. Al moverse:
* una columna mesheada con su vecina **ausente** conserva una cara de borde que
  ya no toca (un **muro**), y encima **oscuro**, porque la celda de aire de
  delante es "fuera de lo cargado" y su luz es 0;
* una columna mesheada con su vecina **presente** se queda sin la cara; al
  descargarse la vecina aparece un **hueco** por el que se ve a traves.

Se veia como lineas oscuras y grietas en el terreno (era el bug reportado como
"la camara se bugea en los bloques", aunque este es de render). Se confirmo
leyendo `renderer.rs` (solo se mesheaban las columnas del `StreamChange`).

**Alternativas descartadas.** (a) Meshear **todo** el radio al cruzar de chunk:
correcto pero carisimo. (b) No re-meshear y aceptar el artefacto: son visibles en
cuanto andas. (c) Retrasar el meshing hasta tener todo el anillo cargado: no
resuelve el caso simetrico de la **descarga** (hueco), que necesita re-meshear la
vecina que se queda.

**Consecuencia.** Nuevo `columns_to_remesh` (+ test). `World::section_is_empty` y
`Column::section_is_empty`; `build_column_meshes` ya no hace 24 pasadas de greedy
por columna (solo de las secciones con geometria). `Chunk::is_empty` pasa a
contar tambien los bloques **visibles-no-solidos**: antes una seccion con solo
una antorcha se habria saltado (bug latente; hay test). 103 tests.

**Desplazamiento de roadmap.** Cuevas pasa a **v0.7.4** y oceanos a **v0.7.5**.
`GENERATOR_VERSION` sigue en 3 (el terreno no cambia).

### 2026-10-05 (v0.7.4) — Rendimiento, niebla y luz que cruza chunks

**Decision.** Cuatro cambios de renderizado/iluminacion que estaban anotados como
"limitaciones":
1. **Back-face culling** en el pipeline, corrigiendo antes el **winding de las
   caras horizontales** (`PosY`/`NegY`), que estaba invertido (su normal miraba
   hacia dentro). Las antorchas emiten sus dos orientaciones.
2. **Frustum culling** por seccion: se descartan las mallas cuyo AABB cae fuera
   del frustum (`math::Frustum`, extraido de `view_projection`).
3. **Niebla a distancia** en el fragment shader (funde con el color del cielo, que
   ya cambia con el dia/noche) para disimular el borde del area cargada.
4. **Luz de bloque que cruza chunks**: `World::recompute_block_light` hace el BFS
   de antorchas a nivel de mundo (antes era por columna, y se cortaba en el borde).
5. **FPS** visibles en el titulo de la ventana.

**Motivo.** El culling y el salto de secciones vacias (v0.7.3) abaratan el dibujo;
el winding estaba mal y por eso `cull_mode` estaba en `None`. La niebla es la
forma estandar de tapar el limite del radio de carga sin cargar mas chunks. El
corte de luz en las fronteras era visible al poner antorchas cerca de un borde.

**Alternativas descartadas.** (a) Subir el radio de carga para esconder el borde:
mas memoria y meshing, y el problema no desaparece, solo se aleja. (b) No emitir
las caras del borde cargado: dejaria ver el vacio a traves del terreno. (c) Fog
"de altura" tipo volumetrico: sobredimensionado.

**Verificacion.** FPS en el titulo (build debug: ~780 fps). Test del winding
(la normal geometrica de cada cara debe coincidir con su salida) y de frustum
(fuera/dentro/rodeando la camara). Test de luz cruzando el borde de chunk (13 a
1 bloque, 0 al quitar la antorcha). 110 tests.

**Deuda pendiente (honesta).** La **luz de cielo sigue siendo columnar** (sin
propagacion lateral). No es visible hoy porque el terreno es de **altura** (sin
voladizos ni cuevas): cualquier celda de aire tiene cielo encima de su propia
columna, asi que 15 es correcto. Se vuelve necesaria con las **cuevas** (v0.7.5),
donde si habra techos; se hara alli, junto con la propagacion entre chunks.

**Desplazamiento de roadmap.** Cuevas pasa a **v0.7.5** y oceanos a **v0.7.6**.
`GENERATOR_VERSION` sigue en 3.

### 2026-10-05 (v0.7.5) — Cuevas + luz de cielo lateral

**Decision.** El hito `v0.7.5` de la guia: **cuevas** con **ruido Perlin 3D** y
umbral. Y la deuda que v0.7.4 dejo anotada: **luz de cielo con propagacion
lateral** (necesaria en cuanto hay techos).

**Motivo.** Las cuevas se tallan donde un Perlin 3D cruza una **iso-superficie**
(`abs(noise) < umbral`), que da **tuneles continuos** en vez de burbujas (un
simple `noise > umbral`). Frecuencia baja (0.06 en X/Z, 0.11 en Y) y umbral 0.07:
~15% del volumen subterraneo queda aire (medido por test). No perfora la
**corteza** (2 bloques bajo la superficie) ni el suelo, para no acribillar el
terreno.

Con cuevas aparecen techos, y la luz de cielo **columnar** deja a oscuras (0) todo
lo que no ve el cielo por su propia columna, con un corte brusco. Se anade un BFS
lateral a nivel de mundo: el aire en sombra se ilumina de lado con -1 por paso
horizontal (sin perdida hacia abajo, como Minecraft). Base columnar + propagacion.

**Rendimiento (medido).** Recalcular la luz de todo el mundo (81 columnas) costaba
~60 ms, inaceptable en cada edicion/cruce. Se hace por **region**: solo las
columnas **sucias** (las que cambian: edicion o streaming) + su anillo 3x3, ya que
la luz viaja <= 15 bloques (< 1 chunk). Optimizando ademas la lectura de vecinos
(misma columna directa en vez de HashMap): **~6 ms por edicion**, ~28 ms la carga
inicial. Encaja con "medir rendimiento" del hito.

**Alternativas descartadas.** (a) BFS global en cada cambio: 60 ms de tiron.
(b) Solo luz de cielo columnar y cuevas oscuras: el corte en las bocas de cueva se
nota. (c) Borrado de luz por BFS (removal) incremental estilo Minecraft: mas
complejo; con region + reinicio de la base columnar de las columnas sucias el
borrado es implicito.

**Consecuencia.** `TerrainGenerator::is_cave` + `CAVE_CRUST`/`CAVE_MIN_Y`;
`GENERATOR_VERSION` 4. `Column.surface` (altura del primer solido) y
`surface_y`. `World::recompute_skylight(dirty)`; el renderer le pasa las columnas
sucias en `sync_streaming`/`set_block`/`set_blocks`. Tests: fraccion de cuevas en
banda, corteza intacta, skylight bajo un techo (atenuacion lateral), y el
benchmark. 114 tests.

**Sin desplazamiento de roadmap.** A diferencia de las ultimas versiones, este SI
era el hito `v0.7.5`: **oceanos** sigue en **v0.7.6**.

### 2026-10-05 (v0.7.6) — Optimizacion del streaming (fin de los tirones de FPS)

**Decision.** Eliminar el tiron de FPS al descubrir chunks, **midiendo** donde se
iba el frame antes de tocar nada (un benchmark que simula un cruce de chunk):

| fase | antes |
| --- | --- |
| generar 9 columnas | ~8 ms |
| luz de cielo (region) | ~11.5 ms |
| luz de bloque | ~4 ms |
| **meshing (greedy)** | **~55 ms** |

Total ~80 ms en el frame del cruce. El culpable era el **meshing**, no la
generacion ni la luz. Tres cambios:

1. **Greedy mas rapido.** La mascara 2D se reservaba como `Vec<Vec<Option>>`
   **dentro del bucle de capas** (miles de asignaciones por seccion). Pasa a un
   `Vec` **plano reutilizado** entre capas. 55 -> 37 ms.
2. **Consultas de bloque sin `HashMap`.** El mesher consultaba el mundo
   (`get_block` con `div_euclid` + `HashMap`) por cada bloque. Ahora lee la propia
   columna directamente y solo acude al vecino en los bordes (-1/16).
3. **Cola de meshing con presupuesto por frame.** En vez de meshear las ~27
   columnas afectadas de golpe, se **encolan** (las cercanas al jugador primero) y
   `pump_meshing` las procesa gastando como mucho **6 ms por frame**. El coste se
   reparte entre varios frames; no hay pico. La calidad es la misma (solo deja de
   llegar todo en el mismo frame; el area nueva esta a ~64 bloques, tras la
   niebla).

Ademas, `compute_skylight` pasa a `fill(15)` + borrar solo bajo el primer solido
(~70 celdas/columna) en lugar de escribir las 384 capas: ~11.5 -> ~9.4 ms.

**Medido.** FPS estable a ~640 en el titulo tras drenar la cola (antes se veia
caer durante el pico). Los benchmarks `bench_*` quedan en `store.rs` como
evidencia.

**Alternativas descartadas.** (a) Mover generacion/meshing a hilos: la solucion
"de libro", pero exige compartir el `World` y subir mallas desde hilos; mas
maquinaria de la necesaria ahora. (b) Bajar el radio de carga: bajaría la
calidad. (c) Bajar el presupuesto de meshing: pop-in visible; 6 ms es el punto
donde no se nota.

**Consecuencia.** `Renderer.mesh_queue` + `queue_mesh`/`pump_meshing`;
`MESH_BUDGET_MS`. Greedy con mascara plana. `compute_skylight` optimizado. 115
tests. `GENERATOR_VERSION` sigue en 4.

**Desplazamiento de roadmap.** Este fix consume el numero `v0.7.6`, que la guia
reservaba para oceanos: **oceanos pasa a v0.7.7**. No se adelanta nada; solo el
arreglo que pidio el usuario tiene su propia version.

### 2026-10-05 (v0.7.7) — Texturas de tierra con grano fino (referencia Luanti)

**Decision.** Redibujar el **dirt** (tile 2) y el **lateral de hierba** (tile 1)
del atlas como **grano fino de bajo contraste**, manteniendo **nuestra paleta**
(marron base `134,96,67`), en `assets/atlas.png` con LibreSprite. El **fallback
procedural** (`atlas.rs`) se alinea con el mismo grano.

**Motivo.** El dirt se veia "raro": usaba 4 tonos con **mucho contraste**
(oscuro `78,52,38` vs claro `166,124,86`) agrupados en **manchas grandes**. Al
mirar como lo resuelve **Luanti/Minetest** (texturas 16x16, `grass_side`
superpuesta sobre `dirt`, upscaling nearest): su tierra es un marron casi uniforme
con **grano por píxel** y muy pocos tonos extremos. Reproducimos esa estructura
con nuestros colores: se **comprime el contraste** (los tonos extremos pasan a ser
~5% de los píxeles) y se **reparte por píxel** con un hash determinista, en vez de
en bloques. La franja de hierba del lateral pasa a tener un **borde irregular**.

**Alternativas descartadas.** (a) Cambiar la paleta a la de Minetest: la peticion
era mejorar **con nuestra paleta**. (b) Solo bajar la opacidad/contraste global:
aplana el relieve y pierde textura; el grano por píxel mantiene el detalle.
(c) Textura mas grande (32x32): el motor y el atlas son de 16x16.

**Consecuencia.** `assets/atlas.png` redibujado (dirt + grass side; antorcha y
demas tiles intactos, transparencia preservada). `atlas.rs` gana `dirt_shade` y
`grass_shade` y los usa en los tiles 1 y 2; tile 0 (hierba arriba) se alinea en
color. 115 tests. `GENERATOR_VERSION` no cambia.

**Roadmap.** **Oceanos pasa a v0.7.8** (la optimizacion de v0.7.6 ya habia corrido
el numero).

### 2026-10-05 (v0.7.8) — Oceanos (agua translucida, playas y nado)

**Decision.** Hito `v0.7.x` de la guia: **oceanos**. Bloque **`Water`** (id 9,
tile 10), generacion de mares/lagos, playas de arena, **pase de transparencia** y
nado basico.

**Referencias (pedidas).** De **Terasology** (TerraForge): `ocean.level` define el
nivel hasta el que se rellena de agua, y `ocean.palette` el bloque; de **Luanti**:
`water_level` en el mapgen y liquidos **translucidos** (`translucent_liquids`,
antes `opaque_water`). Adoptamos ambos: rellenar de `Water` el aire entre la altura
del terreno y `SEA_LEVEL`, con la translucidez como opcion de render.

**Motivo/estilo.** El agua se rellena **por columna** hasta el nivel del mar (el
`ocean.level` de Terasology): simple y suficiente para mares/lagos. Las columnas a
ras de agua (altura <= mar+1) usan **arena** de superficie (playa y fondo marino),
evitando hierba bajo el agua.

**Render.** El agua no puede ir en el mismo pase que lo opaco: necesita
**blending** y **no** escribir z (si no, el agua taparia lo de detras en el orden
de dibujo). El greedy separa la geometria de agua a su propio buffer
(`greedy_section_query` devuelve 4 vecs) y el renderer la dibuja en un **segundo
pase** con un pipeline gemelo (mismo shader/layout/bind group, pero
`BlendState::ALPHA_BLENDING`, `depth_write_enabled: false`, `cull_mode: None`). El
tile de agua (10) es azul con **alfa 175** en el atlas.

**Nado.** El agua es no solida: el jugador cae hasta el fondo. Para que se sienta
bien, dentro del agua la **gravedad se reduce** (`WATER_GRAVITY_SCALE`) y Espacio
**nada hacia arriba** (`SWIM_UP_SPEED`). El `App` pregunta por el bloque en la
cabeza y los pies.

**Alternativas descartadas.** (a) Agua **opaca** (sin pase nuevo): se ve como un
muro azul; no es un oceano. (b) Agua por **cutout** (alfa 0/255): no es
translucida. (c) Un unico pipeline con blending para todo: los bloques opacos
tendrian blending y orden incorrecto.

**Consecuencia.** `Block::Water` + `is_liquid`; `TILES` 10->11 y tile 10 en
`atlas.png`/fallback. `greedy_section_query` -> 4 buffers; `SectionMeshes` en el
renderer (opaco + agua); `ScenePipeline::water_pipeline`; `Renderer::is_water_at`.
`terrain.rs`: `coastal_block` + relleno de agua; `GENERATOR_VERSION` 5. Fisica:
parametro `in_water` en `PlayerController::update`. Demo `SOLARIA_OCEAN=1`. 117
tests.

### 2026-10-05 (v0.7.9) — Arboles, hojas transparentes y texturas de madera

**Decision.** Cerrar la **Etapa 1** con lo que faltaba: **vegetacion**
(arboles). Ademas, mejorar las texturas de **tronco/extremo/tablones** y hacer las
**hojas transparentes**, siguiendo el estilo de **Luanti** y la peticion del
usuario (con nuestra paleta).

**Motivo.**
* Los bloques `Wood`/`Leaves` existian desde v0.2 pero **no se generaban**: los
  biomas estaban pelados. Se anaden arboles deterministicos por bioma (bosque 5%,
  nieve 2%, desierto 0%), restringiendo el tronco a `x,z in 2..=13` para que la
  copa **quepa en la columna** y no se corte en el borde del chunk.
* **Hojas transparentes**: en el atlas tenian alfa 255 (opacas, con huecos
  oscuros). Se redibujan con **huecos de alfa 0** (~34%) y, al ser **no solidas**,
  el mesher las emite con **cutout** (el shader descarta alfa < 0.5) y el jugador
  las **atraviesa**, como en Minecraft/Luanti (variante "fancy").
* **Texturas de madera** (referencia Luanti): tronco con **veta vertical** de bajo
  contraste, extremo con **anillos** concentricos, y **tablones** (nuevo bloque
  `Planks`, tile 11) con tablas horizontales y juntas. Todo con nuestra paleta.

**Alternativas descartadas.** (a) Hojas **solidas**: el test de fisica detecto que
la copa genera techos y el jugador quedaba embebido (mismo fallo que los escalones
de v0.7.2); ademas MC "fast" usa hojas opacas. (b) Arboles que cruzan chunks: exige
una pasada de decoracion a nivel de mundo; se aplaza. (c) Hojas por blending (como
el agua): no; el cutout es lo correcto para follaje.

**Consecuencia.** `terrain.rs`: `hash01`/`hash_u32`/`place_tree`; `GENERATOR_VERSION`
6. `Block::Planks` (id 10, tile 11); `Leaves` pasa a **no solido + visible**; greedy
emite hojas por cutout. `atlas.png`/fallback redibujan tiles 5/6/7 y anaden el 11;
tile 7 con alfa 0. 118 tests. Demo de arboles verificada en captura.

**Nota de roadmap.** Con esto queda **cerrada la Etapa 1** ("mundo jugable") de
verdad. Siguiente: **Etapa 2 (gameplay, v0.8.x)**: hotbar/inventario, crafteo,
mobs y **guardado completo** (posicion del jugador).

### 2026-10-05 (v0.8.0) — Hotbar, inventario y guardado de posicion (Etapa 2)

**Decision.** Empezar la **Etapa 2 (gameplay)** con: **hotbar** de 9 ranuras,
**inventario** desplegable (`E`) y **guardado completo de la posicion** del
jugador. El arte de la interfaz sigue la referencia del usuario (opcion **D**:
marco de madera con ranuras hundidas).

**Motivo.**
* La seleccion de bloque era fija (teclas 1/2/3). Una **hotbar** es el minimo de
  "gameplay": elegir entre varios bloques e ir cambiando.
* "Guardado completo" (hito de la Etapa 2): hoy el mundo siempre reaparecia en el
  spawn; ahora se guarda y restaura la **posicion** del jugador.

**Render (nuevo).** La interfaz 2D necesita su propio pipeline: los vertices van
ya en **NDC** (la CPU convierte de pixels), sin z-buffer real (`depth_compare:
Always`, sin escritura) y con **blending alfa**. El fragment elige textura por la
**capa** del vertice: `-1` = textura de interfaz (`render::gui`), `>= 0` = tile del
atlas (icono de bloque), asi un solo pipeline dibuja marcos e iconos. La textura de
interfaz se **genera por codigo** (como el atlas procedural) para no meter un PNG
binario aparte y tenerla versionada.

**Alternativas descartadas.** (a) Dibujar la interfaz con el pipeline de escena
(3D): habria que pasar una proyeccion ortografica y perderia la simplicidad de
"pixels". (b) PNG de interfaz hecho a mano en LibreSprite: el usuario lo pidio,
pero crear un documento nuevo por script no es fiable; se genera por codigo y se
puede refinar luego. (c) Inventario con drag&drop: demasiado para v0.8.0.

**Consecuencia.** `render::gui` (textura) + `render::ui` (pipeline, `UiQuad`,
`region_uv`) + `ui.wgsl`. `Renderer::render` recibe los quads. `App`: `hotbar`,
`hotbar_sel`, `inventory_open`, cursor; teclas `1`-`9`, `E`, rueda. `save.rs`:
`WorldSave.player_pos` + `FORMAT_VERSION` 3 + migrador `V2ToV3` y lectura
compatible de v2 (`WorldSaveV2`). 123 tests.

**Pendiente (Etapa 2).** **Crafteo** (rejilla + recetas; la segunda referencia del
usuario) y **mobs**. El crafteo reusara `render::ui`.

### 2026-10-05 (v0.8.1) — Texturas cartoon y hotbar fiel a la referencia D

**Decision.** Reestilizado visual completo: (1) los 12 tiles del atlas se
**repintaron en LibreSprite** con la paleta "Solaria Cartoon" (3 tonos cercanos
por material, manchas suaves de 4x4, sin negro puro); (2) la hotbar es la
**referencia D medida del PNG** (marco `#4E351E`, divisores `#593E23`, ranuras
`#1C0B02/#2B190C/#352011/#311C0F`) y vive en `assets/gui.png` (256x160, mismo
layout de regiones) con fallback procedural.

**Motivo.** El arte anterior usaba ruido de 1px con motas casi negras: se veia
"sucio" en vez de cartoon. La doctrina aplicada (Luanti: 16x16 nativo con
nearest, pixel-art nitido; Terasology: acabado estilizado): paleta corta,
clusters suaves, luz cenital leve y bordes funcionales nitidos. Los dibujos son
originales (estilo inspirado, no copias) con nuestra paleta mejorada.

**Alternativas descartadas.** (a) Copiar texturas de Luanti tal cual: licencia
CC BY-SA y paleta ajena; se pinto original inspirado en su estilo. (b) Seguir
solo con procedural: el usuario pidio LibreSprite para todo el arte; el
procedural queda como fallback sin assets.

**Consecuencia.** `atlas::load_png_rgba` (decodificador compartido),
`gui::GUI_PATH` + `gui::load_pixels`, `Renderer` carga `gui.png` con fallback.
Fallback procedural con los mismos tonos base. 124 tests (nuevo: el PNG de GUI
coincide con el layout). Capturas `v0.8.1_hotbar` (hotbar D + iconos) y
`v0.8.1_spawn` (terreno con la nueva paleta).

**Nota de captura.** Las capturas cortas (<15 s) a veces salen sin hotbar porque
la ventana no llega a asentarse (el foco lo tiene otro proceso y
`SetForegroundWindow` falla); con espera larga la UI aparece siempre. No es bug
del juego (quads y config verificados): es entorno de captura.

### 2026-10-05 (v0.8.2) — Mesa de crafteo funcional estilo Minecraft

**Decision.** Mesa de crafteo completa con nuestra hotbar D: nuevo
`Block::CraftingTable` (id 11; tiles 12 lateral / 13 tapa; atlas 64x64), click
derecho sobre la mesa la abre (el resto coloca), rejilla 3x3 + flecha
pergamino + resultado, recetas en `world::recipe` con normalize como MC (1
madera -> tablones, 2x2 tablones -> mesa). Sin conteos (creativo): tomar el
resultado asigna a la ranura activa y limpia la rejilla.

**Motivo.** Es el crafteo de la guia (v0.8.2) y lo que el usuario pidio recrear
de su referencia: solo la parte de mesa, con nuestra hotbar en vez de la de
madera de la foto, mas la textura del bloque de mesa. La mesa es la puerta de
entrada: los tablones salen de ITEMS (se craftean) y entra la mesa.

**Alternativas descartadas.** (a) "Mano" con item flotando en el cursor: mas
estado y arte; el modelo sin-mano (inventario -> primera celda libre, celda ->
limpiar, resultado -> tomar) es simple y testeable. (b) Panel bitmap propio
para la ventana: las ranuras D ya se auto-enmarcan; se reutiliza SLOT_REGION y
solo se añade la region ARROW en el hueco libre de `gui.png`. (c) Abrir la mesa
tambien con `E`: `E` es del inventario; la mesa se abre/cierra con click
derecho, `E` y Escape.

**Consecuencia.** `world::recipe` (5 tests), `App`: `crafting_open`,
`craft_grid`, `craft_result`, `crafting_layout/click`, `hotbar_cells` compartido
entre dibujo y clic. Demo `SOLARIA_CRAFT=1`. El crafteo reusara esto para mas
recetas; los conteos llegaran con los drops (pendientes de la guia).

### 2026-10-05 (v0.8.3) — Segunda pasada de texturas: analisis MC/Luanti + dim

**Decision.** Repintar 8 tiles con criterio Minecraft/Luanti (16x16, luz
cenital, 3-4 valores, formas de 2-4px, sin ruido de 1px ni negros): tierra e
hierba con **terrones** 2x2, piedra con **manchas grandes** + motas grises,
arena casi lisa, corteza con **brillos ocres**, tablones con **nudos**, lateral
de mesa con **sierra** (hoja + mango). Ademas la UI **atenua el mundo** (negro
alfa 130 a pantalla completa) cuando hay ventana abierta. Se conservan
hojas/antorcha/nieve/agua/tapa de mesa (ya verificados).

**Motivo.** La captura del usuario mostro que, aun con la paleta cartoon, el
grano de 1px se leia "sucio" y la mesa no se distinguia de tablones. La doctrina
MC/Luanti resuelve ambos: estructura legible por tile y motivo propio en la
mesa. El dim mejora la legibilidad de rejilla y resultado sobre fondos claros.

**Alternativas descartadas.** (a) Clonar pixeles de MC/Luanti: dibujo original
con la doctrina, no copias. (b) Redimensionar `gui.png` para un panel de
crafteo bitmap: las ranuras D se auto-enmarcan; solo se añadio la region DIM
(8x8) en hueco libre.

**Consecuencia.** `atlas::grain` (ruido 2x2 en el fallback para hierba, tierra,
piedra y arena), region `gui::DIM` + quad de atenuado primero en `build_ui`
(test: el primer quad es fullscreen), motivo de sierra verificable en captura.
Cielo con dim medido: 120,181,247 -> 86,131,180 (mezcla 51% negro exacta).







### 2026-10-05 (v0.8.4) — Fisica AABB de entidades + simulacion de agua

**Decision.** Abrir dos sistemas de "mecanicas" adaptados a este motor (que
guarda **1 byte por voxel** con un `enum Block` sin campos, asi que **no** se
puede copiar el `Block::Water { level, source, flow }` del prompt original):

1. **Fisica AABB de entidades** (`src/physics.rs`): caja alineada a ejes,
   gravedad, resolucion de colision **eje a eje en orden X, Z, Y** (la vertical
   al final para que `on_ground` sea exacto), **anti-tunelado** (recorre el
   barrido completo, no solo la posicion final), friccion de suelo y
   **flotabilidad/arrastre** en el agua. Es la base de los **mobs**; no toca la
   fisica del jugador (cilindro propio).
2. **Simulacion de agua** (`src/world/water.rs` + `World::tick_water`):
   automata celular con **niveles 1..=8**, caida, propagacion horizontal con
   `FLOW_DECAY = 1` (una fuente forma un charco de radio `MAX_LEVEL - 1`, no
   inunda el mundo), igualacion de superficies, **fuentes** inagotables y
   **conservacion** en modo finito. El nivel **no cabe en el bloque**: se guarda
   en `World` como mapa de desbordes (`Block::Water` **sin** entrada = fuente;
   **con** entrada = flujo). Corre a **10 Hz**, apartado de la fisica y del
   render, con presupuesto de celdas por tick.

**Motivo.** El usuario pidio implementar el prompt de "fisica AABB + agua". Ese
texto asume entidades, un `Block` con campos y `Lava`, que aqui no existen; se
ha **adaptado** en vez de copiado (la propia guia lo pedia). El agua estatica
actual (mar relleno al nivel del mar) no reacciona a las ediciones: con el
automata, cavar bajo el mar o colocar agua produce flujo real.

**Alternativas descartadas.** (a) Meter el nivel en el `enum Block` (15
variantes `Water1..15`): contamina el enum, `face_tile` y el guardado por un
dato puramente de runtime. (b) `Block` con datos (`Water { level }`): rompe el
"1 byte/voxel", el meshing y el formato binario. (c) Flujo **hacia arriba** por
presion / cascada diagonal / evaporacion: el modelo MC-like no sube agua por
presion; se documentan fuera de alcance. (d) `Lava` + obsidiana: necesitan
bloques y texturas nuevas (los aporta la IA de diseno); quedan pendientes.

**Consecuencia.** `src/physics.rs` (AABB + `move_and_collide`, 6 tests) y
`src/world/water.rs` (`Fluid`, `FluidGrid`, `step_cell`, `DirtyQueue`, 8 tests).
`World` gana `water`/`water_queue`, `water_at`, `tick_water` e implementa
`FluidGrid`; `Renderer::tick_water` re-meshea el anillo 3x3 (el agua no emite
luz); `App` acumula el tick a 10 Hz. Un `Block::Water` colocado por el jugador
es fuente; el mar ya lo es sin marcarlo (sin entrada = fuente). **No** se
persisten los niveles de flujo: al recargar, el agua fluyente vuelve a ser
fuente hasta que la simulacion la drene (documentado). Bench: 100 ticks de una
charca 16x16 en ~2 ms (~0.02 ms/tick); el objetivo de "1M de bloques activos <
16 ms" **no** se alcanza con este diseno (sin paralelismo por chunk ni
almacenamiento compacto). 143 tests.

### 2026-10-05 (v0.8.5) — Clima, biomas avanzados, cuevas 3D y acuiferos

**Decision.** Reescribir la generacion del mundo (`GENERATOR_VERSION` 6 -> 7):

1. **Clima 2D** (`Fbm` de temperatura y humedad) -> **7 biomas** (desierto,
   sabana, llanura, bosque, pantano, taiga, tundra) por diagrama de Whittaker.
2. **Relieve por bioma**: `(amplitud, frecuencia, peso ridged)` por bioma; la
   taiga montana usa `RidgedMulti` para picos. La frecuencia se aplica escalando
   la coordenada (el ruido no admite frecuencia variable por muestra).
3. **Superficie variada**: un ruido de alta frecuencia por columna elige entre
   `Grass`, `CoarseDirt`, `Gravel`, `Podzol`, `Sand` y `Snow` (bloques nuevos
   `CoarseDirt`/`Gravel`/`Podzol`, ids 12-14).
4. **Cuevas** (`src/world/caves.rs` nuevo): spaghetti (`|Fbm|` pequeno),
   cheese (umbral en `Fbm` de baja frecuencia) y pillar (columnas solidas),
   con umbral que crece con la profundidad; protege bedrock y corteza.
5. **Acuiferos**: las cuevas bajo un nivel 2D (30..56) nacen con `Block::Water`,
   de modo que el automata a 10 Hz no inunda cavernas enteras.
6. **Agua**: fuentes **2x2** (un flujo con 2+ fuentes ortogonales se fija) y
   **equilibrio** (fuentes con fondo bloqueado y vecinos a tope se saltan: los
   oceanos generados cuestan cero).

**Motivo.** El usuario pidio el prompt de worldgen "vasto y realista". La
generacion v6 (Worley + Perlin simple) daba biomas de 3 tipos y cuevas de un
solo ruido.

**Alternativas descartadas.** (a) Frecuencia de ruido variable por bioma: habria
que reconstruir el `Fbm` por muestra; escalar la coordenada es equivalente y
barato. (b) Sin mascara 2D de cuevas: evaluar 3 `Fbm` 3D por celda en *todas* las
columnas hizo la suite subir de 7 a 47 s; la mascara 2D baja a ~6 s y reduce el
tiron de streaming. (c) Cuevas por "ruido > umbral" simple: da burbujas, no
tuneles. (d) Persistir niveles de flujo: sigue fuera de alcance.

**Consecuencia.** `terrain.rs` reescrito; `caves.rs` nuevo; `block.rs` +3
bloques; `atlas.rs` `TILES` 14 -> 17 (64x80) con tiles 14-16 procedurales;
`water.rs` fuentes 2x2; `World::water_in_equilibrium`; `physics::buoyancy_for`.
153 tests. **Asset pendiente**: `assets/atlas.png` es 64x64 y se ignora (medida
esperada 64x80) hasta que la IA de diseno lo repinte; mientras, atlas procedural.

### 2026-10-05 (v0.8.6) — Lava, obsidiana y texturas de tierras nuevas

**Decision.** (1) Pintar en LibreSprite los tiles 14-16 pendientes
(`CoarseDirt`, `Gravel`, `Podzol`) y crecer el atlas a 64x80. (2) Nuevos bloques
`Lava` (id 15, tile 17, liquido estatico que emite 15) y `Obsidian` (id 16, tile
18) con su arte (tiles 17-18). (3) Pozas de lava en cuevas profundas (y 6..11,
celdas de 3x3) con suelo de obsidiana; `GENERATOR_VERSION` 7 -> 8.

**Motivo.** El codigo v0.8.5 esperaba 17 tiles pero el PNG seguia en 64x64, asi
que el juego usaba el fallback procedural (las tierras nuevas se veian "de
codigo"). La lava/obsidiana las pidio el usuario para completar el set. La lava
es estatica a proposito: el automata de agua no la mueve (nueva
`Block::blocks_fluid`, usada en `FluidGrid::is_solid` y en equilibrio); el flujo
de lava y la reaccion agua+lava quedan para mas adelante.

**Alternativas descartadas.** (a) Lava con niveles del sim de agua: contaminaria
el mapa de fluido y el render de superficies; estatica + emision 15 da el 90%
visual con 0 riesgo. (b) Generar obsidiana como anillo por vecindad: necesita
leer columnas vecinas (con otra superficie); el suelo horneado bajo la lava es
local y siempre queda pegado a ella.

**Consecuencia.** Rama de liquidos del greedy generalizada (`is_liquid`), lava
al buffer translucido, nada en agua y flota igual. Demo `SOLARIA_CAVE=1` (busca
una poza real; si no hay, talla muestra). 155 tests (pozas con suelo, agua que
no entra en lava).

### 2026-10-05 (v0.8.7) — Optimizacion de worldgen (cache 2D, cuevas por densidad) y fluidos

**Decision.** Optimizar y refinar la generacion sin perder lo de v0.8.6 (lava):

1. **Cache de ruido 2D por columna**: `generate_column` evalua clima, bioma,
   altura, acuifero, variante y mascara de cuevas **una vez por `(x, z)`** y los
   reutiliza para todas las `y`. Antes `height()` y `biome_at()` recalculaban el
   clima por separado. Test con contador: ~2100 evaluaciones por columna, no
   ~18000 (que es lo que saldria si se llamara dentro del bucle `y`).
2. **Decoracion inteligente**: los arboles se recolectan en una pasada y se
   plantan en una segunda solo si la **pendiente** con las 4 columnas vecinas no
   supera 1 bloque y hay **hueco libre** 3x3 en `ground..ground+6`. Nada de
   arboles flotando sobre cuevas ni colgando de laderas.
3. **Transicion de biomas**: si el clima esta cerca de un borde (umbral en 0.035),
   la capa superior mezcla parches del material vecino, usando solo el clima ya
   calculado (sin evaluar columnas contiguas).
4. **Cuevas por campo de densidad** (`caves.rs`): `densidad = tuneles*0.7 +
   camaras*0.3`, con **atenuacion por profundidad** (0 en la corteza, 1 en
   `y=10`, 0 en la bedrock). Sustituye los umbrales separados por un unico campo.
5. **Agua**: la deteccion de **equilibrio** vive en `step_cell` (fondo firme y
   los 4 vecinos al mismo nivel -> `false` inmediato, sin re-encolar), y las
   **fuentes 2x2** se extraen a `check_2x2_source`. El oceano generado esta en
   equilibrio y no se procesa.

**Motivo.** El prompt del usuario pedia worldgen "rapido" y fluidos "casi
gratis"; y el cache 2D ataca el coste real (la generacion dominaba el test de
fisica del jugador). El campo de densidad da cuevas mas variadas que los dos
umbrales separados.

**Alternativas descartadas.** (a) Contador de ruido siempre activo: se deja como
`Cell<u32>` (micro-coste) para poder testear el cache. (b) Cuevas por `Worley`
3D para las camaras: `Fbm` de muy baja frecuencia ya da camaras grandes y es mas
barato. (c) Tocar `store.rs`: `World::water_in_equilibrium` (v0.8.6) sigue
sirviendo de filtro rapido de fuentes; la logica general queda en `step_cell`.

**Consecuencia.** `terrain.rs` (cache + decoracion + transicion + contador),
`caves.rs` (densidad + atenuacion), `water.rs` (`at_equilibrium`,
`check_2x2_source`, tests). 159 tests; clippy limpio. Conserva la lava/obsidiana
de v0.8.6 y sus tests.

### 2026-10-05 (v0.8.8) — Agua como liquido continuo (mesher + shader)

**Decision.** Sustituir el render del agua por cubos por una **lamina continua**:

1. **`world::fluid_mesher`**: por cada celda de agua se emite una cara superior
   cuyas 4 esquinas suben a `y + max(nivel, vecinos)/8` (se toma el maximo de las
   celdas que comparten la esquina). Eso convierte el escalon de dos niveles en
   una **rampa** (nivel 8 junto a nivel 4 -> esquina compartida a 1.0, esquina
   opuesta a 0.5). Solo se emiten superficie (si arriba no hay agua ni solido) y
   caras laterales expuestas al aire; el interior del agua no genera geometria.
2. **`water.wgsl`** (nuevo): shader propio con **UVs animadas** (`time`), mezcla
   de dos muestras con `sin` (ondulacion), normal por **derivadas** (`dpdx/dpdy`)
   y **especular Blinn-Phong**; el color se oscurece donde hay poca luz (cuevas).
3. **Pipeline**: la variante de agua usa `water.wgsl`, `cull_mode: None`,
   `depth_write_enabled: false` y `depth_compare: Less` (ya estaba; se mantiene).
   El `time` entra por el `uniform` reutilizando el primer `pad` (sigue en 112
   bytes, sin cambiar el layout).

**Motivo.** El usuario pidio que el agua "no se vea como bloques apilados".
Antes el greedy emitia el agua como cubos de altura completa; con niveles 1-8
habia escalones visibles.

**Alternativas descartadas.** (a) Meter la altura del agua en el vertice y
deformar en el vertex shader: no permite caras laterales conectadas correctas.
(b) Un buffer de uniform nuevo solo para agua: cambiaba el layout compartido;
reutilizar el pad no. (c) Teselar el agua (mas vertices): innecesario con la
interpolacion por esquinas.

**Consecuencia.** `fluid_mesher.rs` nuevo (tests: altura = nivel/8 y diferencia
de 0.5 entre niveles 8 y 4); `render/shaders/water.wgsl`; `pipeline.rs` (shader
de agua + campo `time` en el uniform); `renderer.rs` (usa `fluid_mesher` para el
agua, guarda `start: Instant` y pasa `time`). 161 tests; clippy limpio. El agua
del oceano (nivel 8) se ve igual que antes; las rampas aparecen en flujos y
bordes de nivel.

### 2026-10-05 (v0.8.9) — Inventario completo (todos los bloques) y antorcha 3D

**Decision.**
1. **Item system**: `ITEMS` pasa de 9 bloques fijos a la lista **completa** de
   bloques colocables (16). El inventario es una rejilla de 8 columnas que los
   muestra todos; la hotbar usa los 9 primeros (`HOTBAR_SLOTS`). Click en el
   inventario asigna el bloque a la ranura activa (ya existia).
2. **Antorcha 3D**: el mesher emite ademas del cruce de tablas el **palo
   central** de `assets/models/solaria_torch.bbmodel` (cubo 7..9 x 0..10 x 7..9),
   ligeramente inflado para no ser coplanar con las tablas (sin z-fighting) y
   mapeando solo la franja del palo del tile.
3. **Agua**: `check_2x2_source` extendida a la regla clasica (flujo con **2+
   fuentes** ortogonales -> `Source`), que con 3 fuentes + 1 bloque fija el
   manantial 2x2 sin convertir un charco normal en fuente infinita (conserva la
   conservacion en modo finito).

**Motivo.** El usuario aviso de que "no salen todos los bloques" (faltaban
tablones, agua, lava, obsidiana, tierra gruesa, grava, podzol) y pidio usar el
modelo de antorcha `.bbmodel`.

**Alternativas descartadas.** (a) Convertir los 4 de cualquier 2x2 de agua: un
unico manantial convertia todo su charco en fuentes (rompia el radio de
propagacion y la conservacion). (b) Cargar el `.bbmodel` con un parser JSON y
texturas propias: metia una dependencia (`serde_json`) y un pipeline de texturas
por modelo; el palo (que es lo que aporta el modelo sobre las tablas cruzadas)
se modela directo.

**Consecuencia.** `app.rs` (`ITEMS` completo, `inventory_cells` 8 columnas,
`HOTBAR_SLOTS`); `mesher.rs` (`emit_box` + palo de la antorcha, tests de conteo
actualizados); `water.rs` (`check_2x2_source` y su test). 161 tests; clippy
limpio. Backend de terreno/agua del prompt anterior (cache de ruido 2D,
acuiferos, deteccion de equilibrio) ya venia de v0.8.7/v0.8.8.

### 2026-10-05 (v0.8.10) — Antorcha corregida para coincidir con el .bbmodel

**Decision.** La antorcha se dibuja como el **palo** del modelo, no como una
cruz plana. El `.bbmodel` tiene tres cubos, pero el analisis de sus texturas (5
entradas: `torch`, `torch_mc`, `torch_sprite.png`, `torch_32.png`, `blank`)
revela que las tablas cruzadas usan la textura "blank" (transparente) en sus
caras grandes, asi que el unico elemento visible es el palo (7..9 x 0..10 x
7..9). Se emite como caja con UV **1:1** a la columna de la antorcha del sprite
(`u 7..9`, `v 6..16`): asi la llama queda arriba y el palo debajo, sin la
transparencia que dejaba la cruz plana casi invisible.

**Motivo.** En v0.8.9 se anadio el palo *ademas* de la cruz plana (que no debia
verse) y ademas se mapeaba el tile entero a una cara de 2px: en el juego la
antorcha salia como una aguja marron sin llama. El usuario lo comparo con
Blockbench (donde solo se ve el poste).

**Alternativas descartadas.** (a) Mantener la cruz plana: es lo que no coincidia
con Blockbench. (b) Renderizar los tres cubos con el tile: las tablas cruzadas
aparecerian cuando en el modelo son transparentes.

**Consecuencia.** `mesher.rs`: `emit_torch_cross` -> `emit_torch` (solo el palo
3D), helper `emit_box`; tests de conteo actualizados (24 vertices, 72 indices).
161 tests; clippy limpio. Captura `v0.8.10_antorcha.png` (poste con llama).

### 2026-10-05 (v0.8.11) — Agua interactiva y antorcha correcta

**Decision.**
1. **Antorcha**: deshacer el experimento de v0.8.9/v0.8.10. El `.bbmodel` tiene 3
   cubos pero, al extraer sus texturas (`tex3 = torch_32.png`, `tex4 = blank`),
   las caras **visibles** son las **tablas cruzadas** (`cross_x`/`cross_z`, tex3);
   el palo central usa `blank` y es **invisible**. Se vuelve a dibujar la cruz de
   dos planos con el tile entero (cutout), que es lo que se ve en Blockbench
   (una antorcha estrecha, no un poste).
2. **Agua: interaccion**. El raycast ya **no apunta liquidos** (`is_liquid` ->
   se ignora): el rayo los atraviesa, asi se rompe/coloca el bloque del fondo o
   de detras y el resaltado no marca el agua (antes marcaba el cubo de agua y no
   dejaba interactuar con lo de debajo).
3. **Agua: forma**. La superficie se baja a **14/16** del bloque (como MC), en
   lugar del 8/8 a tope: el agua deja ver un labio por debajo del borde y deja de
   parecer un cubo macizo.

**Motivo.** El usuario reporto que la antorcha se veia mal (comparada con
Blockbench), que el agua "seguia siendo un bloque" y que no podia colocar/romper
bloques en el agua.

**Alternativas descartadas.** (a) Poste 3D: la geometria que se ve en Blockbench
no es el palo. (b) Apuntar el agua y romperla: MC no deja; ademas se rellena al
instante (es fuente) y bloquea el acceso al fondo.

**Consecuencia.** `mesher.rs` (vuelve `emit_torch` a la cruz, se elimina
`emit_box`); `renderer.rs` (raycast ignora `is_liquid`); `fluid_mesher.rs`
(`surface_height` = nivel/8 * 14/16, tests actualizados). 161 tests; clippy
limpio. Captura `v0.8.11_oceano.png`.

### 2026-10-05 (v0.8.12) — Persistencia v4: columna completa, atomica y validada (auditoria P0)

**Decision.** Primera fase de la auditoria maestra (orden obligatorio: primero
correccion de datos). Se atacan cinco P0 de persistencia:

1. **Guardado vertical completo.** `ChunkRecord` guardaba una sola seccion
   (`TERRAIN_SECTION = 4`, `y=64..80`): las ediciones en otras alturas se
   perdian al reabrir. Ahora guarda la **columna entera** (`y0`,
   `height = 384`) y `apply_record` escribe el rango que el registro declare, de
   modo que conviven registros nuevos (columna completa) y migrados (una
   seccion). `FORMAT_VERSION = 4`.
2. **Migracion v3 -> v4.** Los archivos v3 (y v2) se leen con espejos
   (`WorldSaveV3`/`ChunkRecordV3`, `WorldSaveV2`): bincode es posicional, asi que
   `load_from` **espia la cabecera** (`format_version`) y elige el layout; luego
   `V3ToV4` fija `y0 = 64`, `height = 16` en los registros antiguos. Verificado
   en runtime con un `world.vf` real (`formato v4`, 2 chunks).
3. **Guardado atomico.** `save_to` escribe `world.vf.tmp`, hace `sync_all`, rota
   el anterior a `world.vf.bak` y renombra el temporal al definitivo. El archivo
   principal nunca queda truncado.
4. **Retry del guardado.** `save_world` marcaba `world_saved = true` **antes** de
   escribir; ahora solo lo marca si `save_to` devuelve `Ok`, y la cadena
   conserva la version original (`format_version.min(3)`).
5. **Cero perdida silenciosa.** Al cargar se valida cada registro: payload que no
   decodifica (`CorruptChunk`) o **id de bloque desconocido** (`UnknownBlock`,
   `Block::is_known_id` = ids `0..=Obsidian`) devuelven error en vez de
   convertirlos a aire.

Ademas: **registro perezoso** (dirty set). Antes cada `set_block` recompilaba y
comprimia la columna entera (98 KB); ahora solo se marca la columna sucia y el
registro se reconstruye al guardar o al descargar (`World::sync_modified`), lo
que tambien evita perder ediciones al salir del radio. Y el **highlight** deja de
crear una `Mesh` GPU por frame: se reutiliza si el bloque apuntado no cambia.

**Motivo.** El `TERRAIN_SECTION` fijo hacia que la persistencia no representara
el mundo de 384 de alto; era el P0 mas grave segun la auditoria.

**Alternativas descartadas.** (a) Guardar solo el "diff" contra el generador:
mas complejo y fragil ante cambios de generador. (b) Anadir campos a
`ChunkRecord` sin espejo: bincode posicional rompe la lectura de v3. (c) Guardar
en un hilo aparte (async) ya: se hara en la siguiente fase; aqui primero la
correccion.

**Consecuencia.** `save.rs` reescrito (formato v4, espejos, migrador, atomico,
validacion); `store.rs` (dirty set, `sync_modified`, `apply_record` por rango);
`app.rs` (retry de guardado); `renderer.rs` (highlight reutilizado);
`block.rs` (`is_known_id`). 166 tests; clippy `-D warnings` limpio. **Limites
conocidos**: el estado de fluidos dinamicos (niveles) aun no se persiste (los
bloques `Water`/`Lava` si); el guardado sigue sincrono en el hilo principal
(se abordara en la fase de save asincrono).

### 2026-10-05 (v0.8.13) — Guardado en segundo plano (auditoria P0: async save)

**Decision.** Sacar la serializacion y la E/S de disco del hilo principal:

1. **`engine::save_worker`**: un hilo recibe instantaneas `WorldSave` por canal,
   las codifica (`bincode`) y las escribe de forma **atomica** (`save_to`, ya de
   v0.8.12); devuelve un `SaveOutcome` (chunks, bytes, `Result`). El hilo
   principal no serializa ni toca disco.
2. **Autoguardado** cada 5 min (`AUTOSAVE_PERIOD`) en `App::update`, y
   `poll_save()` recoge los resultados. Reintentable si falla.
3. **Cierre seguro**: `finalize_save` pide el guardado, hace `join()` y recoge el
   resultado; es **idempotente** (CloseRequested + exiting).
4. **Prerequisito de streaming**: `TerrainGenerator` pasa de `Cell<u32>` a
   `AtomicU32` y queda `Send + Sync` (test `el_generador_es_send_y_sync`), listo
   para compartirse entre workers de generacion.

**Motivo.** El audit pide que el hilo principal nunca comprima/serialice/haga I/O
pesado. Aunque hoy solo se guardaba al cerrar (donde bloquear es tolerable),
esto sienta la base del autoguardado y del guardado en partida sin tirones.

**Alternativas descartadas.** (a) Mover tambien la compresion de las columnas
dirty al worker: requiere enviar bytes crudos y reconstruir registros; se deja
para cuando haya muchos chunks editados (hoy la compresion de las pocas columnas
dirty es despreciable). (b) Guardado 100% sin `join` al cerrar: riesgo de perder
el ultimo estado; se espera.

**Consecuencia.** `engine/save_worker.rs` nuevo; `app.rs` (worker, autoguardado,
`finalize_save`, `poll_save`); `terrain.rs` (`AtomicU32`). 169 tests; clippy
`-D warnings` limpio. Verificado en runtime: al cerrar, `world.vf` (6085 bytes) y
`.bak` rotado, sin errores en stderr. **Limites**: la compresion de columnas
dirty sigue en el hilo principal; el mundo guarda mientras el hilo carga el
`WorldSave` completo en memoria (sin streaming incremental de E/S aun).

### 2026-10-05 (v0.8.14) — Streaming de terreno por jobs (auditoria FASE 2)

**Decision.** Generar las columnas del mundo **fuera del hilo principal**:

1. **`world::streaming::TerrainScheduler`**: pool de 2 hilos que comparten el
   `TerrainGenerator` (`Arc`, `Send + Sync` desde v0.8.13) mediante un
   `Mutex<Receiver>`. Cada peticion lleva un **id monotonico**.
2. **`World::plan_streaming`** (encola el area faltante, descarga la que sale del
   radio, cancela peticiones viejas) + **`World::poll_generation`** (recoge
   resultados, valida `id` vigente y radio, e inserta la columna). El renderer
   combina ambos; ya no se congela al cruzar de chunk.
3. **Modelo `Loaded`/`Unloaded`**: `is_column_loaded`, y `is_solid_or_unloaded`
   para **fisica** (una columna sin cargar es muro -> el jugador no cae al
   vacio); el **raycast** no dispara a traves de lo no cargado; posar/meshing usan
   solo lo cargado (`is_solid`).
4. **Arranque**: `Renderer::warm_streaming` + `App` llaman a `World::warm_streaming`
   (carga **sincrona** del area del jugador, cancelando lo async pendiente) antes
   de posar/demo; y se **sanea** la posicion guardada si cae fuera del mundo.
   `update_streaming` (sincrono) se conserva para tests.

**Motivo.** El audit marca la generacion sincrona como P0 (tiron al cruzar de
chunk). Con jobs + revisiones, el frame no paga la generacion.

**Alternativas descartadas / bugs encontrados.**
* **Stack overflow**: la `Column` pesa ~98 KB; moverla por el canal/`collect` por
  valor desbordaba la pila de 1 MB del hilo principal en `debug`. Se **boxea**
  (`Box<Column>` en el canal y en `World::columns`). Verificado: con 64 MB de
  pila no fallaba -> era tamano de pila.
* **Camara cayendo**: `Unloaded = solido` hacia que `settle` posara al jugador en
  el techo (y≈384) y luego cayera; y esa posicion se guardaba. Se separan las
  consultas (fisica vs posar) y se sanea la posicion al cargar.

**Consecuencia.** `world/streaming.rs` nuevo; `store.rs` (`Arc` generador,
scheduler, pending/ids, plan/poll, warm, helpers de disponibilidad, `Box<Column>`);
`renderer.rs` (plan+poll, warm, `is_solid_loaded_at`, raycast); `app.rs` (warm +
saneamiento). 173 tests; clippy `-D warnings` limpio. Verificado en runtime:
posado en y=80.6 (no 384), streaming incremental (+33/+26/+21 -> 81), demo con
terreno. **Limites**: la fisica aun usa `Unloaded = solido` (puede frenar un
frame al entrar a un chunk pendiente); la luz sigue siendo reconstruccion global
en cada cambio de streaming; el meshing sigue en el hilo principal.

### 2026-10-05 (v0.8.15) — Luz de bloque incremental (auditoria P0)

**Decision.** Editar un bloque ya **no** recalcula la luz de bloque de todo el
mundo cargado. `World::relight_block(p, new_block)`:

1. **Remocion** (BFS): apaga la luz de `p` y, en cascada, la de las celdas que
   dependian de ella (`nl < level`); las celdas que tienen **otra** fuente
   (`nl >= level`) se **re-siembran** en la cola de adicion.
2. **Re-siembra** desde los vecinos con luz.
3. **Adicion** (BFS, solo sube): propaga desde las fronteras y desde la fuente
   nueva si `new_block` emite.

Ambas colas cruzan chunks (coordenadas de mundo) y quedan acotadas al alcance de
la luz (< 16 bloques). `set_block` la ejecuta; el renderer dejo de llamar a
`recompute_block_light()` en las ediciones (solo se mantiene en los cambios de
streaming, cuando entran/salen columnas con antorchas).

**Motivo.** El audit marca `recompute_block_light()` (recorrer y recalcular todo
el mundo cargado por cada edicion) como P0: picos de CPU al romper/colocar.

**Alternativas descartadas.** Recalcular una region local en vez de colas: dejaba
costuras oscuras en el borde de la region. La cola de remocion + adicion es el
algoritmo correcto y acotado.

**Consecuencia.** `store.rs` (`relight_block`, `put_block_light`, llamado en
`set_block`); `renderer.rs` (sin `recompute_block_light` en ediciones). Test
`la_luz_de_bloque_incremental_coincide_con_el_global` verifica que el resultado
coincide con el recalculo global sobre una zona amplia que cruza chunks. 174
tests; clippy `-D warnings` limpio. **Limite restante**: los cambios de streaming
siguen llamando al recalculo global de luz de bloque (columnas nuevas con
antorchas); se acotara despues.

### 2026-10-05 (v0.8.16) — Meshing por secciones (auditoria FASE 6, dirty sections)

**Decision.** La cola de (re)meshing pasa de **columna** a **seccion**
`(ChunkPos, section)`:

* `mesh_queue: VecDeque<(ChunkPos, usize)>`; `queue_section` / `queue_column`.
* `pump_meshing` meshea UNA seccion y actualiza solo ese `SectionMeshes` (crea el
  `ColumnMeshes` de la columna si falta); salta las secciones vacias.
* `build_column_meshes` -> `build_section_meshes`.
* Una **edicion** (`set_block`) encola solo la seccion editada; ademas la seccion
  contigua si el voxel toca un limite de seccion, y las columnas vecinas (y
  diagonales) si toca un borde de chunk -> `refresh_sections(voxel)`. Antes
  `refresh_area` reconstruia 9 columnas x 24 secciones (= 216 secciones) por
  edicion; ahora tipicamente 1.
* Streaming y agua siguen encolando todas las secciones de la columna (el pump
  salta las vacias); `set_blocks` (demo) y `tick_water` usan las nuevas colas.

**Motivo.** El audit pide meshing por **dirty sections** (§9.2): no reconstruir
una columna entera (ni 9) si solo cambio una seccion. Reduce el coste por edicion
de ~216 secciones a 1-4.

**Alternativas descartadas.** Meshing en workers de verdad (CPU mesh neutral +
upload en el hilo principal + revisiones): es el siguiente sub-paso; aqui primero
se elimina el trabajo redundante, que es lo que se nota al editar.

**Consecuencia.** `renderer.rs` (`mesh_queue` por seccion, `queue_section`/
`queue_column`, `build_section_meshes`, `refresh_sections`, fuera `refresh_area`;
`set_block`/`set_blocks`/`tick_water`/`apply_stream_change` adaptados). 174 tests;
clippy `-D warnings` limpio. Verificado en runtime: la demo aplica 561 edits y la
escena (parcela + antorcha) se ve correcta. **Limites**: el meshing CPU sigue en
el hilo principal (amortizado por el presupuesto del pump); los buffers GPU se
recrean por seccion (sin pool/reuso aun).

### 2026-10-05 (v0.8.17) — Meshing CPU asincrono con revisiones (auditoria FASE 6)

**Decision.** El greedy/fluido deja de correr en el hilo principal:

1. **`world::mesh_snapshot`**: `SectionSnapshot` copia el volumen `18x18x18`
   (seccion + anillo de 1 bloque) de bloques, luz y nivel de agua. Es `Send` y
   solo usa metodos **publicos** de `World` (`get_block`, `sky_light_at`,
   `block_light_at`, `water_level`). `mesh_snapshot()` genera la geometria con
   `greedy_section_query` + `fluid_section` sobre el snapshot (puro, sin `wgpu`).
2. **`render::mesh_worker::MeshScheduler`**: pool de 2 hilos que meshea snapshots
   y devuelve `(vertices, indices)` de opaco y agua.
3. **`Renderer`**: `pump_meshing` construye el snapshot (barato, lecturas
   directas) y manda el trabajo; `poll_meshing` (cada frame) valida la
   **revision** por `(columna, seccion)` y sube a GPU, descartando resultados
   obsoletos. Las secciones vacias se saltan sin snapshot (y limpian su malla).

**Motivo.** El audit (§9) pide separar **CPU mesh** (workers) de **GPU upload**
(hilo principal) con revisiones y sin re-meshear trabajo viejo.

**Alternativas descartadas.** Pasar closures/`&World` a los workers: no es `Send`
y acoplaria el renderer. El snapshot de una seccion (~23 KB) es barato de copiar
y desacopla por completo.

**Consecuencia.** `world/mesh_snapshot.rs` y `render/mesh_worker.rs` nuevos;
`renderer.rs` (scheduler + revisiones + `poll_meshing`; fuera `build_section_meshes`).
174 tests; clippy `-D warnings` limpio. Verificado en runtime: demo (561 edits) se
ve correcta, sin stderr; meshing repartido entre frames. **Limites**: los buffers
GPU se siguen creando por re-mesheo (sin pool/reuso aun); el snapshot se construye
en el hilo principal (5832 lecturas/job, barato pero no cero); sin LOD/batching.

### 2026-10-05 (v0.8.18) — Reuso de buffers GPU (auditoria FASE 6)

**Decision.** Al re-meshear una seccion no se crean/destruyen buffers GPU:

* `Mesh` reserva cada buffer con holgura (`next_power_of_two`) y guarda su
  capacidad; `Mesh::update(device, queue, vertices, indices)`:
  - si el nuevo tamano cabe → `queue.write_buffer` (sin allocacion);
  - si no cabe → recrea solo ese buffer con la nueva capacidad.
* `Renderer::poll_meshing` usa `update_mesh` (actualiza la `Mesh` existente de la
  seccion, o la crea si no habia). `Mesh::draw` sale si no hay indices.

**Motivo.** El audit (§9.4) pide reutilizar buffers GPU; crear un `Buffer` por
re-mesheo es churn (allocaciones + descriptors) y ademas se pagaba en cada
edicion de un bloque.

**Alternativas descartadas.** Arena/ring buffer global: mas complejo y no
necesario mientras el tamaño por seccion es acotado; per-section pool con
capacidad holgada ya elimina el churn.

**Consecuencia.** `render/mesh.rs` (capacidades + `update`), `renderer.rs`
(`update_mesh`). 174 tests; clippy `-D warnings` limpio. Verificado en runtime
(demo viva, captura correcta). **Limite**: la malla de una seccion que se
**vacia** se descarta en el `pump` (se pierde su buffer); se podria conservar.

---

### 2026-10-05 (v0.9.0) — Fluido local por columna (auditoria FASE 7, parte 1)

**Decision.** El estado del agua deja de ser un `HashMap<[i32; 3], Fluid>`
global. Los niveles de flujo pasan a la `Column`, empaquetados en **nibbles**
(4 bits; `MAX_LEVEL = 8`) y con asignacion **dispersa** (`Column.fluid:
Option<Box<[u8]>>` se reserva solo al escribir el primer flujo != 0). El flag
"fuente" no se almacena: se **infiere** de `Block::Water` con flujo 0. El
**active set** sigue siendo una cola deduplicada de celdas (`water_queue`); una
celda en equilibrio (oceano quieto) sale al procesarse y no se re-encola.

**Motivo.** El audit (P1 agua) pide estado de fluido local por seccion/columna,
niveles empaquetados y una simulacion de solo celdas activas. La version previa
guardaba un `HashMap<[i32; 3], Fluid>` global: hash costoso por consulta (el
mesher lee 18^3 celdas por seccion) y memoria proporcional a *todas* las celdas
de agua registradas, no a las que fluyen. "Fuente" es exactamente "agua sin
nivel de flujo", asi que inferirlo elimina el flag y no puede desincronizarse.

**Alternativas descartadas.**
- Mantener el `HashMap` y solo anadir persistencia: no arregla localidad ni el
  coste de hash en el mesher.
- Un `Vec<u8>` de nivel por celda (1 byte) en la columna: el doble de memoria;
  el nibble basta porque `MAX_LEVEL = 8`.
- Listas de celdas activas *por seccion*: la simulacion ya usa coordenadas de
  mundo (el agua cruza chunks) y la cola global deduplicada es el active set; una
  lista por seccion duplicaria el estado sin reducir el trabajo.
- Reservar siempre los 48 KB de nibbles por columna: dispararia la memoria de
  columnas sin agua que fluye; se reserva de forma perezosa.

**Consecuencia.** `world/chunk.rs` (almacen de flujo + `flow_at`/`set_flow`),
`world/store.rs` (se elimina `water: HashMap`; `water_at`/`set_water_raw` leen y
escriben la columna; nuevo `pending_water_cells`). 177 tests; clippy
`-D warnings` limpio. **Limites**: los niveles de flujo aun **no se persisten**
(van en v0.9.1) y el remesheo de agua sigue siendo por anillo 3x3 de columnas
(va en v0.9.2). Siguiente cuello de botella: que el flujo sobreviva a cerrar y
reabrir.

---

### 2026-10-05 (v0.9.1) — Persistencia de fluidos (auditoria FASE 7, parte 2)

**Decision.** El formato de archivo sube a **v5**: `ChunkRecord` gana el campo
`fluid` (nivel de flujo por celda, mismo indice `(y,z,x)` que `blocks`,
comprimido con LZ4; **vacio** si la columna no tiene agua que fluya). Se anaden
los espejos posicionales `ChunkRecordV4` y `WorldSaveV4` y el migrador
**v4 -> v5**, que deja `fluid` vacio: un mundo v4 no guardaba niveles, asi que
todo `Water` vuelve como **fuente**, que es exactamente como se comportaba.
`apply_record` restaura bloques y niveles de flujo.

**Motivo.** El audit (P1 agua, §11.5 / §5.6) exige que el agua que fluye
sobreviva a cerrar y reabrir. Antes solo vivia en el `HashMap` en memoria (v0.9.0
lo movio a la columna), asi que al recargar todo el flujo volvia a fuente y el
mundo "perdia" el nivel real.

**Alternativas descartadas.**
- Empaquetar `fluid` en un *nibble* tambien en disco: el archivo se comprime con
  LZ4 y el nibble complicaria `from_column`/`apply_record` por ~1 byte/celda
  antes de comprimir; no compensa (la memoria en RAM ya es nibble).
- Fusionar bloques y fluido en un unico payload comprimido: obligaria a un
  formato entrelazado y a reescribir el migrador de bloques; campos separados
  mantienen el layout simple y aditivo.
- Reconstruir el flujo por simulacion al cargar: no es determinista y pagaria
  CPU en cada carga; el agua no es derivable del bloque.

**Consecuencia.** `world/save.rs` (`FORMAT_VERSION = 5`, `fluid`, espejos v4,
`V4ToV5`, `decompressed_fluid`, `is_corrupt` valida ambos campos),
`world/store.rs` (`apply_record` restaura flujo). 181 tests (incluye roundtrip
de flujo, migracion v4->v5 y supervivencia a descarga/recarga); clippy
`-D warnings` limpio. **Limites**: el remesheo de agua sigue siendo el anillo 3x3
de columnas (v0.9.2) y la transparencia no esta ordenada (v0.9.3). Siguiente
cuello de botella: el coste de re-meshear de mas al fluir agua.

---

### 2026-10-05 (v0.9.2) — Remeshing incremental de fluidos (auditoria FASE 7, parte 3)

**Decision.** `World::tick_water` devuelve `Vec<FluidDirty>` (seccion + marcas de
borde X/Z de chunk) en vez de `Vec<ChunkPos>`. El renderer encola **solo** la
seccion afectada y sus vecinas verticales; si el cambio toco un borde de chunk,
tambien las secciones correspondientes de la(s) columna(s) vecina(s). La
simulacion pasa a tener un presupuesto configurable `FluidBudget { cells, ms }`
(por entorno con `SOLARIA_FLUID_BUDGET_CELLS` / `SOLARIA_FLUID_BUDGET_MS`).

**Motivo.** El audit (§11.4) pide no re-meshear el anillo 3x3 completo si solo
cambio una celda. El codigo anterior re-mesheaba 9 columnas x 24 secciones por
cada tick con agua activa; en una cascada se disparaba el coste de meshing. La
geometria de agua de una seccion lee la celda de arriba (cara superior) y las
laterales de su misma `y`, asi que hacen falta la seccion y las verticales
colindantes; el borde X/Z solo afecta a la columna vecina.

**Alternativas descartadas.**
- Devolver solo `ChunkPos` y volver a encolar la columna entera: no reduce el
  trabajo (24 secciones por columna).
- Devolver la posicion local de cada celda: no hace falta; las marcas de borde
  bastan y evitan un tipo mas grande.
- Presupuesto solo por celdas: una cascada con celdas baratas pero muchas puede
  seguir pasandose de tiempo; la cota de ms lo acota en hardware lento.

**Consecuencia.** `world/water.rs` (`FluidBudget`), `world/store.rs` (`FluidDirty`,
`tick_water_with`, `add_fluid_dirty`, `tick_water` delega),
`render/renderer.rs` (`queue_fluid_dirty`, `vertical_neighbor_sections`),
`engine/app.rs` (presupuesto por entorno). 185 tests; clippy `-D warnings`
limpio. **Limites**: sigue habiendo over-queuing de las secciones verticales
vecinas (correcto pero no minimo); no se midio aun el ahorro con benchmark
(FASE 13). Siguiente cuello de botella: el orden de dibujo del agua (v0.9.3).

---

### 2026-10-05 (v0.9.3) — Transparencia ordenada del agua (auditoria FASE 7, parte 4)

**Decision.** El pase translucido del agua deja de iterar el `HashMap` de mallas
sin orden: cada frame se recogen las secciones con malla de agua que pasan el
frustum en `water_order: Vec<(dist2, ChunkPos, seccion)>` (buffer reutilizado, sin
allocar por frame), se ordenan de **lejos a cerca** por distancia al centro de la
seccion a la camara y se dibujan en ese orden. Se mantiene **z-test ON y z-write
OFF** (ya configurado en `pipeline.rs`).

**Motivo.** El audit (§11.5) pide transparencia correcta: el blending alfa es
sensible al orden y el orden de un `HashMap` no esta definido, asi que el agua
podia componerse de forma inconsistente entre frames/campo de vision. El agua no
debe escribir z (taparia las caras de agua que tiene detras) pero si consultarlo
(no debe dibujarse sobre geometria opaca delante).

**Alternativas descartadas.**
- Ordenar por columna solo (sin seccion): insuficiente con agua en varias alturas
  de la misma columna.
- OIT (order-independent transparency) / weighted-blended: mas complejo y
  costoso; no hace falta para una capa de agua discreta.
- `HashMap` ordenado / `BTreeMap` de meshes: orden por clave, no por distancia;
  no resuelve la composicion.
- Recalcular distancias de todos los vertices: el centro de la seccion basta para
  ordenar secciones y es O(n).

**Consecuencia.** `render/renderer.rs` (`water_order`, `sort_water_back_to_front`,
pase de agua). 186 tests; clippy `-D warnings` limpio. Verificado en runtime
(demo de oceano: agua translucida correcta, 396 fps). **Limite**: el orden es por
seccion, no por triangulo; con varias capas de agua muy solapadas podria quedar
algun artefacto, aceptable para el modelo actual. **FASE 7 cerrada.** Siguiente
cuello de botella: la duplicacion de definiciones de bloque (FASE 9, registry).

---

### 2026-10-05 (v0.10.0) — Registro central de bloques (auditoria FASE 9)

**Decision.** La metadata de los bloques se centraliza en `world/registry.rs`
(`BlockDefinition` + tabla `BLOCKS` + fachada `BlockRegistry`). `Block` sigue
siendo un `u8` y delega sus consultas (`is_solid`, `is_visible`, `is_liquid`,
`blocks_fluid`, `light_emission`, `face_tile`, `name`, `render_kind`,
`hardness`) en la tabla. `atlas::TILES` se deriva del tile mas alto del registro
(`TILE_COUNT`); `app.rs` usa `BlockRegistry::items()` en vez de su propio `ITEMS`;
`Block::from_u8`/`is_known_id` se derivan de `ALL_BLOCKS`.

**Motivo.** El audit (FASE 9, §13) pide una definicion central para eliminar la
duplicacion entre `block.rs`, `ITEMS` de `app.rs` y los tiles de `atlas.rs`. Esa
duplicacion es una fuente real de bugs: anadir un bloque obligaba a tocar el
`match` de `face_tile`, la lista del inventario y `TILES` por separado, y nada
detectaba un olvido.

**Alternativas descartadas.**
- Meter el estado en el propio bloque (struct por voxel): rompe el objetivo de
  1 byte/voxel; el registro es externo y de solo lectura.
- Un `HashMap<u8, BlockDefinition>` cargado en runtime: indireccion y memoria de
  mas para un conjunto fijo; una tabla `const` indexada por id es O(1) y sin
  asignaciones.
- `Box<dyn>`/trait objects para el registro: sobre-ingenieria para una tabla
  estatica; se descarta segun la regla del proyecto de no abstraer por estetica.
- Reordenar el inventario al orden de id: cambiaria el UX visible; se conserva
  el orden de `PLACEABLE_ITEMS` y un test garantiza que cubre exactamente los
  bloques marcados `item`.

**Consecuencia.** Nuevos `world/registry.rs` + `pub mod registry` y re-exports.
`block.rs` (delega), `atlas.rs` (`TILES` derivado), `app.rs` (sin `ITEMS`).
192 tests (nuevos: ids contiguos, tiles en rango, colocables == items, flags
historicos, coherencia de `RenderKind`); clippy `-D warnings` limpio. **Limite**:
`hardness` queda como dato reservado (aun no hay tiempos de minado) y el
`RenderKind` documenta la intencion pero el greedy todavia clasifica por flags
(migracion futura). Siguiente cuello de botella: medir memoria por categoria
(FASE 10).

---

### 2026-10-05 (v0.11.0) — Memoria: medir por categorias y luz de bloque dispersa (auditoria FASE 10)

**Decision.** (1) Se anade `world/memory.rs` (`WorldMemory`) y
`World::memory_report()`, que miden el mundo cargado por categorias (bloques,
luz de cielo, luz de bloque, fluido, cabeceras, registros editados, cola de
agua) y se imprimen al arrancar. (2) `Column.block_light` pasa de `Vec<u8>` a
`Option<Box<[u8]>>` **disperso**: sin emisores no reserva 98 KB y
`clear_block_light` libera en vez de rellenar. (3) Se **decide no** hacer
bit-packing (paleta 1/2/4/8 bits) de bloques/luz todavia.

**Motivo.** El audit (FASE 10) pide medir antes de cambiar la representacion y
solo aplicar bit-packing si las mediciones lo justifican. Medicion real (radio 4,
81 columnas, mundo de terreno): bloques 7.6 MB, luz de cielo 7.6 MB, luz de
bloque 6.2 MB, cabeceras ~0.04 MB; total ~21.5 MB (~265 KB/columna). La luz de
bloque era ~1/3 pero solo 15 de 81 columnas no tienen emisores (hay lava
repartida), asi que el ahorro de memoria es modesto (~1.7 MB); el beneficio
grande es de **CPU**: elimina el `memset` de 98 KB por columna en cada cambio de
streaming (era 81 x 98 KB = ~8 MB de escritura por cruce de chunk).

**Alternativas descartadas.**
- Bit-packing de bloques por seccion (paleta + indices): es el mayor ahorro
  potencial, pero reescribe `Chunk::get/set`, meshing, luz y save; sin un
  benchmark (FASE 13) el riesgo de degradar CPU/cache supera a ~7.6 MB de ahorro
  a radio 4. Se pospone con datos.
- Empaquetar la luz a 4 bits (mitad): mismo argumento; los accesos de luz son hot
  path (meshing y BFS) y el shift/mask podria costar mas de lo que ahorra.
- Cache LRU caliente/templada/fria: el streaming ya **descarga** columnas fuera
  del radio (frio) conservando sus ediciones en `modified`; un LRU de columnas
  recien descargadas es una optimizacion de latencia, no de memoria, y se pospone
  a FASE 11 (renderer scale).
- Contar tambien la memoria GPU en `WorldMemory`: rompe la frontera `world`/`render`;
  el renderer expone `gpu_mesh_bytes()` y el arranque lo imprime aparte.

**Consecuencia.** `world/memory.rs`, `World::memory_report`, `Column` (block_light
disperso + `blocklight_bytes`/`skylight_bytes`/`fluid_bytes`), `render/mesh.rs`
(`gpu_bytes`), `render/renderer.rs` (`world_memory`/`gpu_mesh_bytes`/...),
`engine/app.rs` (traza `[mem]`). 194 tests (nuevos: no reserva sin emisores,
liberacion, informe por categorias); clippy `-D warnings` limpio. **Limite**:
la traza de GPU sale al arrancar (antes de que la cola de meshing se vacie), asi
que el dato de GPU no es representativo aun. Siguiente cuello de botella: draw
calls y culling con mundo grande (FASE 11).

---

### 2026-10-05 (v0.12.0) — Culling por distancia y metricas de frame (auditoria FASE 11, parte 1)

**Decision.** Se anade `FrameStats` (columnas, secciones dibujadas, draw calls,
triangulos, culls por frustum/distancia) al renderer, y un **culling jerarquico
por distancia** sobre el frustum: una seccion cuya AABB entera queda mas alla de
`FOG_END` (64) se descarta porque la niebla la cubre por completo (su color es el
del cielo). La distancia se mide punto-AABB (`nearest_dist2`). Los stats se
muestran en el titulo y, con `SOLARIA_STATS=1`, se trazan por consola.

**Motivo.** El audit (FASE 11, §17) pide culling jerarquico (frustum -> distancia
-> seccion -> chunk) y **medir antes/despues**. Hasta ahora todo lo que pasaba el
frustum se dibujaba aunque estuviera totalmente en la niebla.

**Medicion (vista de oceano, radio 4, 81 columnas).**
```
con culling por distancia:  dc=128  tri=36986  cull_frustum=178  cull_dist=164
sin culling por distancia:  dc≈292  (128 + 164)
```
~56% menos draw calls. Los triangulos bajan en la misma proporcion en las
secciones cullidas.

**Alternativas descartadas.**
- Batching por columna (fusionar las 24 secciones en una malla): reduce draw
  calls pero **sube** el coste de re-meshing en cada edicion (habria que
  reconstruir la columna entera) y complica las revisiones por seccion; se
  pospone hasta medir el cuello real con mundos mayores.
- Indice indirecto / GPU-driven: sobre-ingenieria sin medir que ayude (regla del
  audit: solo si el benchmark lo justifica).
- LOD (near/mid/far): gran cambio de calidad; se pospone a una fase posterior.
- Culling por umbral menor que `FOG_END`: cortaria geometria visible; `FOG_END`
  es el limite exacto en el que la niebla es total.

**Consecuencia.** `render/renderer.rs` (`FrameStats`, `nearest_dist2`,
`axis_distance`, culling + contadores, `frame_stats`), `render/mesh.rs`
(`index_count`), `engine/app.rs` (titulo con dc/tri + traza `SOLARIA_STATS`).
195 tests (nuevo: distancia punto-AABB); clippy `-D warnings` limpio. **Limites**:
el culling por distancia usa la AABB de seccion, no un octree/region; no hay
batching ni LOD todavia. Siguiente cuello de botella: la fisica/`player` no usan
todavia un timestep fijo (FASE 12).

---

### 2026-10-05 (v0.13.0) — Fisica a timestep fijo y colisiones unificadas (auditoria FASE 12)

**Decision.** (1) La fisica del jugador corre a **timestep fijo**
(`FIXED_DT = 1/120`) con un acumulador acotado (`MAX_FIXED_STEPS = 8`,
`MAX_ACCUMULATOR = 0.25`): cada frame acumula el tiempo real y ejecuta `N` pasos
de duracion constante. El giro de camara sigue siendo por frame. (2)
`physics::box_hits_solid` es la **consulta de colision comun** (AABB vs voxeles)
que usa el jugador; gravedad, tope de caida y escala de gravedad en agua pasan a
ser una unica constante en `physics`, reexportada por `player::controller`. (3)
Nuevo `VoxelAvailability::{Loaded(Block), Unloaded, OutOfBounds}`: modelo
explicito que la fisica usa para no tratar un chunk sin cargar como aire.

**Motivo.** El audit (FASE 12) pide timestep fijo para no atar fisica/IA al
framerate (determinismo, base de mobs/proyectiles/multijugador), y una base comun
de colision para que jugador y entidades no diverjan. La gravedad estaba
duplicada literalmente en dos modulos (28.0 / 50.0 / 0.30): un cambio en uno
dejaba al jugador comportandose distinto a los mobs.

**Alternativas descartadas.**
- Timestep variable suavizado: sigue dependiendo del framerate; no da
  determinismo.
- **Render interpolado**: el audit lo sugiere, pero el `Camera` actual guarda
  una sola `position` que usan raycast/colocar/highlight; interpolar requiere una
  transformacion de render separada. Se pospone como mejora (no afecta a la
  correccion del timestep fijo).
- Tercio/redondeo de pasos: el acumulador con tope es mas simple y suficiente;
  el tope evita la "espiral de la muerte".
- Unificar toda la resolucion de movimiento (sweep) entre jugador y entidades:
  el jugador necesita auto-step y huella (cilindro), las entidades un sweep X/Z/Y;
  se comparte la **consulta** (que es lo que puede divergir), no el algoritmo de
  resolucion (el audit lo permite explicitamente).

**Consecuencia.** `physics.rs` (`box_hits_solid`, unica fuente de constantes),
`player/controller.rs` (reexporta constantes + usa `box_hits_solid`),
`world/store.rs` (`VoxelAvailability` + `availability`, `is_solid_or_unloaded`
reescrito), `engine/app.rs` (`FIXED_DT`, acumulador, `simulate_player`,
`fixed_steps`). 199 tests (nuevos: consulta de caja, disponibilidad, pasos fijos
y tope); clippy `-D warnings` limpio. Verificado en runtime (jugador posado sin
panic). **Limites**: sin interpolacion de render; no hay aun pruebas de
determinismo secuencial vs paralelo (pendiente). Siguiente cuello de botella:
faltan diagnosticos/overlay y benchmarks (FASE 13).

---

### 2026-10-05 (v0.14.0) — Diagnosticos y benchmarks (auditoria FASE 13)

**Decision.** (1) Overlay **F3** (o `SOLARIA_STATS=1`) que muestra en el titulo de
la ventana fps, tiempos de `update`/`render`, draw calls, triangulos, columnas,
cola de meshing, memoria del mundo/GPU y estado de guardado; ademas traza una
linea `[stats]` cada ~0.5 s. (2) `world::bench` (solo `#[cfg(test)]`): benchmarks
reproducibles de generacion, meshing, luz incremental, fluidos y guardado. (3)
`docs/performance.md` con la metodologia y las mediciones.

**Motivo.** El audit (FASE 13) pide un overlay de diagnostico, un `FrameStats`
ligero y benchmarks reproducibles con informe. Las optimizaciones previas
(culling, memoria, fluidos) necesitaban una forma de **medirse** de forma estable.

**Alternativas descartadas.**
- Overlay de **texto en pantalla**: requiere una fuente bitmap (no la hay; el
  `gui.png` son marcos y ranuras). Implementar un renderer de texto es una fase
  propia; por ahora el titulo de la ventana cumple la funcion sin "features de
  lujo" antes de la base. Se documenta como pendiente.
- Framework de profiling externo (tracy, puffin): una dependencia pesada para
  algo que el propio motor puede medir con `Instant`.
- `criterion`: anade dependencias y un runner aparte; los `#[test]` que imprimen
  tiempos ya son reproducibles con `--nocapture` y no rompen `cargo test`.
- Toggles de wireframe/bordes/luz/fluido (F4-F7): necesitan un pase de debug
  (lineas/billboards) que no existe; se posponen junto con el overlay de texto.

**Consecuencia.** `engine/app.rs` (`show_stats`, `update_ms`/`render_ms`,
`title_line`, tecla F3), `world/bench.rs`, `docs/performance.md`. 204 tests;
clippy `-D warnings` limpio. **Cuello pendiente medido**: la luz de bloque en
cambios de streaming sigue siendo global (~19-20 ms/cruce, ver
`docs/performance.md`). **Fases de la auditoria completadas: 7, 9, 10, 11, 12 y
13.**

---

### 2026-10-05 (v0.15.0) — Migracion v1 real y test de determinismo

**Decision.** (1) Se anaden `ChunkRecordV1`/`WorldSaveV1` (formato v1: una
seccion de 4096 bytes **sin comprimir**, sin el flag `compressed` ni
`player_pos`). `WorldSave::load_from` elige el layout por la version de la
cabecera y, para una version desconocida, devuelve `SaveError::NoMigration` en
vez de decodificar con el layout equivocado. (2) Test de determinismo:
`TerrainScheduler` (4 workers) y la generacion secuencial producen el **mismo
hash FNV-1a de columna** para 49 posiciones.

**Motivo.** El handoff anotaba "migracion real probada con archivos v1/v2/v3
(hoy v2/v3 con espejos; v1 sin compressed no soportado de verdad)" y "test de
determinismo secuencial vs paralelo". El arm `_` de `load_from` decodificaba
v1/v2 con el layout de v2 (que espera `compressed`), lo que **desalineaba** los
bytes de un v1 real; un test sintetico con cabecera 0 lo ocultaba.

**Alternativas descartadas.**
- Seguir usando el layout v2 para v1: incorrecto (bincode es posicional); daba
  falsas garantias.
- Version 0: no existio nunca; ahora es un error claro.
- Cambiar `FORMAT_VERSION` por esto: no cambia el layout de escritura, solo se
  **anade lectura** de un formato antiguo; `FORMAT_VERSION` se queda en 5.

**Consecuencia.** `world/save.rs` (`ChunkRecordV1`, `WorldSaveV1`,
`upgrade_v1_record`, `load_from`), `world/streaming.rs` (test de determinismo +
`hash_column`). 206 tests; clippy `-D warnings` limpio. **Limite**: no hay fixture
binaria v1 en disco (se sintetiza en el test); la migracion de v0 no existe
(nunca hubo). Siguiente cuello de botella: luz de bloque en cambios de streaming
(global, ~19-20 ms/cruce).

---

### 2026-10-05 (v0.15.1) — Cache de emisores de luz por columna

**Decision.** `Column` guarda una **cache perezosa de emisores** de luz
(`Vec<(indice local, nivel)>`), construida al primer acceso y **invalidada en
`Column::set`**. `World::recompute_block_light` recoge los emisores de esa cache
en vez de escanear las 24 secciones de cada columna.

**Motivo.** La medicion (`docs/performance.md`) situo el recalculo de luz de
bloque al cruzar de chunk en ~19 ms. El grueso era el barrido de emisores: por
cada cambio de streaming se recorrian las secciones no vacias de las 81 columnas
buscando `light_emission() > 0`, aunque solo cambiasen 9 columnas.

**Medicion (dev, opt-level 1).**
```
recompute_block_light (cruce de chunk):  19.03 ms -> 11.95 ms  (-37%)
recompute_block_light (frio, 1a llamada): ~21 ms (igual: construye la cache)
```
Solo las columnas que cambian tienen la cache fria; las demas se reutilizan.

**Alternativas descartadas.**
- **Luz de bloque regional** (como la de cielo): es el objetivo final, pero
  requiere resolver correctamente la **remocion** de la luz de columnas que se
  descargan (un chunk con lava/torch que se va deja luz obsoleta en los vecinos).
  Es mas invasivo y arriesgado; se pospone con la cache como paso intermedio
  seguro.
- Mantener un `HashSet` de emisores en el `World` actualizado en `set_block`:
  duplicaria el estado y complicaria la carga de columnas generadas/restauradas;
  la cache por columna se invalida sola y viaja con la columna.

**Consecuencia.** `world/chunk.rs` (`emitters`, `emitters()`), `world/store.rs`
(`recompute_block_light`). 207 tests; clippy `-D warnings` limpio. **Limite**: el
recalculo sigue siendo global (recorre todo el mundo cargado); la cache solo
elimina el barrido. Siguiente cuello: luz de bloque regional + remocion al
descargar columnas.

---

### 2026-10-05 (v0.15.2) — Luz de bloque regional (siembra de frontera)

**Decision.** `World::recompute_block_light_region(changed)` limpia y reconstruye
la luz de bloque solo de la **region** = `changed` (columnas cargadas/descargadas)
mas su anillo de 1 columna, en vez de todo el mundo. Como la luz viaja 15 bloques
(< 1 chunk), la region cubre todo lo que puede cambiar. Ademas siembra la
**frontera** de la region desde la luz **preservada** de las columnas de fuera,
para no perder la luz que entra desde mas alla del anillo (sin bordes oscuros).
El renderer usa esta ruta en `apply_stream_change`; `recompute_block_light` se
mantiene como referencia y en tests.

**Motivo.** El audit pide luz incremental/regional y la medicion
(`docs/performance.md`) mostraba ~12 ms por cruce tras cachear emisores, con el
BFS propagando por **todo** el mundo cargado. El recálculo regional acota el
trabajo a las columnas afectadas y, sobre todo, **escala con el radio** (O(perimetro)
en vez de O(area)).

**Medicion (dev, opt-level 1, cruce +9/-9).**
```
recompute_block_light (global, cache):   12.04 ms
recompute_block_light_region (region):   10.69 ms   (era 19.03 ms en v0.15.0)
```
La mejora es modesta con **lava densa** (el BFS desde emisores es casi constante
al radio), pero la ruta regional no depende del area del mundo.

**Alternativas descartadas.**
- Igual que v0.15.1 (solo cache, global): no acota el BFS ni mejora al crecer el radio.
- BFS acotado estrictamente a la region (sin propagar fuera): perderia la luz que
  un emisor nuevo de la region proyecta hacia fuera; ademas la propagacion hacia
  fuera solo **sube** luz ya valida, asi que es inocua.
- Sembrar la frontera recorriendo TODAS las celdas de la region: O(celdas). Solo
  se recorren las **caras** de las columnas de borde que dan a fuera (4 x 384 x 16
  por columna de borde), mucho mas barato.

**Consecuencia.** `world/store.rs` (`recompute_block_light_region`,
`collect_block_light_emitters`, `seed_block_light_boundary`,
`propagate_block_light`, `recompute_block_light` refactorizado),
`render/renderer.rs` (ruta regional). 208 tests (nuevo: equivalencia
regional==global con altas/bajas/frontera); clippy `-D warnings` limpio. **Bug
corregido en el camino**: el BFS re-encolaba celdas de columnas no cargadas
(`put_block_light` es no-op) → bucle infinito; ahora escribe directo en la
columna y solo encola si escribio. **Limite**: con lava muy densa el coste sigue
dominado por el BFS. Siguiente cuello: batching/LOD (draw calls) o interpolacion
de render.

---

### 2026-10-05 (v0.15.3) — Hardening de tests (raycast y streaming)

**Decision.** Se anaden los casos de prueba de robustez que pedia la auditoria:
`raycast` (origen dentro de un bloque, rayos negativos, direccion nula, direccion
casi cero, `max_distance = 0`, borde de chunk x=15->16, predicado que atraviesa
liquidos, rayo sobre un borde de celda, diagonal en coordenadas negativas) y
streaming (carga y edicion en **chunks negativos**).

**Motivo.** El audit (§21 "Raycast robusto" y §41 "tests obligatorios") listaba
estos casos; solo habia 4 tests de raycast, todos de casos "comodos". La logica
del DDA (empates de `t_max`, signos, coordenadas negativas) es justo donde
aparecen bugs sutiles.

**Resultado.** Los 10 casos nuevos (mas el de chunks negativos) pasan **sin
cambios de codigo**: el raycast ya era correcto en bordes, negativos y empates.
El valor es de **red de seguridad** (regresion) mas que de bugfix.

**Consecuencia.** `world/raycast.rs` (10 tests), `world/store.rs` (1 test).
218 tests; clippy `-D warnings` limpio. Sin cambio visible en runtime.
Siguiente cuello: batching/LOD, interpolacion de render o overlay de texto.

---

### 2026-10-05 (v0.16.0) — Radio de vista configurable (y por que NO hay batching/LOD)

**Decision.** El radio de carga/render se configura con `SOLARIA_VIEW_RADIUS`
(1..=12, por defecto 4). La niebla (`fog_start/fog_end`) y el **culling por
distancia** se derivan de el (`fog_end = radio * 16`, `fog_start = 0.625 *
fog_end`, mismo ratio que 40/64). Se documenta el escalado medido y se decide
**no** implementar batching/LOD.

**Motivo.** El renderer estaba a radio fijo 4 sin forma de escalarlo ni medirlo.
Ahora se puede subir la vista y comprobar el coste con datos.

**Medicion (dev, opt-level 1, vista de oceano).**
```
radio 4:  81 col | ~22 MB | 128 dc |  37k tri | render ~2.0 ms
radio 6: 169 col | ~44 MB | 340 dc | 132k tri | render ~2.4 ms
radio 8: 289 col | ~76 MB | 530 dc | 187k tri | render ~2.4 ms
```

**Por que no batching/LOD.** El culling por distancia (niebla) hace que el coste
de render sea **casi plano** al subir el radio (~2.4 ms hasta radio 8) y 530 draw
calls es trivial para una GPU moderna. El audit pide medir antes de optimizar;
no hay un cuello medido que justifique batching (que ademas subiria el coste de
re-meshing por edicion) ni LOD (la vista cabe entera). Se deja documentado como
posible mejora futura si el objetivo pasa a ser mundos mucho mas grandes.

**Alternativas descartadas.**
- Radio fijo mayor: no permite ajustar al hardware ni medir; el env var lo deja en
  manos del usuario.
- Fog end constante con radio variable: dejaria un borde visible (terreno mas alla
  de la niebla) o niebla antes del borde; atarlo al radio lo mantiene limpio.

**Consecuencia.** `render/renderer.rs` (`view_radius_from_env`, `fog_start/end`),
`docs/performance.md` (tabla de escalado). 218 tests; clippy `-D warnings` limpio.
Verificado en runtime a radios 4/6/8. **Limite**: la memoria crece ~lineal con las
columnas (~265 KB/columna); a radios grandes el `warm_streaming` inicial es mas
lento. Siguiente: interpolacion de render y overlay de texto.

---

### 2026-10-05 (v0.16.1) — Persistencia end-to-end (tests de integracion)

**Decision.** Dos tests de integracion de guardado: (1) **roundtrip completo** de
un mundo editado (bloques a y=5 y y=200 mas una fuente de agua) via
`WorldSave::save_to` -> `load_and_migrate` -> `World::new(seed, r, restaurado)` ->
`warm_streaming`; (2) **recuperacion tras crash**: se corrompe el archivo
principal y se comprueba que el `.bak` (guardado atomico anterior) sigue cargando.

**Motivo.** El audit (§41) pide tests de "crash simulation durante save" y de
guardado/carga integrados. Hasta ahora habia tests de registro (una columna) pero
ninguno ejercitaba el flujo **mundo completo**: volcar -> guardar -> cargar ->
reconstruir, ni la recuperacion del `.bak`.

**Alternativas descartadas.** Test puramente de unidad de `ChunkRecord`: ya
existe y no cubre la integracion (volcado, restore, agua). No hacia falta mas.

**Consecuencia.** `world/save.rs` (2 tests). 220 tests; clippy `-D warnings`
limpio. Sin cambio de runtime. Con esto la persistencia queda cubierta a nivel
registro, migracion (v1/v3/v4/v5) y mundo completo, incluida la recuperacion.

---

### 2026-10-05 (v0.17.0) — Worldgen por etapas: celular + continentes + costas (FASE 1/2)

**Contexto.** El generador antiguo derivaba el relieve del **bioma** (un `match`
de amplitudes) sobre un `Fbm` continental que no separaba tierra/océano: las
costas se reducian a `height <= SEA_LEVEL + 1` y los biomas eran umbrales de
clima, sin regiones geometricas. La auditoria de worldgen pide jerarquia
espacial: celdas -> continentes -> clima -> landforms -> hidrologia -> ...

**Decision.** Nuevo modulo `world/worldgen/` con arquitectura por etapas:
`WorldGenConfig` (central + validacion), seeds derivadas por campo,
helpers de math (`smoothstep`/`remap`/`spline`), muestreador **celular Worley**
(`CellSample` con id estable), **continentalness** con domain warping,
clasificacion `LandClass`, **costa de ancho variable** (roll por celda) y una
altura base = spline(continentalness) + relieve macro + cordilleras (mascara de
rango + cresta) + valles. `TerrainGenerator` produce un `TerrainSample` y lo
convierte a bloques; el relieve **deja de depender del bioma**. Se anade un
preview offline (`examples/worldgen_preview.rs`) que exporta PNG y metricas.

**Alternativas descartadas.**
- Reescribir `terrain.rs` de golpe: el audit pide migracion por fases conservando
  equivalencia; se extrajo la geografia a un modulo y `terrain.rs` la consume.
- Campo continental como un solo `Fbm`: se combino macro + detalle (0.78/0.22)
  y domain warping para romper la regularidad.
- Clasificar tierra/océano punto a punto con un segundo ruido: la auditoria pide
  clasificar por la **celda** (identidad geometrica); el id de celda es estable y
  da coherencia regional (costas, futuras features).
- Empaquetar la altura y el bioma en la misma fase (como antes): mantenerlos
  separados permite regionalizar el bioma despues (FASE 3) sin tocar el relieve.

**Por que.** Da continentes, oceanos con plataforma y mar abisal, costas de
ancho variable y cordilleras con identidad, sin `noise + noise + noise`. Todo
determinista: `(seed, x, z)` -> mismo resultado; `WorldGen` es `Send + Sync`.

**Tradeoffs.** El bioma sigue siendo por clima (FASE 3 `DEFERRED`), asi que las
regiones de bioma aun no coinciden con las celdas (las celdas ya se usan para el
ancho de costa y quedan listas para el bioma y las features). La hidrologia y la
jerarquia de cuevas no se tocaron (FASE 5/6 `DEFERRED`). `GENERATOR_VERSION`
sube a 9 (el relieve cambia por completo); los mundos guardados se regeneran con
el nuevo generador y solo se reaplican las ediciones del jugador (comportamiento
ya existente).

**Consecuencia.** `world/worldgen/{mod,config,math,cells}.rs`, `world/terrain.rs`
(usa `WorldGen`; `Biome::relief` eliminado), `examples/worldgen_preview.rs`
(FASE 9 PARCIAL), `save.rs` (`GENERATOR_VERSION = 9`). Medido con el preview:
oceano 31-55% segun seed, abisal presente, picos hasta el techo de mundo.
236 tests; clippy `-D warnings` limpio. Verificado en runtime.

---

### 2026-10-05 (v0.17.1) — Fix: overrun del buffer GPU de malla

**Contexto.** Al caminar hacia tierra, el juego se cerraba con
`wgpu Validation Error: In Queue::write_buffer ... would end up overrunning the
bounds of the Destination buffer of size 84352` (copia de 84992 bytes). El nuevo
worldgen produce mallas de seccion de tamano mas variable y destapo el bug.

**Causa.** `Mesh::new` (v0.8.18) creaba los buffers con `create_buffer_init`
(tamano = datos exactos) pero guardaba `vertex_capacity = bytes.next_power_of_two()`
(mayor). `Mesh::update` decidia recrear solo si `bytes > capacity`; entre el
tamano exacto y la potencia de dos, creia que cabia y `write_buffer` escribia mas
alla del buffer real.

**Decision.** Crear los buffers con la capacidad **reservada** (`buffer_capacity`,
potencia de dos) y subir los datos con `write_buffer`; asi `capacity` refleja el
tamano real. Helper `buffer_capacity(bytes)` con test de la invariante
`capacidad >= bytes` y potencia de dos.

**Alternativas descartadas.** `create_buffer_init` con `contents` rellenado a la
capacidad: sube CPU por una copia extra sin necesidad. No reservar holgura
(crear siempre del tamano exacto): vuelve al churn de buffers por edicion.

**Consecuencia.** `render/mesh.rs` (`buffer_capacity`, `new` con `queue`,
`update`), `render/renderer.rs` (llamadas con `queue`). 237 tests; clippy limpio.
Verificado reproduciendo la caminata real (varios cruces de chunk) sin crash.
**Leccion**: es un bug latente de v0.8.18 que un cambio de datos (worldgen) hizo
visible; el test de la invariante evita que reaparezca.

---

### 2026-10-05 (v0.18.0) — Worldgen FASE 3: bioma por region celular

**Contexto.** En v0.17.0 el relieve dejo de depender del bioma, pero el bioma
seguia saliendo de una cascada de `if` sobre umbrales de clima: regions amorfas,
sin identidad geométrica ni relacion con las celdas que ya usabamos para las
costas.

**Decision.** Nuevo `worldgen/biomes.rs`:
- `BiomeDefinition` como **datos** (rangos de temperatura/humedad/altura +
  `tree_density`); `select(t, h, elevation)` puntua cada bioma con bandas suaves y
  elige el mejor (desempate estable por orden). Anadir un bioma = anadir una fila.
- **Regionalizacion**: `WorldGen` calcula el clima local y el del **centro de la
  celda**, y los mezcla con `blend = smoothstep(0.15, 0.55, cell_edge)`; en el
  interior domina el centro (bioma unico por region), cerca del borde domina lo
  local (transicion suave).
- **Lapse de altitud**: `T -= lapse_rate * max(0, elev - lapse_start)`, con
  `elev = smoothstep(sea, altitude_top, altura)`; asi las cumbres son frias.
- El clima (`temperature`/`humidity`) se mueve a `WorldGen`: una sola muestra por
  `(x,z)` da geografia + clima + bioma. `TerrainGenerator::climate`/`biome_at`
  delegan; `surface_block` y la decoracion siguen igual.
- `Biome::tree_density` lee de la definicion (una sola fuente).

**Alternativas descartadas.**
- Bioma puramente por celda sin mezcla: transiciones duras (patron "bioma A
  pegado a B"); la mezcla por `cell_edge` da bordes suaves.
- Bioma puramente local (como antes): regiones amorfas; no aprovecha las celdas.
- Biomas como `enum` con `match` de propiedades: volveria a la cascada; los
  datos permiten expandir sin tocar el selector.
- Mover `Biome` a `biomes.rs`: churn de imports; se deja el `enum` en `terrain`
  (la logica de seleccion si vive en `biomes`).

**Tradeoffs.** `GENERATOR_VERSION → 10`: cambian materiales/vegetacion (el
relieve es el mismo que v9). Los biomas `Tundra`/`Taiga` son raros en climas
templados (correcto por scoring, pero el porcentaje depende mucho de la seed).

**Consecuencia.** `worldgen/biomes.rs`, `worldgen/{mod,config}.rs`,
`world/terrain.rs`, `examples/worldgen_preview.rs` (mapa logico de biomas +
reparto; `layer` biome/height/continental), `save.rs` (v10). 241 tests; clippy
limpio. Preview: 7 biomas con regiones coherentes; verificado en juego caminando
varios chunks sin crash. Siguiente: FASE 5 hidrologia/rios.

### 2026-10-05 (v0.19.0) — Worldgen FASE 5: hidrologia (rios y lagos)

**Contexto.** Tras continentes/biomas, faltaba la capa que convierte relieve en
agua. La auditoria pide rios estructurales (cauces, anchos variables) y no
`noise > umbral => river`.

**Decision.** Hidrologia analitica y determinista dentro de `WorldGen::sample`:
- La **linea del rio** es una **cresta** (`ridge = 1 - |river_noise|`) deformada
  por su propio **domain warping** → trazados sinuosos y alargados.
- **Caudal** `flow = 0.35·humedad + 0.65·ruido_ancho`; de ahi el ancho y la
  profundidad (`lerp(min,max,flow)`).
- **Cauce**: `river_proximity = smoothstep(1-width, 1, ridge) · landness`;
  `cut = proximity^power · depth`; el terreno baja `cut` y el material pasa a
  arena/grava donde el cauce es claro.
- **Nivel de agua** `h - depth·0.30` (contenido bajo el borde); cerca del mar,
  `max(agua, sea)`; cualquier columna bajo el mar se inunda a mar.
- **Lagos**: `valle · humedad · cuenca` sobre un umbral → depresion rellena.
- `TerrainSample` gana `river_proximity` y `surface_water`; `generate_column`
  rellena agua hasta `surface_water` (mar/rio/lago).

**Alternativas descartadas.**
- Rejilla hidrologica + routing de caudal (niveles A-D del audit): objetivo
  "completo" (afluentes/orden), requiere simulacion por region y cache; se
  pospone. La version analitica da rios creibles y baratos.
- `Perlin` simple sin cresta: lineas redondeadas sin cauce.
- Meter el agua de mundo al automata de fluidos: el audit pide agua de worldgen
  **estable**; aqui nace fuera del active set (solo las ediciones lo alimentan),
  asi que un rio quieto cuesta 0 CPU.

**Tradeoffs.** Sin afluentes/orden de rio ni lagos oxbow (PARTIAL). En pendientes
el nivel de agua puede variar por columna, pero queda contenido por las paredes
solidas y no se simula hasta que el jugador lo toca. `GENERATOR_VERSION -> 11`.

**Consecuencia.** `worldgen/{mod,config}.rs`, `world/terrain.rs` (relleno por
`surface_water`, lecho de arena), `engine/demo.rs` + `app.rs` (`SOLARIA_RIVER`),
`examples/worldgen_preview.rs` (capa `river`), `save.rs` (v11). 244 tests; clippy
limpio. Preview: rios serpenteantes que llegan al mar. Verificado en juego sin
crash. Siguiente: FASE 6 (cuevas jerarquicas).

## v0.19.1 — Limpieza y orden del repositorio

### 2026-10-07 — Higiene antes de seguir con el worldgen

**Decision.** Antes de continuar el worldgen (FASE 6+) hacemos una pasada de
orden, sin tocar el mundo ni el formato:
- Se elimina codigo **demostrablemente** muerto (`raycast::_unused`,
  `terrain::max_height`; `MAX_HEIGHT` si se usa, se queda).
- Los backups de arte (`*_pre28`, `*_med`, los mapas de atlas en texto y el
  `.bbmodel` previo) se mueven a `assets/backup/` (reversible) y cada asset se
  documenta en `assets/README.md`.
- `Cargo.toml` gana metadatos (repository/authors/keywords/categories/readme);
  se anade licencia dual (`LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`),
  `.gitattributes` (`* text=auto eol=lf`) y `docs/worldgen.md`.
- `README.md` y `screenshots/README.md` al dia (controles, variables, estructura,
  indice de capturas).

**Motivo.** Dejar el repo autoexplicativo y ligero antes de anadir mas worldgen.
Tambien se libera disco de desarrollo (`target/debug/incremental`, ~3.5 GB).

**Alternativas descartadas.** Reorganizar `screenshots/` en carpetas por version:
mucho churn y rompe los enlaces a las imagenes; en su lugar, un indice
(`screenshots/README.md`). **Borrar** los backups de arte: es arte del usuario y
se prefiere moverlo (reversible). Bump artificial de versiones: no cambia el
mundo, asi que `GENERATOR_VERSION`/`FORMAT_VERSION` quedan intactos (11 / 5).

**Consecuencia.** Solo cambios de repo/docs; mismos 244 tests y clippy limpio.
Disco liberado: ~3.5 GB. `Cargo.toml` y el titulo pasan a `v0.19.1`.

## v0.20.0 — Worldgen FASE 6: cuevas jerarquicas

### 2026-10-07 — Cuevas por capas (no un unico campo de densidad)

**Decision.** Sustituir el campo unico (`tunnels*0.7 + chambers*0.3`) por varios
sistemas que se suman, cada uno con su escala y su activacion barata:
- **Spaghetti**: dos campos `Fbm` de tubos que se cruzan -> red de galerias.
- **Tubos regionales**: mas anchos y de baja frecuencia.
- **Camaras `cheese`**: blobs grandes (frecuencia muy baja), profundos y en
  regiones aptas.
- **Pozos verticales** (campo que varia lento en Y) y **canones** anisotropos
  (largos en X, en banda media).
- **Pilares/puentes**: mascara de preservacion dentro de las camaras.
- **Entradas**: grietas/sinkholes **raras** que rompen la corteza.
- Densidad **atenuada por profundidad** (`surface - y`) y **reforzada bajo
  montanas** (`mountain_mask`, campo nuevo en `TerrainSample`).
- El trabajo 2D (region de camaras, pozos, canones, entradas) se calcula **una
  vez por columna** en `CaveContext`; el bucle por voxel solo paga los 3D activos.

**Motivo.** El audit pide cuevas **jerarquicas** (FASE 6): variedad de formas y
control (entradas, pilares) en vez de una nube homogenea de huecos.

**Alternativas descartadas.** Un solo campo con mas octavas (poca variedad de
formas); recalcular el contexto 2D por voxel (coste inutil); evaluar todos los
campos 3D en cada celda (se evita con el contexto 2D y el early-out).

**Tradeoffs.** `terrain_generate_column` pasa de 3.74 a ~6.3 ms/columna (~1.7x).
Se amortigua sacando `pillar` del camino comun (solo se evalua si un sistema ya
propone cavar) y evaluando `tubes_b` solo cerca de la banda cero de `tubes_a`.
Sin afluentes de cueva ni biomas subterraneos (fuera de alcance).

**Consecuencia.** `world/caves.rs` reescrito; `world/worldgen/mod.rs`
(`mountain_mask`); `world/terrain.rs` (contexto por columna); `world/save.rs`
(`GENERATOR_VERSION -> 12`). 246 tests (determinismo, corteza salvo entradas,
refuerzo por montana, pozos verticales); clippy limpio. Fraccion de aire
subterraneo medida ~4 %. Siguiente: FASE 7 (decoracion por reglas).

```
### [fecha] vX.Y.Z — Titulo
**Decision.** ...
**Motivo.** ...
**Alternativas descartadas.** ...
**Consecuencia.** ...
```
