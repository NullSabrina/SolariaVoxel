# Capturas — Solaria Voxel

Indice de capturas por version (`vX.Y.Z-descripcion.png`, o `.png` cuando son la
primera de su version). Se generan con `tools/screenshot.ps1` a partir del
ejecutable en modo demo (ver `README.md`).

## Fundamentos (`v0.1`–`v0.6`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.1.0.png` `v0.1.1.png` `v0.1.2.png` | Ventana, camara y primeros triangulos. |
| `v0.2.0.png` `v0.2.1.png` | Primer cubo y su textura. |
| `v0.3.0.png` `v0.3.1.png` `v0.3.2.png` `v0.3.2_fix.png` | Primer chunk y correcciones de UV/culling. |
| `v0.4.0.png` `v0.4.1.png` | Terreno inicial. |
| `v0.5.0.png` `v0.5.1.png` `v0.5.2.png` | Camara/culling/cielo. |
| `v0.6.0.png` `v0.6.1.png` `v0.6.2_torch.png` | Meshing y antorcha. |
| `v0.6.4_amanecer.png` `v0.6.4_dia.png` `v0.6.4_noche.png` | Ciclo dia/noche. |
| `v0.6.5_colision.png` | Colision del jugador. |

## Mundo jugable (`v0.7`–`v0.9`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.7.0_biomas.png` `v0.7.1_terreno.png` | Biomas y terreno. |
| `v0.7.7_dirt.png` | Materiales. |
| `v0.7.8_oceano.png` | Oceanos/agua. |
| `v0.7.9_arboles.png` | Vegetacion. |
| `v0.8.0_hotbar.png` `v0.8.1_hotbar.png` `v0.8.1_spawn.png` | Hotbar e inventario. |
| `v0.8.2_mesa.png` `v0.8.3_mesa.png` | Mesa de crafteo. |
| `v0.8.4_oceano.png` `v0.8.5_biomas.png` `v0.8.6_lava.png` `v0.8.7_spawn.png` `v0.8.8_agua.png` | Mundo, lava y agua. |
| `v0.8.9_antorcha.png` `v0.8.10_antorcha.png` | Antorchas y luz. |
| `v0.8.11_oceano.png` | Oceano. |
| `v0.8.14_normal.png` `v0.8.14_streaming.png` | Streaming. |
| `v0.8.15_luz.png` `v0.8.16_secciones.png` `v0.8.17_meshing_async.png` `v0.8.18_buffers.png` | Luz y reuso de mallas. |
| `v0.9.2-ocean.png` `v0.9.3-ocean.png` | Fluidos (agua estable). |

## Optimizacion (`v0.10`–`v0.16`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.10.0-torch.png` | Registry de bloques (antorcha). |
| `v0.11.0-ocean.png` | Memoria dispersa. |
| `v0.12.0-ocean.png` `v0.13.0-ocean.png` `v0.14.0-stats.png` | Culling, timestep fijo y overlay F3. |
| `v0.15.2-cave.png` | Luz de bloque regional. |
| `v0.16.0-radius8.png` | Radio de vista configurable. |

## Worldgen por etapas (`v0.17`+)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.17.0-ocean.png` `v0.17.0-worldgen.png` | FASE 1/2: continentes, costas, cordilleras. |
| `v0.18.0-biomes.png` | FASE 3: bioma por region celular. |
| `v0.19.0-river.png` | FASE 5: rios serpenteantes. |
| `worldgen_preview_13371_biome.png` `worldgen_preview_13371_river.png` | Previews offline (`examples/worldgen_preview.rs`). |

## Cielo y atmosfera (`v0.30`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.30.0_sky_dawn.png` | Amanecer (elevacion solar ~7 grados): horizonte dorado, cenit azul. |
| `v0.30.0_sky_noon.png` | Mediodia: cenit azul profundo hacia una bruma palida en el horizonte. |
| `v0.30.0_sky_sunset.png` | Atardecer (golden hour): horizonte naranja y cielo malva. |
| `v0.30.0_sky_night.png` | Noche profunda: azul muy oscuro. |
| `v0.30.0_sky_twilight.png` | Hora azul (crepusculo nautico): violeta profundo. |
| `sky_preview.png` | Preview offline (`examples/sky_preview.rs`): tira de 24 h + hemisferio. |
| `v0.30.1_fog_directional.png` | Atardecer con niebla direccional (color de horizonte por azimut). |
| `v0.30.1_fog_horizonte.png` | Amanecer: sin costura entre cielo y terreno lejano. |
| `v0.31.0_sun_cube.png` | Sol como cubo 3D (se ve la cara superior y el giro). |
| `v0.31.0_moon_cube.png` | Luna cubo 3D con fase y campo de estrellas. |
| `v0.31.0_stars.png` | Cielo nocturno con estrellas y luna. |
| `v0.31.1_belt_of_venus.png` | Cinturon de Venus (banda rosa anti-sol) y hora azul. |
| `v0.31.1_halo.png` | Halo solar con funcion de fase de Henyey-Greenstein. |

## Distancia de vista y niebla (`v0.33`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.33.0_view_r12.png` | Radio de render 12 (defecto): vista larga, niebla al horizonte. |
| `v0.33.0_view_r16.png` | Radio 16: mas detalle lejano (~871 MB, por encima del presupuesto). |
| `v0.33.0_fog_short.png` | Modo de niebla `short`. |

## UI creativa (`v0.40`/`v0.41`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.40.1_toast.png` | Nombre del bloque ("PIEDRA") sobre la hotbar al cambiar de ranura. |
| `v0.41.0_inventory.png` | Inventario creativo con pestanas por categoria. |
| `v0.41.0_search.png` | Busqueda "tierra" (insensible a mayusculas/tildes). |

## Astros texturizados (`v0.35`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.35.0_sun_texture.png` | Sol como disco texturizado (arte de LibreSprite) + halo HG. |
| `v0.35.0_moon_texture.png` | Luna texturizada con fase (tira de 8) sobre el campo de estrellas. |

## Mundos multiples y pantallas (`v0.42`)

| Captura | Que muestra |
| ------- | ----------- |
| `v0.42.0_title.png` | Pantalla de titulo (logo + botones). |
| `v0.42.0_worlds.png` | Selector de mundos (lista + acciones). |
| `v0.42.0_create.png` | Crear mundo (nombre + semilla). |
| `v0.42.0_pause.png` | Menu de pausa (Esc). |
| `v0.43.1_title.png` | Titulo con boton "Opciones". |
| `v0.43.1_options.png` | Menu de opciones (distancia, niebla, FOV, idioma...). |
| `v0.43.2_controls.png` | Pantalla de Controles (reasignar teclas, con intercambio de conflictos). |
| `v0.43.3_options_acentos.png` | Fuente con tildes: 'SIMULACION' con acento. |
| `v0.43.3_controls_acentos.png` | Fuente con tildes: 'ATRAS' con acento. |
