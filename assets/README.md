# Assets de Solaria Voxel

## Canonicos (los carga el motor)

| Archivo | Tamano | Uso |
| ------- | ------ | --- |
| `atlas.png` | 64x80 | Atlas de texturas de bloques (4 columnas x 5 filas de tiles 16x16). Lo carga `world/atlas.rs`; si falta o cambia de tamano, el motor cae al atlas procedural. |
| `gui.png` | 256x160 | Iconos de la interfaz 2D (hotbar / inventario). |
| `torch_32.png` | 16x16 | Sprite fuente de la antorcha. |
| `torch_sprite.png` | 16x16 | Sprite fuente alternativo de la antorcha. |
| `paleta_master.gpl` | - | Paleta maestra de LibreSprite (documenta los colores del atlas). |
| `paleta_medieval.gpl` | - | Paleta medieval de LibreSprite. |
| `models/solaria_torch.bbmodel` | - | Modelo fuente Blockbench de la antorcha. |
| `src/models/solaria_celestial.bbmodel` | - | Modelo fuente Blockbench del "rig celeste" (cubo-sol ~22 u, cubo-luna ~17 u, giro fijo yaw 30 grados / pitch -20). Es **referencia de proporciones**: el sol/luna se dibujan por interseccion rayo-caja en `render/sky.wgsl` (sin texturas), con las medidas reflejadas en `render/sky.rs`. |

Regenerar el atlas (`tools/gen_atlas_final.py`) reproduce `atlas.png` byte a byte
desde las matrices embebidas; los `.gpl` y los sprites son la referencia de
color/forma.

## Herramientas (`tools/`)

- `gen_atlas_final.py` — regenera `atlas.png` byte-identico.
- `gen_atlas_medieval.py` — variante medieval del atlas.
- `screenshot.ps1` — captura la ventana del ejecutable en ejecucion.

## `backup/`

Copias historicas de arte que **no** carga el motor: variantes anteriores del
atlas y de los sprites de la antorcha (`*_pre28`, `*_med`), los mapas de atlas en
texto y el `.bbmodel` previo. Se conservan por trazabilidad; si no hacen falta,
se pueden borrar sin afectar al motor.