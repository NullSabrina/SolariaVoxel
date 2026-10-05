//! Textura de la **interfaz** (hotbar e inventario), generada por codigo.
//!
//! El estilo sigue la referencia del usuario ("Rappenem's Reforge", opcion D):
//! un marco de **madera** con ranuras **hundidas** de tonos marrones. Generarlo
//! por codigo (como el atlas procedural) mantiene todo bajo control de versiones
//! y reproducible, sin un PNG binario aparte.

/// Ancho de la textura de la interfaz.
pub const GUI_W: u32 = 256;
/// Alto de la textura de la interfaz.
pub const GUI_H: u32 = 160;

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
/// Fondo del panel de inventario.
pub const PANEL: Region = Region {
    x: 0,
    y: 48,
    w: 200,
    h: 76,
};
/// Resalte de la ranura seleccionada (marco claro + tinte translucido).
pub const SELECTION: Region = Region {
    x: 0,
    y: 126,
    w: SLOT,
    h: SLOT,
};

/// Genera la imagen RGBA de la interfaz (sRGB).
pub fn build_pixels() -> Vec<u8> {
    let mut px = vec![0u8; (GUI_W * GUI_H * 4) as usize];

    // 9 ranuras en fila, con un marco de madera rodeandolas.
    for i in 0..9 {
        draw_slot(&mut px, HOTBAR.x + 1 + i * SLOT, HOTBAR.y + 1);
    }
    draw_hotbar_frame(&mut px);

    // Ranura suelta.
    draw_slot(&mut px, SLOT_REGION.x, SLOT_REGION.y);

    // Panel del inventario: marco + interior.
    draw_panel(&mut px);

    // Resalte de la ranura seleccionada.
    draw_selection(&mut px);
    px
}

/// Resalte: tinte blanco translucido con un marco claro.
fn draw_selection(px: &mut [u8]) {
    for y in 0..SLOT {
        for x in 0..SLOT {
            let edge = x == 0 || y == 0 || x == SLOT - 1 || y == SLOT - 1;
            let c = if edge {
                [255, 245, 200, 200]
            } else {
                [255, 245, 200, 70]
            };
            put(px, SELECTION.x + x, SELECTION.y + y, c);
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

/// Dibuja una ranura hundida de `SLOT`x`SLOT` en `(ox, oy)`.
fn draw_slot(px: &mut [u8], ox: u32, oy: u32) {
    for y in 0..SLOT {
        for x in 0..SLOT {
            let d = x.min(y).min(SLOT - 1 - x).min(SLOT - 1 - y);
            let c = match d {
                0 => [26, 18, 12, 255], // borde exterior oscuro
                1 => [96, 70, 42, 255], // marco de madera claro
                _ => {
                    // Interior: cuadrado hundido con un contorno mas claro.
                    let e = x.min(y).min(SLOT - 1 - x).min(SLOT - 1 - y);
                    if e == 4 {
                        [78, 58, 36, 255]
                    } else if e < 4 {
                        [50, 38, 26, 255]
                    } else {
                        [42, 32, 22, 255]
                    }
                }
            };
            put(px, ox + x, oy + y, c);
        }
    }
}

/// Marco de madera alrededor de la barra de la hotbar.
fn draw_hotbar_frame(px: &mut [u8]) {
    let wood = [104, 76, 46, 255];
    let trim = [70, 50, 30, 255];
    for x in 0..HOTBAR.w {
        put(px, HOTBAR.x + x, HOTBAR.y, trim); // fila superior
        put(px, HOTBAR.x + x, HOTBAR.y + HOTBAR.h - 1, trim); // inferior
    }
    for y in 0..HOTBAR.h {
        put(px, HOTBAR.x, HOTBAR.y + y, wood);
        put(px, HOTBAR.x + HOTBAR.w - 1, HOTBAR.y + y, wood);
    }
}

/// Fondo del panel de inventario (marco + interior).
fn draw_panel(px: &mut [u8]) {
    let wood = [110, 80, 48, 255];
    let dark = [30, 22, 14, 255];
    let inner = [58, 46, 32, 255];
    for y in 0..PANEL.h {
        for x in 0..PANEL.w {
            let edge = x == 0 || y == 0 || x == PANEL.w - 1 || y == PANEL.h - 1;
            let near = x == 1 || y == 1 || x == PANEL.w - 2 || y == PANEL.h - 2;
            let c = if edge {
                dark
            } else if near {
                wood
            } else {
                inner
            };
            put(px, PANEL.x + x, PANEL.y + y, c);
        }
    }
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
}
