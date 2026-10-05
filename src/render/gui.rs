//! Textura de la **interfaz** (hotbar e inventario).
//!
//! El estilo sigue la referencia del usuario (opcion D, colores medidos del
//! PNG): un marco de **madera** (`#4E351E`) con ranuras **hundidas** de tonos
//! marrones (`#1C0B02/#2B190C/#352011/#311C0F`).
//!
//! La textura se **pinta en LibreSprite** (`assets/gui.png`, 256x160) y se carga
//! con [`load_pixels`]; el procedural de [`build_pixels`] queda como fallback
//! sin assets (mismos tonos, mismo layout de regiones).

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

/// Ruta de la textura de interfaz en disco (pintada en LibreSprite).
pub const GUI_PATH: &str = "assets/gui.png";

/// Carga `assets/gui.png` (RGBA8 256x160). Si no existe o no encaja, devuelve
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

    // Flecha de crafteo.
    draw_arrow(&mut px);

    // Atenuador de fondo.
    for y in 0..DIM.h {
        for x in 0..DIM.w {
            put(&mut px, DIM.x + x, DIM.y + y, [0, 0, 0, 130]);
        }
    }
    px
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
///
/// Replica la referencia D del usuario (medida del PNG): borde exterior
/// `#1C0B02`, anillos concentricos `#2B190C` / `#352011` y centro `#311C0F`.
fn draw_slot(px: &mut [u8], ox: u32, oy: u32) {
    for y in 0..SLOT {
        for x in 0..SLOT {
            let d = x.min(y).min(SLOT - 1 - x).min(SLOT - 1 - y);
            let c = match d {
                0 => [28, 11, 2, 255],  // borde exterior oscuro
                1 => [43, 25, 12, 255], // anillo 1
                2 => [53, 32, 17, 255], // anillo 2
                3 => [49, 28, 15, 255], // centro
                _ => [53, 32, 17, 255], // interior: eco del anillo 2
            };
            put(px, ox + x, oy + y, c);
        }
    }
}

/// Marco de madera alrededor de la barra de la hotbar.
///
/// Madera de la referencia D: marco `#4E351E`, divisores `#593E23`, contorno
/// oscuro `#1C0B02`.
fn draw_hotbar_frame(px: &mut [u8]) {
    let wood = [78, 53, 30, 255];
    let trim = [28, 11, 2, 255];
    for x in 0..HOTBAR.w {
        put(px, HOTBAR.x + x, HOTBAR.y, trim); // fila superior
        put(px, HOTBAR.x + x, HOTBAR.y + HOTBAR.h - 1, trim); // inferior
    }
    for y in 0..HOTBAR.h {
        put(px, HOTBAR.x, HOTBAR.y + y, wood);
        put(px, HOTBAR.x + HOTBAR.w - 1, HOTBAR.y + y, wood);
    }
}

/// Fondo del panel de inventario (marco + interior). Misma madera que la
/// hotbar (referencia D): marco `#4E351E`, contorno `#1C0B02`.
fn draw_panel(px: &mut [u8]) {
    let wood = [78, 53, 30, 255];
    let dark = [28, 11, 2, 255];
    let inner = [43, 25, 12, 255];
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

    #[test]
    fn el_png_cargado_coincide_con_el_layout() {
        // Si existe assets/gui.png debe medir GUI_W x GUI_H; si no existe, el
        // fallback procedural sigue valiendo (test de tamanos).
        if let Some(px) = crate::world::atlas::load_png_rgba(GUI_PATH, GUI_W, GUI_H) {
            assert_eq!(px.len(), (GUI_W * GUI_H * 4) as usize);
            // La primera ranura del PNG debe ser opaca (marco D).
            assert_eq!(px[3], 255);
        }
    }
}
