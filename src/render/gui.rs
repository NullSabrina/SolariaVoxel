//! Textura de la **interfaz** (hotbar, inventario, botones, mirilla).
//!
//! Estilo Minecraft: **bisel** de 1 px (luz arriba-izquierda, sombra
//! abajo-derecha), ranuras hundidas y botones con 4 estados (normal/hover/
//! pulsado/desactivado). El arte se **pinta en LibreSprite** (`assets/gui.png`,
//! 256x256) con el script `tools/gen_gui.js`; el manifiesto de regiones vive en
//! `assets/gui.json`. El procedural de [`build_pixels`] es el fallback sin assets.

/// Ancho de la textura de la interfaz.
pub const GUI_W: u32 = 256;
/// Alto de la textura de la interfaz.
pub const GUI_H: u32 = 256;

/// Lado de una ranura, en pixels.
pub const SLOT: u32 = 20;

/// Region rectangular dentro de la textura de interfaz.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Barra de la hotbar (9 ranuras + marco).
pub const HOTBAR: Region = Region {
    x: 0,
    y: 0,
    w: 9 * SLOT + 2,
    h: SLOT + 2,
};
/// Una ranura suelta (para el inventario).
pub const SLOT_REGION: Region = Region {
    x: 0,
    y: 24,
    w: SLOT,
    h: SLOT,
};
/// Ranura bajo el cursor (hover).
pub const SLOT_HOVER: Region = Region {
    x: 20,
    y: 24,
    w: SLOT,
    h: SLOT,
};
/// Resalte de la ranura seleccionada.
pub const SELECTION: Region = Region {
    x: 40,
    y: 24,
    w: SLOT,
    h: SLOT,
};
/// Fondo del panel de inventario.
pub const PANEL: Region = Region {
    x: 0,
    y: 48,
    w: 200,
    h: 76,
};
/// Boton en estado normal.
pub const BUTTON: Region = Region {
    x: 0,
    y: 128,
    w: 200,
    h: 20,
};
/// Boton bajo el cursor.
pub const BUTTON_HOVER: Region = Region {
    x: 0,
    y: 150,
    w: 200,
    h: 20,
};
/// Boton pulsado.
pub const BUTTON_PRESSED: Region = Region {
    x: 0,
    y: 172,
    w: 200,
    h: 20,
};
/// Boton desactivado.
pub const BUTTON_DISABLED: Region = Region {
    x: 0,
    y: 194,
    w: 200,
    h: 20,
};
/// Flecha de crafteo (rejilla -> resultado), estilo pergamino.
pub const ARROW: Region = Region {
    x: 204,
    y: 64,
    w: 32,
    h: 16,
};
/// Pixel oscuro translucido para atenuar el mundo con una ventana abierta.
pub const DIM: Region = Region {
    x: 244,
    y: 64,
    w: 8,
    h: 8,
};
/// Mirilla (crosshair).
pub const CROSSHAIR: Region = Region {
    x: 204,
    y: 90,
    w: 15,
    h: 15,
};
/// Pestana de categoria del inventario.
pub const TAB: Region = Region {
    x: 204,
    y: 112,
    w: 40,
    h: 14,
};

/// Ruta de la textura de interfaz en disco (pintada en LibreSprite).
pub const GUI_PATH: &str = "assets/gui.png";

/// Carga `assets/gui.png` (RGBA8 256x256). Si no existe o no encaja, devuelve
/// el procedural de [`build_pixels`].
pub fn load_pixels() -> Vec<u8> {
    match crate::world::atlas::load_png_rgba(GUI_PATH, GUI_W, GUI_H) {
        Some(p) => {
            println!("[gui] cargado {GUI_PATH} ({GUI_W}x{GUI_H})");
            p
        }
        None => {
            println!("[gui] sin {GUI_PATH}; uso la interfaz procedural");
            build_pixels()
        }
    }
}

/// Genera la imagen RGBA de la interfaz (sRGB).
pub fn build_pixels() -> Vec<u8> {
    let mut px = vec![0u8; (GUI_W * GUI_H * 4) as usize];

    // Hotbar: marco + 9 ranuras.
    bevel(&mut px, HOTBAR.x, HOTBAR.y, HOTBAR.w, HOTBAR.h, WOOD, BEVEL_LIGHT, DARK, None);
    for i in 0..9 {
        let sx = HOTBAR.x + 1 + i * SLOT;
        bevel(&mut px, sx, HOTBAR.y + 1, SLOT, SLOT, INNER, DARK, WOOD, Some(DARK));
    }
    // Ranuras sueltas: normal, hover, seleccionada.
    bevel(&mut px, SLOT_REGION.x, SLOT_REGION.y, SLOT, SLOT, INNER, DARK, WOOD, Some(DARK));
    bevel(&mut px, SLOT_HOVER.x, SLOT_HOVER.y, SLOT, SLOT, SLOT_HOVER_FILL, DARK, WOOD, Some(DARK));
    bevel(&mut px, SELECTION.x, SELECTION.y, SLOT, SLOT, SELECTION_FILL, [255, 248, 210, 255], [150, 130, 80, 255], Some(DARK));
    // Panel de inventario.
    bevel(&mut px, PANEL.x, PANEL.y, PANEL.w, PANEL.h, INNER, WOOD, DARK, Some(DARK));
    // Botones: normal, hover, pulsado, desactivado.
    let btn = [
        (BUTTON, [110, 110, 110, 255], [168, 168, 168, 255], [52, 52, 52, 255]),
        (BUTTON_HOVER, [126, 126, 150, 255], [190, 190, 210, 255], [60, 60, 74, 255]),
        (BUTTON_PRESSED, [80, 80, 80, 255], [52, 52, 52, 255], [150, 150, 150, 255]),
        (BUTTON_DISABLED, [64, 64, 64, 255], [96, 96, 96, 255], [44, 44, 44, 255]),
    ];
    for (r, base, light, dark) in btn {
        bevel(&mut px, r.x, r.y, r.w, r.h, base, light, dark, Some([30, 30, 30, 255]));
    }
    // Pestana.
    bevel(&mut px, TAB.x, TAB.y, TAB.w, TAB.h, [110, 110, 110, 255], [168, 168, 168, 255], [52, 52, 52, 255], Some([30, 30, 30, 255]));

    // Flecha de crafteo.
    draw_arrow(&mut px);
    // Mirilla.
    draw_crosshair(&mut px);
    // Atenuador de fondo.
    for y in 0..DIM.h {
        for x in 0..DIM.w {
            put(&mut px, DIM.x + x, DIM.y + y, [0, 0, 0, 130]);
        }
    }
    px
}

const DARK: [u8; 4] = [13, 6, 0, 255];
const INNER: [u8; 4] = [36, 18, 9, 255];
const WOOD: [u8; 4] = [78, 53, 30, 255];
const BEVEL_LIGHT: [u8; 4] = [96, 68, 40, 255];
const SLOT_HOVER_FILL: [u8; 4] = [58, 36, 18, 255];
const SELECTION_FILL: [u8; 4] = [240, 224, 160, 255];

/// Rellena un rectangulo con `base` y le aplica el **bisel** de Minecraft:
/// borde `light` arriba-izquierda, `dark` abajo-derecha y contorno `border`.
#[allow(clippy::too_many_arguments)]
fn bevel(
    px: &mut [u8],
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    base: [u8; 4],
    light: [u8; 4],
    dark: [u8; 4],
    border: Option<[u8; 4]>,
) {
    for y in 0..h {
        for x in 0..w {
            put(px, x0 + x, y0 + y, base);
        }
    }
    // Contorno exterior (1 px).
    if let Some(b) = border {
        for x in 0..w {
            put(px, x0 + x, y0, b);
            put(px, x0 + x, y0 + h - 1, b);
        }
        for y in 0..h {
            put(px, x0, y0 + y, b);
            put(px, x0 + w - 1, y0 + y, b);
        }
    }
    // Bisel interior (1 px): luz arriba-izquierda, sombra abajo-derecha.
    if w >= 3 && h >= 3 {
        for x in 1..w - 1 {
            put(px, x0 + x, y0 + 1, light);
            put(px, x0 + x, y0 + h - 2, dark);
        }
        for y in 1..h - 1 {
            put(px, x0 + 1, y0 + y, light);
            put(px, x0 + w - 2, y0 + y, dark);
        }
    }
}

/// Mirilla blanca con contorno oscuro (alto contraste sobre cualquier fondo).
fn draw_crosshair(px: &mut [u8]) {
    let (ox, oy) = (CROSSHAIR.x, CROSSHAIR.y);
    let c = CROSSHAIR.w / 2;
    for i in 3..=11 {
        put(px, ox + c, oy + i, [0, 0, 0, 255]);
        put(px, ox + i, oy + c, [0, 0, 0, 255]);
    }
    for i in 4..=10 {
        put(px, ox + c, oy + i, [255, 255, 255, 255]);
        put(px, ox + i, oy + c, [255, 255, 255, 255]);
    }
}

/// Flecha de crafteo mirando a la derecha, en tonos pergamino sobre fondo
/// transparente (se dibuja con blending).
fn draw_arrow(px: &mut [u8]) {
    for y in 0..ARROW.h {
        for x in 0..ARROW.w {
            let (xi, yi) = (x as i32, y as i32);
            let in_body = (6..10).contains(&yi) && xi < 22;
            let in_head = xi >= 20 && (yi - 8).abs() <= (31 - xi) / 2 + 1;
            if !(in_body || in_head) {
                continue;
            }
            let edge = yi == 6 || yi == 9 || xi == 0;
            let c = if edge {
                [138, 115, 85, 255]
            } else {
                [216, 196, 154, 255]
            };
            put(px, ARROW.x + x, ARROW.y + y, c);
        }
    }
}

/// Escribe un pixel (ignora fuera de rango).
fn put(px: &mut [u8], x: u32, y: u32, c: [u8; 4]) {
    if x >= GUI_W || y >= GUI_H {
        return;
    }
    let i = ((y * GUI_W + x) * 4) as usize;
    px[i..i + 4].copy_from_slice(&c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_tamano_es_el_esperado() {
        let px = build_pixels();
        assert_eq!(px.len(), (GUI_W * GUI_H * 4) as usize);
    }

    #[test]
    fn la_primera_ranura_es_opaca_y_con_hundido() {
        let px = build_pixels();
        let at = |x: u32, y: u32| {
            let i = ((y * GUI_W + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        // El borde exterior de la ranura es oscuro y opaco.
        let e = at(SLOT_REGION.x, SLOT_REGION.y);
        assert_eq!(e[3], 255);
        assert!(e[0] < 60);
        // El centro es mas claro que el borde.
        let c = at(SLOT_REGION.x + 10, SLOT_REGION.y + 10);
        assert!(c[0] > e[0]);
    }

    #[test]
    fn los_botones_tienen_los_cuatro_estados() {
        // Cada estado vive en una region distinta y del tamano esperado.
        for r in [BUTTON, BUTTON_HOVER, BUTTON_PRESSED, BUTTON_DISABLED] {
            assert_eq!(r.w, 200);
            assert_eq!(r.h, 20);
        }
        assert_ne!(BUTTON.y, BUTTON_HOVER.y);
        assert_ne!(BUTTON_HOVER.y, BUTTON_PRESSED.y);
    }

    #[test]
    fn el_png_cargado_coincide_con_el_layout() {
        // Si existe assets/gui.png debe medir GUI_W x GUI_H; si no existe, el
        // fallback procedural sigue valiendo (test de tamanos).
        if let Some(px) = crate::world::atlas::load_png_rgba(GUI_PATH, GUI_W, GUI_H) {
            assert_eq!(px.len(), (GUI_W * GUI_H * 4) as usize);
            // La primera ranura del PNG debe ser opaca (marco).
            assert_eq!(px[3], 255);
        }
    }
}
