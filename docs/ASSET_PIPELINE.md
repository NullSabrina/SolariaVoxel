# Pipeline de assets

## Texturas de bloque

1. **Fuente**: PNG 16x16 CC0 de Minetest Game en
   `assets/src/textures/minetest/`. La fuente editable completa del atlas esta
   en `assets/src/textures/blocks.aseprite` (LibreSprite).
2. **Construccion**: `tools/gen_atlas.ps1` compone un atlas de **256x96**
   (8 columnas x 3 filas de tiles **32x32**), escala cada fuente 2x con
   nearest-neighbor y genera:
   - `assets/atlas.png`
   - `assets/atlas.json` (`indice -> nombre, x, y, w, h`)
3. **Runtime**: `world/atlas.rs` trocea el PNG en capas y `render/pipeline.rs`
   lo sube como **texture array** `D2Array`, una capa por tile. Los mipmaps se
   generan por capa en CPU (`atlas::mip_chain`), evitando bleeding entre tiles.
4. **Fallback**: si falta el PNG, `atlas::build_pixels()` genera un set
   procedural 32x32 con matrices 16x16 escaladas.

### Anadir un tile

1. Anade el PNG 16x16 a `assets/src/textures/minetest/`.
2. Anade la entrada en `SOURCES`/`$map` de `tools/gen_atlas.ps1`.
3. Anade el nombre en `atlas::TILE_NAMES` y el tile en
   `registry::BLOCKS[..].face_tiles`.
4. Regenera:

```powershell
powershell -File tools/gen_atlas.ps1
cargo test --release atlas::tests
```

## Hojas y transparencia

- `Leaves` es **solida para colision** (no se atraviesa), pero se dibuja en
  `Cutout`: los huecos del PNG tienen alfa 0 y no ocluyen las caras vecinas.
- La oclusion usa `Block::occludes()` (solo `RenderKind::Opaque`), no
  `is_solid()`.
- La fuente CC0 de hojas trae ~37% de huecos. `gen_atlas.ps1` la **densifica**:
  rellena la mayoria con verdes oscuros de la propia hoja y deja ~13% de huecos
  para que la copa se lea frondosa sin parecer una reja.
- `default_grass_side.png` de Minetest es un **overlay transparente** (se
  compone sobre `default_dirt.png`). El atlas compone ambos: el bloque de
  hierba tiene hierba arriba y tierra opaca en el resto de la cara lateral.
- El agua tiene alfa parcial y va al pase translucido.

## Modelos de bloque

Los bloques no cubicos pueden definirse con JSON compatible con Blockbench:

```json
{
  "texture_size": 32,
  "textures": { "torch": "8" },
  "elements": [
    {
      "from": [7, 0, 7],
      "to": [9, 10, 9],
      "faces": {
        "north": { "texture": "#torch", "uv": [13, 16, 19, 32] }
      }
    }
  ]
}
```

- `world/block_model.rs` resuelve `#ref`, indices de tile, nombres de
  `TILE_NAMES` y el padre `cube`.
- `mesher::emit_model` emite las caras del cuboide. Esta reservado para bloques
  no cubicos nuevos; la antorcha conserva su modelo original de Blockbench.
- Guarda siempre la fuente `.bbmodel` en `assets/models/`.
- La antorcha original vive en `assets/models/solaria_torch.bbmodel`; no se
  sustituye por el JSON de ejemplo.

## Verificacion

```powershell
cargo test --release atlas::tests block_model
cargo run --release --example gen_atlas
```

Para ver el arte: abre `assets/src/textures/blocks.aseprite` en LibreSprite.
