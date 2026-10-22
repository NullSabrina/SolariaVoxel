# Auditoría de SolariaVoxel

Auditoría del snapshot local `HEAD` **v0.47.0** (`4d1c32d`). Este commit coincide con `origin/master`; no se hizo una auditoría independiente del remoto. La auditoría está dividida en tres informes:

1. [Fase 1 — Inventario](docs/audit/01-inventario.md): estructura, stack, módulos, candidatos y hotspots.
2. [Fase 2 — Auditoría de código](docs/audit/02-auditoria-codigo.md): persistencia/biblioteca, streaming y agua; hallazgos P0/P1/P2.
3. [Fase 3 — Documentación, organización y plan](docs/audit/03-docs-arte-plan.md): revisión documental/artística, lista de borrado, prioridades y roadmap.

## Resumen ejecutivo

- **Fase 1:** motor voxel propio en Rust 2024, con `winit`/`wgpu`, un ejecutable y una librería; módulos centrales `world`/`worldgen`, `engine`, `render` y `ui`.
- **Fase 2:** 1 hallazgo P0, 8 P1 y 21 P2. El P0 principal es tratar saves corruptos o de formato futuro como mundos nuevos, con posible sobrescritura del backup válido. Varios hallazgos siguen pendientes de reproducción.
- **Fase 3:** prioriza proteger saves, asegurar guardados concurrentes, corregir biblioteca/rutas, medir streaming/agua y después sincronizar documentación y assets.
- **Checkout local:** al iniciar la auditoría, 278 archivos rastreados aparecían borrados y faltaba `Cargo.toml`; se trabajó desde los blobs locales de Git sin restaurar ni sobrescribirlos. Los tests/build no se pudieron ejecutar.
- **Plan de continuación:** usar un worktree separado tras preservar el estado actual, reproducir P0/P1, implementar con tests y actualizar docs/organización; no ejecutar borrados históricos sin decisión explícita.

El detalle, las limitaciones y las anclas para sesiones posteriores están en los tres informes enlazados arriba.

## Estado de remediación (v0.48.0, 2026-10-21)

Tras restaurar el checkout (`git restore .`, conservando `AUDIT.md` y `docs/audit/`)
se aplicaron las correcciones sobre el código real y se verificaron con la suite:

| Área | Estado |
|---|---|
| **P0-1** save ilegible / formato futuro | **Corregido.** `save::load_world` distingue `Missing`/`Loaded`/`Recovered`/`Unreadable`; nunca abre vacío ni sobrescribe; el dañado va a `world.vf.corrupt`. Tests en `save` y `app`. |
| **P1-1** guardado en vuelo | **Corregido.** `save_pending` + snapshot forzado en cierre/cambio. |
| **P1-2** import legacy recurrente | **Corregido.** Marca `.legacy-imported`. |
| **P1-3** ruta base | **Corregido.** `SOLARIA_HOME` > `saves/` existente en cwd > directorio del ejecutable (fallback a cwd). |
| **P1-4** integridad del payload | **Corregido.** `FORMAT_VERSION = 7` con checksum FNV-1a de bloques y fluido; migración v6→v7. |
| **P1-5** recuperación `.bak` | **Corregido.** Automática y probada (incluido principal ausente). |
| **P1-6** bordes de agua | **Corregido.** `FluidGrid::is_loaded`: un vecino descargado **congela**, no seca. Test de borde. |
| **P1-7** metadatos ilegibles | **Corregido.** Se rechaza la entrada con mensaje; no se degrada semilla/generador. |
| **P1-8** coste al cruzar chunk | **Corregido.** Cache `Column::flow_cells` (sin escanear 98 304 celdas por columna). |
| **P2-1** flujo no persistido | **Corregido.** `set_water_raw` marca `dirty` al cambiar. |
| **P2-2..P2-9** | **Corregidos** (hoist, validación única, prefijo acotado, error de cabecera, bytes sobrantes, `sync_dir`, migración sin clones, `BTreeMap` determinista). |
| **P2-10..P2-15** | **Corregidos** (fsync de `level.json`, listado que propaga, duplicado atómico, slugs validados, import solo cabecera, borrado del mundo activo). |
| **P2-16** | **Parcial.** Reloj cada 64 celdas; el resto se deja para medir antes de optimizar. |
| **P2-17** | **Verificado, sin cambio.** Al volver al radio, `wake_sim_water` re-encola el agua propia de la columna; no se pierde trabajo. |
| **P2-18 / P2-19** | **Corregidos.** Genéricos, capacidad, esquinas perezosas y UV lateral a escala. |
| **P2-20** mojibake | **Falso positivo.** Los `.rs` son UTF-8 válido; solo hay acentos y guiones tipográficos legítimos. |
| **P2-21** dos caminos de carga | **Corregido.** `load_world_data` único para arranque y menú. |

**Verificación:** `cargo test` → **467 pasan, 0 fallan, 1 ignorado (+1 doctest)**;
`cargo clippy --all-targets` limpio; `cargo fmt`.

**Además (2026-10-21):** se ejecutó la lista de borrado seguro eliminando
`assets/backup/`, y se re-midieron los benchmarks en `--release`
(`docs/performance.md`): R12 en vivo **120.1 MB CPU + 358.1 MB GPU (~478 MB)**,
dentro del objetivo < 500 MB; Larion **1.54×** de mediana (rango 1.43–1.59). R16
no se re-midió por falta de RAM libre (1.4 GB).
