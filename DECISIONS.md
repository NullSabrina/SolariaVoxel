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







---

## Plantilla para futuras entradas

```
### [fecha] vX.Y.Z — Titulo
**Decision.** ...
**Motivo.** ...
**Alternativas descartadas.** ...
**Consecuencia.** ...
```
