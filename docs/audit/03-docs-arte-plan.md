# Fase 3 de 3 — Documentación, organización y plan de remediación

**Base:** resumen de Fase 1, auditoría de código de Fase 2 y lectura del snapshot local `HEAD` (`4d1c32d`, `v0.47.0`). No se reconstruyeron ni modificaron las 278 rutas versionadas borradas. La revisión documental de fuentes de código se limita a los blobs locales ya examinados y a los hallazgos de Fase 2; no equivale a revisar cada comentario del proyecto línea a línea.

## Anclas recibidas

### Resumen final de `01-inventario.md`

- SolariaVoxel es un motor voxel propio, sin motor de juego, escrito principalmente en Rust.
- Snapshot local `HEAD`: `4d1c32d`, tag `v0.47.0`; mismo commit que `origin/master` y remoto.
- Al inicio solo estaba `.git/`; esta fase añadió este informe. Los 278 archivos rastreados siguen borrados; no restaurarlos sin decisión explícita.
- Cargo, edición Rust 2024, mínimo Rust 1.87; ejecutable `src/main.rs` llama a `solaria_voxel::run()`.
- Plataforma/GPU: `winit` + `wgpu`; shaders WGSL y adaptador async `pollster`.
- Persistencia/serialización: `bincode 2`, `lz4_flex`, `serde`/`serde_json`; imagen PNG con `png`.
- Módulos prioritarios: `world`/`worldgen`, `engine/app.rs`, guardado/biblioteca y renderer.
- Tres generadores: Legacy16, Graph y Larion; versiones de mundo 21 y guardado 6.
- Hotspots documentados: streaming/iluminación, presupuesto de meshing, fluidos en fronteras y memoria GPU.
- Larion está medido a 1,44× legacy; falta validación visual GPU del mundo integrado.
- El objetivo documentado es menos de 500 MB; R12 ~477 MB y R16 ~871 MB CPU+GPU.
- README declara 429 tests y la última decisión 432; no se ejecutaron tests en esta fase.
- Hay candidatos de archivo en `assets/backup`; dos copias del modelo de antorcha tienen hash idéntico.
- `assets/src/ui/gui.aseprite` se menciona como fuente, pero no aparece en el árbol versionado.

### Resumen final de `02-auditoria-codigo.md`

- **Auditados en esta sesión:** persistencia y biblioteca (`save.rs`, `library.rs`, `save_worker.rs`), streaming y agua (`store.rs`, `water.rs`), malla de agua (`fluid_mesher.rs`), y los flujos de guardado/carga de `app.rs`.
- **Hallazgos:** P0: 1 · P1: 8 · P2: 21. Ver tabla al inicio.
- **Prioridad de corrección (no aplicada):** P0-1 primero (abrir sin sobrescribir, distinguir `NotFound` de corrupción/versión), luego P1-5 (fallback `.bak`) y P1-1 (snapshot final al cerrar/cambiar), y después P1-2 y P1-3.
- **Pendiente de verificar en ejecución:** P1-4 (integridad LZ4), P1-6 (frontera de agua), P1-7 (generador con metadatos corruptos), P1-8 (coste de `wake_sim_water`), y P2-1, P2-4, P2-11, P2-15, P2-17, P2-19.
- **Sin compilar ni ejecutar:** no hay `Cargo.toml` en el árbol de trabajo. Antes de cualquier corrección hay que restaurar el checkout (decisión del usuario) para poder correr `cargo test` y reproducir los casos.
- **Siguiente sesión sugerida:** `src/render/` (renderer de 1.285 líneas, mallas y sincronización GPU) y el resto de `engine/app.rs` (pantallas, input, streaming de render). Y, para completar lo ya auditado, reproducir P0-1 y P1-6 en ejecución cuando el checkout esté restaurado.

## 1. Revisión documental

La estructura documental existente es suficiente como punto de partida (`README.md`, `ARCHITECTURE.md`, `DECISIONS.md`, `docs/worldgen.md`, `docs/performance.md` y los README de assets/capturas), pero los hechos de operación y versión no están centralizados ni completamente sincronizados.

### Qué está obsoleto o se contradice

| Archivo/zona | Observación | Acción documental |
|---|---|---|
| `README.md` — resumen de versiones y tests | La cabecera presenta v0.47.0 y formato 6, pero el resumen conserva una mención a `FORMAT_VERSION = 5`; la cuenta de tests dice 429 mientras la última decisión dice 432. | Dejar versión, generadores, formatos y test count en un único bloque “estado verificado”; si no se ejecutó la suite, marcar el conteo como histórico/no verificado. Quitar del resumen actual valores pasados. |
| `README.md` — ubicación de los mundos | Dice que `world.vf` queda junto al ejecutable; Fase 2 encontró que la base por defecto deriva del directorio de trabajo (o `SOLARIA_HOME`). | Documentar ruta efectiva, precedencia de `SOLARIA_HOME`, comportamiento al lanzar desde un acceso directo y ejemplo por sistema operativo. |
| `src/lib.rs` — docstring de crate | Describe como únicas dependencias `winit` y `wgpu`, llama “futuro” a meshing/iluminación/guardado, y contiene un gran resumen histórico con versiones/formatos anteriores. | Reescribir como orientación breve del estado actual; la lista de módulos y el histórico deben residir en README/arquitectura/decisiones, no en el prólogo API. |
| `src/world/mod.rs` — docstring | Habla de un chunk estático y de hitos de v0.3.x aunque el proyecto tiene columnas, streaming, generadores, fluidos y guardado versionado. | Sustituir por responsabilidades y límites actuales del módulo `world`; enlazar a `ARCHITECTURE.md`. |
| `src/render/mod.rs` y demás comentarios | Algunos comentarios son históricos o redundantes; las rutas de shader no tienen una convención uniforme. | Mantener historia solo si aporta contexto; documentar invariantes y el límite GPU/CPU, no narrar código evidente. |
| `src/world/library.rs` — comentario de ruta | Fase 2 reporta comentario “junto al ejecutable” contra resolución real desde el directorio de trabajo. | Corregir junto con README y tests de la ruta efectiva. |
| Comentarios de `src/` | Fase 2 reporta ~295 ocurrencias de mojibake en 40 archivos. Los comentarios de `font.rs` pueden incluir tablas de glifos y necesitan tratamiento especial. | Corregir texto UTF-8 dañado por grupos pequeños; distinguir texto explicativo de tablas/datos y verificar el diff byte a byte. Añadir regla de editor UTF-8 en `.editorconfig` si procede. |
| `docs/performance.md` | Hay cifras históricas útiles, pero no todas reflejan Larion/v0.47 ni identifican claramente la antigüedad del hardware/configuración. | Etiquetar fecha, commit/tag, build, backend, GPU, radios, cantidad de muestras y comandos por resultado; separar baseline histórica de medición vigente. |
| `docs/worldgen.md` / `ARCHITECTURE.md` | Hay documentación sólida de pipeline e invariantes, pero los generadores, las versiones y las limitaciones visuales necesitan fuente única y sincronización. | Añadir matriz de generador (identificador, versión, compatibilidad, límites y cómo elegirlo) y enlazar desde ambas páginas. |
| `docs/audit/02-auditoria-codigo.md` | En P1-8 aparece una nota ajena al proyecto (“robloxstudio no aplica”), probablemente residuo editorial. | Retirar esa frase en una corrección editorial futura; conservar el método pertinente de profiling del motor. |

### Qué falta y qué documentar primero

**Primero (P0/P1 documental):** (1) estado de checkout y cómo trabajar sin perder las eliminaciones locales; (2) manejo de `world.vf` ausente, corrupto o de formato futuro, `.bak` y riesgo de sobrescritura; (3) ruta real de mundos y backups; (4) cifra de tests con estado de verificación; (5) versión de formato/generador actual.

**Después:** guía reproducible de build/test/lint en Windows/Linux/macOS y requisitos de GPU; flujos para crear, importar, duplicar, renombrar y borrar mundos; recuperación ante error; matriz Legacy16/Graph/Larion; presupuesto de memoria y limitaciones de render; y comandos de ejemplos con semilla/argumentos/salida esperada.

**Docstrings/comentarios:** documentar APIs públicas y contratos no obvios (determinismo `(seed,x,z)`, fronteras cargadas/no cargadas, serialización/migración, vida de snapshots del guardado, ownership de buffers GPU y quién marca datos como `dirty`). No expandir comentarios que solo repitan el nombre de una función.

## 2. Organización de assets, shaders y licencias

### Evaluación organizativa

- **Assets runtime y fuentes editables están mezclados.** `assets/` contiene imágenes cargadas por runtime, `.aseprite`, fuentes `.bbmodel`, paletas, manifiesto JSON y `backup/`. La subcarpeta `src/` no agrupa hoy todas las fuentes: el GUI esperado sería `assets/src/ui/gui.aseprite`, mientras el snapshot contiene fuentes celestes en `assets/src/` y `assets/src/models/`.
- **Separación de shaders irregular.** Cinco WGSL viven directamente bajo `src/render/` y solo `water.wgsl` está bajo `src/render/shaders/`. La carga por `include_str!` está ligada a las rutas actuales, de modo que cualquier movimiento requiere actualizar llamadas y validación de shaders en la misma tarea.
- **Nombres ambiguos/heterogéneos.** `torch_32.png` y `torch_sprite.png` no hacen evidente cuál es canónico y cuál es variante; `gen_atlas_final.py` suena a artefacto de un proceso, frente a nombres de rol. Aplicar nombres de rol y distinguir `source`, `runtime`, `generated` y `archive` en el manifiesto.
- **Fuente de verdad de GUI no clara.** `assets/README.md` llama manifiesto a `gui.json` y dice que lo emite `tools/gen_gui.js`, pero el script inspeccionado dibuja `gui.png` y guarda el `.aseprite`; sus regiones están escritas como constantes/código, y no genera el JSON. Además, el runtime usa regiones constantes y no carga `gui.json` según el inventario. Elegir una sola fuente de verdad: generar el manifiesto desde el script/código, o declararlo manual y validar consistencia con un test.
- **Fuente editable de GUI no versionada en el snapshot.** El script guarda `assets/src/ui/gui.aseprite`, pero ese path no está en el árbol de `HEAD`. Documentar que es salida generada y no fuente de entrada, o incluir la fuente editable si debe ser mantenible.
- **Licencias.** Hay `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE` y `Cargo.toml` declara `MIT OR Apache-2.0`: no se observa ausencia de licencia general del código. Lo que falta es un registro explícito y granular de procedencia/licencia de imágenes, fuentes, modelos, paletas y capturas, incluyendo contenido propio, generado o de terceros. No atribuir una licencia a assets sin verificar sus autores y fuentes.
- **Assets posiblemente prescindibles:** `assets/backup/` se declara histórico y no runtime; dos modelos de antorcha en esa carpeta tienen hash idéntico. `star.png` se describe como reserva sin consumidor conocido. `gui.json` y la herramienta medieval no participan en el runtime observado, pero son condicionales, no borrados seguros hasta resolver manifiesto y usos externos.
- **No hay evidencia de temporales versionados de build.** `.gitignore` excluye `target/`, `.vf`, `.vf.bak`, `.vf.tmp` y `saves/`. El estado local no permite inventariar archivos ignorados que pudieran existir fuera del snapshot.

### Estructura objetivo propuesta

Aplicar gradualmente, tras crear un checkout limpio de trabajo y establecer pruebas de carga/rutas; no mover assets ahora porque el árbol de fuentes no está disponible en la carpeta de trabajo.

```text
assets/
├── runtime/
│   ├── textures/blocks/atlas.png
│   ├── textures/ui/gui.png
│   ├── textures/sky/{sun,moon,star}.png
│   └── textures/items/{torch_*.png}
├── source/
│   ├── aseprite/{ui,sky,items}/...
│   ├── blockbench/{items,sky}/...
│   └── palettes/*.gpl
├── manifests/assets.json              # origen, uso, licencia, generador, dimensiones
└── archive/                           # solo material histórico conservado

src/render/shaders/
├── scene.wgsl
├── water.wgsl
├── sky.wgsl
├── model.wgsl
├── ui.wgsl
└── highlight.wgsl

tools/assets/                          # generadores/reproductores de assets
tools/dev/                             # capturas y utilidades de desarrollo
docs/audit/{01-inventario,02-auditoria-codigo,03-docs-arte-plan}.md
```

Convenciones propuestas: nombres ASCII en `snake_case`; no usar `final` para un asset canónico estable; sufijos o metadatos para indicar fuente/variante; el manifiesto enumera si cada asset se carga en runtime, se regenera, está solo como fuente o es archivo. Mantener un identificador estable de asset y declarar licencia/procedencia antes de incorporar arte externo. La reorganización debe actualizar rutas de carga, scripts, README y snapshots de pruebas de assets en un solo cambio rastreable.

## 3. Plan de remediación priorizado

### Preservación previa (bloqueante, antes de P0 de código)

El checkout registra 278 eliminaciones ajenas al informe; no asumir que son accidentales ni ejecutar `git restore`/`reset`. Preservar el estado actual y hacer el trabajo de código en un árbol de trabajo separado basado en los objetos locales:

```powershell
git status --short
git diff --binary > ..\SolariaVoxel-local-diff.patch
git worktree add ..\SolariaVoxel-audit HEAD
Copy-Item -Recurse docs\audit ..\SolariaVoxel-audit\docs\
Copy-Item AUDIT.md ..\SolariaVoxel-audit\AUDIT.md
```

El patch es una salvaguarda de las eliminaciones rastreadas, no un registro de los informes no rastreados ni de datos ignorados; por eso se copian también `docs/audit/` y `AUDIT.md`, y se deben respaldar otros archivos de usuario si los hubiera. Comprobar el resultado antes de actuar. No hacer `git clean` ni borrar carpetas globalmente. La divergencia de HEAD respecto a GitHub es cero; el bloqueo es el árbol de trabajo local.

### Lista de borrado seguro propuesta

“Seguro” aquí significa que no forma parte del runtime declarado por el snapshot. La decisión de descartar archivo histórico sigue siendo del mantenedor. Los comandos son propuestas para una copia de trabajo limpia, no se ejecutaron.

| Candidato | Justificación | Paso/comando sugerido | Confianza / riesgo |
|---|---|---|---|
| `assets/backup/models/solaria_torch.backup.bbmodel` | Duplicado byte a byte de `solaria_torch.pre28.bbmodel` según hash Git. Si se conserva la carpeta histórica, basta conservar una copia. | `git rm assets/backup/models/solaria_torch.backup.bbmodel` | Alta; riesgo runtime bajo, archivo histórico bajo. |
| `assets/backup/` completo | El `assets/README.md` declara explícitamente que no se carga en runtime y permite borrarlo si no hace falta trazabilidad. Puede contener historia de arte que se desee conservar. | Tras respaldar/revisar: `git rm -r assets/backup/` | Alta para no-runtime; riesgo runtime bajo, pérdida de trazabilidad media. |

No incluir en borrado seguro todavía: `assets/star.png` (reserva, potencial uso futuro), `assets/gui.json` (manifiesto con el estado de generación sin resolver), `tools/gen_atlas_medieval.py` (variante reproducible), fuentes Aseprite/Blockbench antiguas (proveniencia/referencia), capturas indexadas ni ningún código Rust. Para estos, decidir primero si se eliminan, se archivan o se conectan a un flujo reproducible.

### P0 — pérdida de datos y capacidad de trabajar

1. **P0 operativo — desbloquear auditoría sin tocar el checkout sucio.** Preparar el worktree separado descrito arriba, capturar `git status` y comprobar que contiene `Cargo.toml`/fuentes desde el `HEAD` local. No restaurar los 278 borrados sobre el árbol actual sin instrucción explícita.
2. **P0-1 — corrupción/formato futuro se interpreta como mundo nuevo y puede sobrescribir el `.bak` bueno.** Separar `NotFound` de `Decode`, `TooNew`, `CorruptChunk` y demás; bloquear la entrada/guardado si el archivo existe pero es ilegible; ofrecer recuperación desde `.bak` sin destruir la única copia válida. Añadir tests de integración a través del flujo de app antes de liberar.

### P1 — integridad de datos, funcionamiento del mundo y riesgo alto

Orden recomendado después de P0, con los IDs de `02-auditoria-codigo.md`:

1. **P1-5 recuperación `.bak`** al fallar el principal, conservando el principal dañado para diagnóstico y probando recuperación real en la aplicación.
2. **P1-1 guardado en vuelo**: coalescer cambios y encolar snapshot final al cerrar/cambiar mundo; cubrir carrera con worker mediante test determinista.
3. **P1-2 importación legado recurrente**: marcar/mover atómicamente `world.vf` legado o registrar estado de importación; test borrado de todos + reinicio.
4. **P1-3 ruta de guardado**: decidir política (directorio estable de usuario vs cwd), hacer coincidir implementación, `SOLARIA_HOME`, docstrings y README; migración/aviso para rutas previas si cambia.
5. **P1-7 metadatos inválidos**: no degradar en silencio semilla/generador; mostrar error o recuperación explícita y conservar la identidad del mundo.
6. **P1-6 fronteras de agua** *(a verificar)*: reproducir en frontera de columnas cargadas y no cargadas; fijar contrato para vecino desconocido (no seco) y testear carga/descarga.
7. **P1-4 integridad de payload** *(a verificar)*: comprobar qué corrupción detecta LZ4; definir checksum/versionado si se confirma el hueco, con migración compatible.
8. **P1-8 escaneo de agua en cruces** *(a verificar/perf)*: medir con perfil y mundos con fluidos; optimizar solo si se reproduce el coste, preferiblemente índice por sección/columna.

### P2 — robustez, rendimiento, coherencia y deuda organizativa

Mantener todos los IDs de la Fase 2 en el backlog. Los marcados “verificar” no deben convertirse en cambio funcional sin reproducción.

| ID | Acción resumida | Estado de Fase 2 |
|---|---|---|
| P2-1 | Confirmar y corregir si procede la persistencia/dirty de flujo de agua. | Verificar reinicio y forma del flujo. |
| P2-2 | Hoist de `has_flow_storage`/reserva al serializar columna. | Confirmado por lectura; medir antes/después. |
| P2-3 | Reutilizar buffers descomprimidos durante carga/validación de chunk. | Confirmado por lectura; medir. |
| P2-4 | Acotar longitud antes de asignación/descompresión LZ4. | Verificar API exacta de `lz4_flex` y entrada malformada. |
| P2-5 | Preservar error de cabecera; evitar `unwrap_or(0)` engañoso. | Confirmado por lectura. |
| P2-6 | Validar bytes sobrantes tras decodificar archivo. | Confirmado por lectura; definir compatibilidad. |
| P2-7 | Revisar durabilidad de rotación/rename y fallo entre pasos. | Confirmado por lectura; diseñar por plataforma. |
| P2-8 | Evitar clonar `ChunkRecord` en cada migración si perfil lo justifica. | Confirmado por lectura; optimización. |
| P2-9 | Definir determinismo binario/orden estable para snapshots de guardado. | Confirmado; evaluar coste y utilidad. |
| P2-10 | Sincronizar `level.json` temporal antes de rename. | Confirmado por lectura. |
| P2-11 | No silenciar fallos de `read_dir`; impedir colisión/sobrescritura de slug. | A verificar con error de listado y slug ya existente. |
| P2-12 | Hacer duplicación atómica y coordinarla con guardados activos. | Confirmado por lectura. |
| P2-13 | Validar slugs/nombres en API de borrado/rename/duplicate/touch. | Confirmado por lectura; defensa en profundidad. |
| P2-14 | Leer cabecera necesaria para importar sin descomprimir mundo completo y propagar error. | Confirmado por lectura. |
| P2-15 | Evitar reintento continuo si se borra mundo activo. | A verificar. |
| P2-16 | Perfil del tick: timer por celda, búsquedas, HashSet y dirty list. | Confirmado por lectura; optimizar tras medir. |
| P2-17 | Verificar qué ocurre con cola de agua fuera del radio y al cargar después. | A verificar. |
| P2-18 | Reservar capacidad de la malla de fluido y evitar cómputos/dispatch innecesarios. | Confirmado por lectura; medir. |
| P2-19 | Verificar UV de caras laterales parciales en imagen/render. | A verificar visualmente en GPU. |
| P2-20 | Reparar mojibake en comentarios y normalizar UTF-8 sin tocar tablas de glifos. | Confirmado; ~295 apariciones en 40 archivos según Fase 2. |
| P2-21 | Unificar caminos de carga del arranque y `load_world`. | Confirmado por lectura. |

Extensiones derivadas de Fase 1 (backlog de mantenimiento): sincronizar resumen de README/docstrings; corregir asset README y manifiesto GUI; registrar procedencia/licencia de assets; añadir el `.aseprite` esperado o documentar su condición de generado; definir política de archivos históricos; validar Larion visualmente y auditar `render/`, resto de `app.rs`, `ui/`, `player/`, `physics.rs`, `worldgen/` y ejemplos, todos fuera del alcance de Fase 2.

### Roadmap sugerido por sprints

| Sprint | Objetivo | Entregables/criterio de salida |
|---|---|---|
| 0 — Preparación | Crear espacio de trabajo seguro y baseline reproducible. | Worktree local separado desde `HEAD`; confirmar Rust/Cargo/GPU disponibles; `cargo fmt --check`, `cargo test`, `cargo clippy`; registrar test count, commit y limitaciones. Mantener intactas las eliminaciones del árbol original. |
| 1 — Datos primero | P0-1, P1-5 y P1-1. | La corrupción/formato futuro nunca se convierte en mundo vacío ni destruye backup; recuperación probada; cerrar/cambiar durante guardado persiste el último snapshot; tests de integración pasan. |
| 2 — Biblioteca segura | P1-2, P1-3, P1-7 y P2-10..15/P2-21 relacionados. | Política de ruta documentada; importar una vez; errores de metadata visibles; slugs seguros; operaciones de biblioteca coherentes/atómicas; cargar mundo usa un flujo compartido. |
| 3 — Simulación y rendimiento | Reproducir P1-6/P1-8; P2-1, P2-16, P2-17; luego P2-2/3/8/14/18 según perfil. | Tests de frontera de agua y reinicio; benchmark/profile con escenario y hardware descritos; solo optimizaciones con baseline y resultado comparable. |
| 4 — Integridad y calidad documental | P1-4 + P2-4..9 y P2-20; actualizar README/docstrings/docs. | Payload inválido limitado/detectado; contratos de guardado claros; UTF-8 saneado; versiones/tests/rutas correctos; enlaces y comandos probados en el worktree limpio. |
| 5 — Assets, render y cierre | Normalizar manifests/carpetas, auditar shaders/UI/render y validar visualmente Larion/agua. | Assets con uso/origen/licencia/receta; shaders compilados y nombres organizados; pruebas visuales/capturas GPU registradas; borrar solo lo aprobado en la lista. |

## 4. Criterios de cierre de la remediación

- Cero pérdida/sobrescritura silenciosa al cargar, guardar, migrar, recuperar o cambiar de mundo.
- Tests de integración para corrupción de save, backup, importación legado, mundos activos y fronteras de agua; las hipótesis de Fase 2 quedan marcadas como reproducidas o descartadas.
- `README.md`, `ARCHITECTURE.md`, docstrings, `docs/worldgen.md` y `docs/performance.md` coinciden en versión, rutas, generadores, formatos y resultados medidos.
- Assets runtime, fuentes y archivo se distinguen; cada asset tiene procedencia/licencia/uso identificados o un estado explícito “por verificar”. Un solo contrato gobierna `gui.png`, sus regiones y la herramienta generadora.
- Auditorías restantes de Fase 1 quedan delegadas a sesiones específicas; no se considera revisado código que quedó fuera de Fase 2.
- Las eliminaciones locales previas se mantienen intactas hasta que su propietario decida restaurarlas, conservarlas o descartarlas.
