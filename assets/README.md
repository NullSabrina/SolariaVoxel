# Assets de Solaria Voxel

## Canonicos (los carga el motor)

| Archivo | Tamano | Uso |
| ------- | ------ | --- |
| `atlas.png` | 256x96 | Atlas de bloques **32x32** (8 columnas x 3 filas). Fuente: texturas CC0 de Minetest Game + derivados, escaladas 2x. Lo carga `world/atlas.rs`; si falta, cae al fallback procedural. |
| `atlas.json` | - | Manifiesto de tiles: `indice -> nombre, x, y, w, h`. |
| `src/textures/blocks.aseprite` | - | Fuente editable del atlas en LibreSprite. |
| `src/textures/minetest/*.png` | 16x16 | Fuente **CC0** de Minetest Game usada para construir el atlas (grass, dirt, stone, sand, tree, leaves, water, snow, torch, lava, obsidian...). |
| `gui.png` | 256x256 | Arte de interfaz 2D con **bisel** estilo Minecraft: hotbar, ranuras (normal/hover/seleccionada), panel, botones (normal/hover/pulsado/desactivado), pestana, flecha, mirilla y atenuador. Pintado en **LibreSprite**; regenerable con `tools/gen_gui.js`. Si falta, el motor cae al procedural de `render/gui.rs`. |
| `gui.json` | - | Manifiesto **legible** de regiones de `gui.png`. La fuente de verdad son las constantes de `src/render/gui.rs`; un test exige que coincidan. |
| `src/ui/gui.aseprite` | - | **Salida generada** por `tools/gen_gui.js` (no versionada). No es una fuente requerida: el arte editable se repinta con el script. |
| `torch_32.png` | 16x16 | Sprite fuente de la antorcha. |
| `torch_sprite.png` | 16x16 | Sprite fuente alternativo de la antorcha. |
| `paleta_master.gpl` | - | Paleta maestra de LibreSprite (documenta los colores del atlas). |
| `paleta_medieval.gpl` | - | Paleta medieval de LibreSprite. |
| `models/solaria_torch.bbmodel` | - | Modelo fuente Blockbench de la antorcha. |
| `sun.png` | 16x16 | **Superficie** del cubo-sol (amarillo moteado emisivo). La carga `render/sky.rs`; si falta, fallback procedural. Regenerable: `cargo run --example gen_celestial`. |
| `moon.png` | 16x16 | **Superficie** del cubo-luna (gris con crateres). La fase se aplica en el shader sobre el cubo. Fallback procedural si falta. Regenerable: `cargo run --example gen_celestial`. |
| `star.png` | 8x8 | Estrella de 4 puntas (reserva; las estrellas del cielo son procedurales). Fuente: `src/star.aseprite`. |
| `src/*.aseprite` | - | Fuentes editables en LibreSprite de las versiones **anteriores** (sprite 2D): `sun.aseprite`, `star.aseprite`. |
| `src/models/solaria_celestial.bbmodel` | - | Modelo fuente Blockbench del "rig celeste" (cubo-sol ~22 u, cubo-luna ~17 u, giro fijo yaw 30 grados / pitch -20). Es **referencia de proporciones**; el sol/luna se dibujan por interseccion rayo-caja con estas superficies en `render/sky.wgsl`, con las medidas en `render/sky.rs`. |

`sun.png`/`moon.png` son **superficies por cara** (no discos 2D): se aplican a las
6 caras del cubo y el sombreado por cara + la fase dan la forma. Las fuentes
`.aseprite` que quedan son de la version anterior (sprite 2D con disco), conservadas
por trazabilidad.

Regenerar el atlas: `tools/gen_atlas.ps1` compone las fuentes CC0 de
`assets/src/textures/minetest/` en `assets/atlas.png` (32x32, mipmaps en runtime).
`cargo run --release --example gen_atlas` escribe el fallback procedural y
`atlas.json` si no hay fuentes.

## Herramientas (`tools/`)

- `gen_atlas.ps1` — compone el atlas 32x32 desde las fuentes CC0 de Minetest.
- `gen_atlas_final.py` — atlas historico procedural.
- `gen_atlas_medieval.py` — variante medieval del atlas.
- `gen_gui.js` — repinta `gui.png` desde LibreSprite y guarda una copia
  `assets/src/ui/gui.aseprite` (salida generada, no versionada).
- `screenshot.ps1` — captura la ventana del ejecutable en ejecucion.

## Procedencia y licencia

Los recursos propios del proyecto se distribuyen bajo la misma licencia dual
que el codigo (`MIT OR Apache-2.0`). Las texturas de
`assets/src/textures/minetest/` son **CC0** de Minetest Game
(`luanti-org/minetest_game`); se usan como fuente del atlas y no requieren
atribucion, aunque se acredita aqui. El resto de `.aseprite`/`.bbmodel` son
material historico del proyecto.

El material historico que no usa el runtime (`assets/backup/`) se retiro en
v0.48.0; queda en el historial de Git si hace falta consultarlo.