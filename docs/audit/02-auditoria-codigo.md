# Fase 2 de 3 — Auditoría de código de SolariaVoxel

**Alcance de esta sesión (3 módulos de la Fase 1):**

1. **Persistencia y biblioteca de mundos:** `src/world/save.rs`, `src/world/library.rs`, `src/engine/save_worker.rs`, y los flujos de carga/guardado en `src/engine/app.rs`.
2. **Streaming y simulación de agua:** `src/world/store.rs` (agua, streaming, guardado de columnas) y `src/world/water.rs`.
3. **Malla de agua:** `src/world/fluid_mesher.rs`.

Quedan fuera de esta sesión, para auditorías posteriores: resto de `engine/app.rs`, `render/`, `ui/`, `player/`, `physics.rs`, `worldgen/` (Larion/Graph) y `examples/`.

## Método y limitaciones

- El árbol de trabajo sigue con los 278 archivos versionados borrados (Fase 1). El código se leyó desde los objetos Git de `HEAD` (`4d1c32d`, `v0.47.0`) con `git show HEAD:<ruta>`. **No se modificó ningún archivo del proyecto.**
- **No se compiló ni se ejecutaron tests** (no hay `Cargo.toml` en el árbol de trabajo). Todos los hallazgos son por lectura de código; los marcados como **a verificar** requieren reproducción en ejecución.
- Antes de reportar, comprobé algunas sospechas que resultaron **falsas** y las descarto para no contaminar el informe: (a) el comentario de `water.rs` dice "nibble de flujo", y `chunk.rs` efectivamente almacena nibbles, así que es correcto; (b) `renderer.rs` sí re-encola las secciones verticales vecinas al cambiar agua (`queue_fluid_dirty` + `vertical_neighbor_sections`), así que no hay costura vertical sin re-mallar.

## Tabla resumen por severidad

| Sev. | Id | Módulo | Título | Estado |
|---|---|---|---|---|
| P0 | P0-1 | `app.rs` + `save.rs` | Un `world.vf` ilegible o de formato futuro se abre vacío y se **sobrescribe**; el `.bak` bueno se pierde | Confirmado por código |
| P1 | P1-1 | `app.rs` | Cerrar o cambiar de mundo durante un autoguardado en vuelo **descarta** las ediciones posteriores | Confirmado por código |
| P1 | P1-2 | `library.rs` + `app.rs` | El `world.vf` legado **resucita** un mundo importado tras borrar todos | Confirmado por código |
| P1 | P1-3 | `library.rs` | La base de guardado es el **directorio de trabajo**, no "junto al ejecutable" como documenta el módulo | Confirmado por código |
| P1 | P1-4 | `save.rs` | Sin **integridad** del payload (ni checksum); corrupción que no cambia el tamaño pasa inadvertida | A verificar |
| P1 | P1-5 | `save.rs` + `app.rs` | No hay **recuperación automática** desde `world.vf.bak` al cargar | Confirmado por código |
| P1 | P1-6 | `water.rs` + `store.rs` | Vecinos **no cargados** se tratan como seco: el agua de borde puede secarse | A verificar |
| P1 | P1-7 | `library.rs` + `app.rs` | Metadatos ilegibles → generador `legacy16` y semilla 0/13371 **silenciosos** | A verificar |
| P1 | P1-8 | `store.rs` | Cada cruce de chunk escanea columnas completas (98 304 celdas) de todo el radio de simulación | A verificar (perf) |
| P2 | P2-1 | `store.rs` | El flujo de agua no marca `dirty`: el registro persistente de columnas no editadas queda obsoleto | A verificar |
| P2 | P2-2 | `save.rs` | `from_column` consulta `has_flow_storage()` y `get()` por celda | Confirmado (perf) |
| P2 | P2-3 | `save.rs` | Un mismo chunk se descomprime 2-3 veces al cargar (`is_corrupt`, `first_unknown_id`, `decompressed_*`) | Confirmado (perf) |
| P2 | P2-4 | `save.rs` | Tamaño de descompresión tomado del prefijo del archivo: asignación no acotada | A verificar |
| P2 | P2-5 | `save.rs` | `unwrap_or(0)` convierte un error de cabecera en `NoMigration{from:0}` (mensaje engañoso) | Confirmado |
| P2 | P2-6 | `save.rs` | Bytes sobrantes al final del archivo no se detectan | Confirmado |
| P2 | P2-7 | `save.rs` | Rotación de archivos sin `fsync` del directorio; ventana sin `world.vf` tras un fallo | Confirmado |
| P2 | P2-8 | `save.rs` | Cadena de migración clona cada `ChunkRecord` (hasta ~98 KB comprimidos) por paso | Confirmado (perf) |
| P2 | P2-9 | `save.rs` | `HashMap` serializado en orden no determinista: el binario no es reproducible | Confirmado |
| P2 | P2-10 | `library.rs` | `level.json` se escribe sin `sync_all` antes del rename | Confirmado |
| P2 | P2-11 | `library.rs` | `list_worlds` traga errores de `read_dir`; `create_world` puede reutilizar un slug existente | A verificar |
| P2 | P2-12 | `library.rs` | `duplicate_world` no es atómico, deja carpetas parciales y copia el binario sin coordinar con el guardado | Confirmado |
| P2 | P2-13 | `library.rs` | Slugs sin validar en `delete/rename/duplicate/touch`; `rename_world` acepta nombre vacío | Confirmado |
| P2 | P2-14 | `library.rs` | `import_legacy` carga el mundo entero solo para leer la semilla, con `unwrap_or(0)` silencioso | Confirmado |
| P2 | P2-15 | `app.rs` | Borrar el mundo activo deja el autoguardado reintentando en bucle | A verificar |
| P2 | P2-16 | `water.rs` + `store.rs` | Coste innecesario en el tick de agua: `HashSet` con SipHash, `Instant::now()` por celda, búsquedas repetidas | Confirmado (perf) |
| P2 | P2-17 | `store.rs` | Celdas fuera del radio de simulación se **descartan** de la cola en vez de aplazarse | A verificar |
| P2 | P2-18 | `fluid_mesher.rs` | Malla sin `with_capacity`, closures `dyn` por celda y esquinas calculadas aunque la cara superior esté oculta | Confirmado (perf) |
| P2 | P2-19 | `fluid_mesher.rs` | Caras laterales de altura parcial usan v=0..1: la textura se comprime verticalmente | A verificar (visual) |
| P2 | P2-20 | `store.rs`, `water.rs`, `save.rs`, `chunk.rs`, +35 archivos | **Mojibake** en comentarios (`┬┐`, `Ô├®`, `Ã`): ~295 ocurrencias en 40 archivos | Confirmado |
| P2 | P2-21 | `app.rs` | Dos caminos de carga (arranque y `load_world`) con lógica y fallbacks **divergentes** | Confirmado |

**Conteo:** P0: 1 · P1: 8 · P2: 21.

---

## Detalle por archivo

### `src/engine/app.rs`

**P0-1 — Mundo ilegible o de formato futuro se abre vacío y se sobrescribe (crítico)**
- `app.rs:1380-1415` (`load_world`) y `app.rs:2187-2218` (arranque): cualquier `Err` de `load_and_migrate` se trata como "mundo recién creado". Solo se consulta `level.json` y se arranca con `Vec::new()` como chunks restaurados.
- `save.rs:473-488` y `save.rs:803-817`: `TooNew`, `Decode`, `CorruptChunk` y `UnknownBlock` son errores **distintos** de "no existe", pero el llamador no los distingue.
- Consecuencia: el primer guardado (`app.rs:597-617`, `save.rs:451-466`) escribe un mundo casi vacío. `save_to` **rota** el `world.vf` corrupto a `world.vf.bak` y **borra el `.bak` anterior**, que era la última copia buena.
- **Cómo reproducir:** 1) crear un mundo, editar bloques, guardar dos veces (existen `world.vf` y `world.vf.bak` válidos); 2) truncar `saves/<slug>/world.vf` a 3 bytes; 3) arrancar y entrar al mundo: en consola aparece `sin world.vf (...); se inicia con semilla N`; 4) provocar un guardado (autoguardado o salir). Resultado esperado con el código actual: `world.vf.bak` pasa a ser el archivo corrupto y los datos buenos se pierden. Igual para un archivo de versión futura (`TooNew`) abierto con un binario más antiguo, que además se degradaría.
- **Recomendación para la fase de corrección (no aplicar aún):** distinguir `NotFound` de otros errores; si el archivo existe pero no carga, no entrar en modo "vacío" y no sobrescribir; y añadir recuperación desde `.bak` (ver P1-5).

**P1-1 — Ediciones perdidas al cerrar o cambiar de mundo con autoguardado en vuelo (importante)**
- `app.rs:590-593`: `save_world()` retorna sin encolar si `save_requested` es `true`.
- `app.rs:647-657` (`finalize_save`) llama a `save_world()` (no encola nada si hay un guardado en vuelo), luego `join()` y `poll_save()`. Solo se persiste el snapshot **anterior**, y las ediciones posteriores se pierden.
- `app.rs:1374-1376` (`load_world`): mismo patrón; el renderer viejo se descarta justo después.
- **Cómo reproducir:** 1) editar un bloque y esperar a que empiece un autoguardado (`[world] guardado en segundo plano`); 2) editar otro bloque antes de que termine; 3) cerrar. El segundo bloque no se guarda. Probabilidad baja por la ventana temporal, pero es pérdida silenciosa.
- **Recomendación:** encolar siempre un snapshot final en `finalize_save`/`load_world`, o coalescer (marcar "pendiente" y reenviar al terminar el anterior).

**P1-5 — Sin recuperación automática desde `.bak` (importante)**
- `save.rs:803` (`load_and_migrate`) y `app.rs:1380`/`2187` no intentan `world.vf.bak` si el principal falla. Solo `library.rs` (`load_meta`, `library.rs:205-208`) tiene fallback. Los tests (`save.rs`, `un_archivo_principal_corrupto_no_pierde_el_bak`) prueban que el `.bak` es **legible**, no que el juego lo use.

**P2-21 — Dos caminos de carga divergentes (mejora)**
- `load_world` (`app.rs:1380-1415`) retorna si no hay `level.json`; el arranque (`app.rs:2206-2210`) usa semilla 13371 y continúa. Misma situación, dos resultados distintos.
- Ambos duplican conversión de la posición del jugador, selección de generador y creación del renderer.

**P2-15 — Borrar el mundo activo (a verificar)**
- `app.rs:1588-1596`: `delete_world` no comprueba si el slug es `self.world_slug` con renderer activo. El siguiente autoguardado llama a `save_to` (`save.rs:451`), cuya `File::create` falla porque la carpeta ya no existe; el error se reintenta indefinidamente (`app.rs:640`).

### `src/engine/save_worker.rs`

- Diseño sólido: un hilo, canal de trabajos y `join` al cerrar (`save_worker.rs:95-99`). Tests cubren éxito y error de ruta (`save_worker.rs:110-153`).
- El problema real está en el llamador (P1-1), no en el worker: `request()` devuelve `false` solo si el canal está cerrado, y el llamador lo registra como `no se pudo encolar`.

### `src/world/save.rs`

**P1-4 — Sin integridad del payload (importante, a verificar)**
- `ChunkRecord` (`save.rs:256-300`) y `WorldSave` no guardan checksum. `is_corrupt` (`save.rs:279`) solo valida rango vertical y **tamaño** tras descomprimir. LZ4 sin checksum puede devolver un tamaño correcto con datos alterados; el mundo se cargaría sin error.
- **Verificar:** alterar un byte dentro de un payload LZ4 y comprobar si `decompress_size_prepended` lo detecta. Si no, es un fallo silencioso de datos.

**P1-5 — Ver `app.rs`.**

**P2-2 — `from_column` por celda (rendimiento)**
- `save.rs:218-232`: el bucle de 98 304 celdas llama `column.has_flow_storage()` y `column.get()` dentro del bucle interno. Se puede hoistar y reservar el buffer de `fluid` una sola vez.

**P2-3 — Descompresiones repetidas (rendimiento)**
- `is_corrupt` (`save.rs:279-300`) llama `decompressed_blocks()` y, si hay fluido, `decompressed_fluid()`. Luego `first_unknown_id()` (`save.rs:305`) vuelve a descomprimir los bloques, y `load_and_migrate` (`save.rs:810-815`) repite el ciclo por chunk. Son 2-3 decodificaciones LZ4 por columna en la carga. Conviene validar una vez y reutilizar el buffer.

**P2-4 — Asignación no acotada desde el prefijo (a verificar, seguridad/robustez)**
- `decompressed_blocks` (`save.rs:256-261`) usa `decompress_size_prepended`, que toma la longitud del prefijo del archivo. Un archivo malicioso o dañado puede pedir una asignación enorme antes de validar `raw_len()`. Verificar el comportamiento de `lz4_flex 0.14` y acotar el tamaño antes de descomprimir.

**P2-5 — Error de cabecera enmascarado (mejora)**
- `load_from` (`save.rs:476-478`): `decode_from_slice::<WorldHeader>(...).unwrap_or(0)`. Un archivo con cabecera corrupta reporta `NoMigration { from: 0 }`, no "cabecera corrupta". Dificulta el diagnóstico de P0-1.

**P2-6 — Bytes sobrantes no detectados (mejora)**
- `load_from` ignora los bytes restantes tras decodificar (`decode_from_slice` devuelve el consumido). Un archivo truncado o con basura al final se acepta sin aviso si la parte útil decodifica.

**P2-7 — Rotación sin fsync del directorio (mejora)**
- `save_to` (`save.rs:451-466`): `sync_all` sobre el temporal, pero no sobre la carpeta tras los `rename`. Tras un corte de energía el rename puede no ser durable. Además hay una ventana entre `rename(path, bak)` y `rename(tmp, path)` sin `world.vf`; el `.bak` existe, pero nada lo usa automáticamente (P1-5).

**P2-8 — Migración con clones (rendimiento, mundos antiguos)**
- `MigrationChain::migrate` (`save.rs:770-797`): `*record = migrator.migrate_chunk(record)` clona cada registro en cada paso. Con v1→v6 son hasta 5 copias de ~98 KB por chunk.

**P2-9 — Orden no determinista (mejora)**
- `WorldSave.chunks` es `HashMap` (`save.rs`, struct `WorldSave`). Dos guardados del mismo estado producen bytes distintos. Complica diffs, pruebas de regresión por hash y depuración.

**Otros hallazgos menores de `save.rs`**
- Tests bien enfocados (migraciones v1→v6, corrupción, atomicidad). Faltan: carga de un archivo corrupto a través de `app.rs` (P0-1), y recuperación desde `.bak` (P1-5).

### `src/world/library.rs`

**P1-3 — Base de guardado mal documentada (importante)**
- `library.rs:81-85` (`base_dir_from_env`): sin `SOLARIA_HOME`, la base es `PathBuf::from(".")`, es decir, el **directorio de trabajo**. La cabecera del módulo dice "Base por defecto: junto al ejecutable".
- Consecuencia: al lanzar desde un acceso directo o desde otra carpeta, la biblioteca parece vacía y se crea un mundo nuevo. Los mundos existentes quedan en otra ruta.
- Recomendación: resolver `std::env::current_exe()?.parent()` o corregir la documentación de forma explícita.

**P1-2 — El legado resucita tras borrar todos los mundos (importante)**
- `library.rs:331-333` (`import_legacy`): importa `base/world.vf` si existe y la lista está vacía. Se **copia** (no se mueve) y nunca se marca como importado.
- `app.rs:2157`: se ejecuta en cada arranque. Si el usuario borra todos los mundos, el siguiente arranque vuelve a importar el mismo binario como "Mundo importado".
- **Cómo reproducir:** 1) dejar un `world.vf` junto al ejecutable (base), sin `saves/`; 2) arrancar y borrar el único mundo importado; 3) reiniciar: reaparece "Mundo importado". Los tests (`importa_el_world_vf_antiguo_una_sola_vez`, `library.rs:~430`) solo cubren la segunda llamada con mundos existentes.
- Recomendación: mover el legado en vez de copiarlo, o registrar en `level.json` un marcador de importación.

**P1-7 — Metadatos ilegibles con valores por defecto silenciosos (a verificar)**
- `list_worlds` (`library.rs:214-245`): si `level.json` y su `.bak` fallan, la entrada se marca `corrupt` con `generator_kind = "legacy16"` y `seed = 0`.
- Al entrar: `app.rs:1427-1429` y `app.rs:2231-2235` usan `unwrap_or_default()` para el generador. Si el `world.vf` existe pero `level.json` no, el terreno no guardado se generaría con el generador equivocado y la semilla de la cabecera (correcta) conviviría con un generador distinto.
- **Verificar:** crear un mundo `larion`, corromper `level.json` y `level.json.bak`, y comparar el terreno de una zona no editada.

**P2-10 — `level.json` sin `sync_all` (mejora)**
- `save_meta` (`library.rs:185-200`): `std::fs::write(&tmp, json)` sin `sync_all`. Con corte de energía puede quedar un archivo vacío; `load_meta` cae al `.bak`, pero el `.bak` puede ser también antiguo. Usar el mismo patrón que `save.rs`.

**P2-11 — Errores de listado silenciados (a verificar)**
- `list_worlds` (`library.rs:214-225`): `read_dir` falla → `return out` vacío; `entries.flatten()` descarta entradas erróneas. Entonces `create_world_kind` (`library.rs:~255`) ve `taken = []` y, si el slug base ya existe en disco, `save_meta` sobrescribe el `level.json` del mundo existente (cambiando su semilla y generador, sin tocar su `world.vf`).

**P2-12 — `duplicate_world` no atómico y sin coordinación (mejora)**
- `library.rs:292-310`: crea la carpeta destino antes de copiar; si `copy` falla, queda una carpeta sin `level.json` que aparecerá como "corrupta". Además copia `world.vf` del disco, no el estado en memoria, sin comprobar que el mundo no esté abierto.

**P2-13 — Validación de slugs y nombres (mejora)**
- `rename_world`, `duplicate_world`, `delete_world` y `touch_last_played` (`library.rs:283-320`, `~330`) unen `slug` sin validar. Hoy los slugs salen de `list_worlds` (nombres de directorio), así que el riesgo real es bajo, pero `delete_world(base, "..")` borraría el directorio base. Defensa en profundidad.
- `rename_world` no valida nombre vacío ni longitud.

**P2-14 — Importación con carga completa (mejora)**
- `import_legacy` (`library.rs:331-345`): llama `load_and_migrate` sobre todo el archivo solo para leer la semilla de la cabecera, que se puede leer con `WorldSave::load_from`/peek. Si falla, `unwrap_or(0)` deja semilla `0` sin aviso.

**Positivo:** `seed_from_text` es determinista y está fijado por test (`library.rs:~470`). `slugify` evita nombres reservados y caracteres no ASCII (ver `slugify`, ahí la salida es siempre ASCII).

### `src/world/store.rs` (agua, streaming y persistencia de columnas)

**P1-6 — Vecinos no cargados tratados como seco (importante, a verificar)**
- `FluidGrid::in_bounds` (`store.rs:1254-1260`) significa "columna cargada". `get_new_level` (`water.rs:172-200`) y `spread` (`water.rs:204-242`) ignoran cualquier vecino no cargado: no cuenta como fuente, no recibe agua.
- Consecuencia: una celda de flujo en el borde de la zona cargada cuya fuente está en una columna **descargada** se recalcula a `Fluid::None` al procesarla (`water.rs:253-258`) y el agua desaparece. Se procesa cuando un cambio vecino la re-encola (`enqueue_water`, `store.rs:1108-1116`).
- No hay tests de frontera: `TestGrid::in_bounds` (`water.rs:362-365`) es siempre "cargado" dentro de su rango.
- **Verificar:** en un río o lago que cruce un borde de radio de carga, ajustar un bloque justo al otro lado del borde mientras la fuente está descargada, y observar si el agua se seca.

**P1-8 — Escaneo de columnas completas en cada cruce de chunk (importante, a verificar)**
- `plan_center` (`store.rs:822-855`) llama `wake_sim_water(center)` en cada cambio de chunk del jugador.
- `wake_sim_water` (`store.rs:859-870`) llama `wake_column_water` para **todas** las columnas del radio de simulación.
- `wake_column_water` (`store.rs:437-458`): si la columna tiene almacén de flujo (`has_flow_storage`), recorre `WORLD_HEIGHT × 16 × 16` celdas (98 304) con `flow_at`. Con N columnas con flujo en el radio, son N × 98 304 comprobaciones por cruce.
- Riesgo: hitches al cruzar un chunk en zonas con ríos o lagos. Coherente con la deuda documentada (~10,7 ms por cruce de iluminación). **Verificar con perfil** (`robloxstudio` no aplica; usar el overlay de memoria/tiempo del propio motor o un contador local).
- Recomendación: mantener un índice de celdas con flujo por columna o por sección, en vez de escanear la columna completa.

**P2-1 — Flujo no marcado como `dirty` (a verificar)**
- `set_water_raw` (`store.rs:1121-1156`) escribe bloque y nibble de flujo sin `self.dirty.insert(pos)`. Su comentario lo justifica ("sin marcarla como editada"), pero el resultado es que:
  - el flujo de una columna no editada no se persiste, y
  - una columna editada solo refleja el estado del **último** `sync_modified` (`store.rs:1000-1012`), no el actual.
- Al recargar, el agua se re-despierta (`wake_column_water`) y vuelve a fluir, por lo que el daño suele ser transitorio. Sí puede alterar la forma de un charco o una cascada tras reabrir (p. ej., fuentes creadas por la regla 2x2 que no quedan registradas).
- **Verificar:** colocar una fuente en terreno abierto cerca de un borde de chunk, dejar que fluya, guardar, salir y comparar la forma.

**P2-16 — Coste innecesario en el tick de agua (rendimiento)**
- `tick_water_with` (`store.rs:1178-1217`): `Instant::now()` por celda (`store.rs:1183`).
- `water_in_equilibrium` (`store.rs:1206`) y `get_new_level` (`water.rs:172`) repiten búsquedas en `HashMap<ChunkPos, Box<Column>>` con SipHash (`in_bounds`, `fluid`, `water_at` llaman `world_to_local` y `columns.get` por cada vecino).
- `add_fluid_dirty` (`store.rs:1223-1240`) hace búsqueda lineal sobre la lista de secciones sucias por cada celda cambiada.
- `DirtyQueue` (`water.rs:271-309`) usa `HashSet<[i32;3]>` con SipHash en el camino caliente.
- Medir antes de optimizar; el presupuesto de 6 ms/tick (`water.rs:39-57`) ya limita el daño.

**P2-17 — Celdas fuera del radio descartadas (a verificar)**
- `tick_water_with` (`store.rs:1200-1204`): si la celda está fuera del radio de simulación se **descarta** de la cola. Se re-despierta solo al cambiar de centro (`wake_sim_water`), y únicamente las columnas **cargadas** dentro del radio (`store.rs:859-870`). Una celda descartada que aún no tenía su columna cargada y se carga más tarde se despierta en `wake_column_water`, pero conviene confirmar que no se pierde trabajo en el borde del radio.

**Positivo:** `warm_streaming`, `plan_streaming` y `poll_generation` descartan resultados obsoletos mediante `id` por posición (`store.rs:944-972`); la cola de generación es coherente con el radio de carga.

### `src/world/water.rs`

- Modelo bien documentado y con buena cobertura de tests (fuente infinita, alcance 7, drenaje, cascada, 2x2). Los tests no cubren las fronteras de carga (P1-6).
- **P2-16** (rendimiento): ver `store.rs` arriba.
- **P2-20** (mojibake): `water.rs:81-82` y `water.rs:102`, `108`, `… ÔÇö`, `┬┐`.

### `src/world/fluid_mesher.rs`

**P2-18 — Rendimiento de la malla (mejora)**
- `fluid_section` (`fluid_mesher.rs:84-85`): `Vec::new()` sin capacidad para vértices e índices; cada sección de agua hace varias reasignaciones. Un `with_capacity` por cuota razonable evita reallocs.
- Las closures `&dyn Fn` (`fluid_mesher.rs:77-81`) generan despacho dinámico en el bucle más caliente: ~25 llamadas por celda de agua (4 esquinas × 4 + vecinos laterales + arriba).
- Las cuatro esquinas (`fluid_mesher.rs:104-108`) se calculan **antes** de comprobar si la cara superior está oculta (`113`). Se puede mover el cálculo dentro del `if`.

**P2-19 — Textura en caras laterales de altura parcial (a verificar, visual)**
- `fluid_mesher.rs:175`: `uv = [[0,0],[1,0],[1,1],[0,1]]` en todas las caras laterales, aunque el lado mida menos de 1 bloque (nivel 1..7). La textura se comprime verticalmente en vez de mostrarse a escala. Revisar en render.

**Positivo:** la regla de rampa por esquina máxima y la regla de caída sin rendija (`una_cascada_no_deja_rendijas_entre_bloques`, `fluid_mesher.rs:262-283`) están cubiertas por tests.

---

## Hallazgo transversal

**P2-20 - Mojibake en comentarios (consistencia).** Hay ~295 ocurrencias de caracteres dobles-codificados repartidas en 40 archivos de `src/`. Ejemplos: `store.rs:1052`, `water.rs:81-82`, `save.rs`, `chunk.rs`. `font.rs` (78) probablemente incluye tablas de glifos y requiere revisión aparte. Corregir la codificación de fuente (guardar todo como UTF-8 sin BOM) y revisar `.editorconfig`/`rustfmt` si los hay.

**Cobertura de tests (transversal):** la suite cubre bien los formatos y el agua en aislamiento, pero **no** cubre: (a) el camino `app.rs` con un `world.vf` corrupto (P0-1), (b) recuperación desde `.bak` (P1-5), (c) la importación repetida tras borrar (P1-2), ni (d) las fronteras de carga de la simulación de agua (P1-6).

---

## == Ancla Fase 3 ==

- **Auditados en esta sesión:** persistencia y biblioteca (`save.rs`, `library.rs`, `save_worker.rs`), streaming y agua (`store.rs`, `water.rs`), malla de agua (`fluid_mesher.rs`), y los flujos de guardado/carga de `app.rs`.
- **Hallazgos:** P0: 1 · P1: 8 · P2: 21. Ver tabla al inicio.
- **Prioridad de corrección (no aplicada):** P0-1 primero (abrir sin sobrescribir, distinguir `NotFound` de corrupción/versión), luego P1-5 (fallback `.bak`) y P1-1 (snapshot final al cerrar/cambiar), y después P1-2 y P1-3.
- **Pendiente de verificar en ejecución:** P1-4 (integridad LZ4), P1-6 (frontera de agua), P1-7 (generador con metadatos corruptos), P1-8 (coste de `wake_sim_water`), y P2-1, P2-4, P2-11, P2-15, P2-17, P2-19.
- **Sin compilar ni ejecutar:** no hay `Cargo.toml` en el árbol de trabajo. Antes de cualquier corrección hay que restaurar el checkout (decisión del usuario) para poder correr `cargo test` y reproducir los casos.
- **Siguiente sesión sugerida:** `src/render/` (renderer de 1.285 líneas, mallas y sincronización GPU) y el resto de `engine/app.rs` (pantallas, input, streaming de render). Y, para completar lo ya auditado, reproducir P0-1 y P1-6 en ejecución cuando el checkout esté restaurado.
