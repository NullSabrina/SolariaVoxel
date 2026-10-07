//! Color: sRGB <-> lineal y mezcla perceptual en OKLab.
//!
//! La GPU trabaja en espacio **lineal**, pero nosotros elegimos colores como en
//! un editor de imagenes: en **sRGB**. Ademas, mezclar dos colores en sRGB (o en
//! lineal) entre el naranja del atardecer y el azul del cenit da un **gris
//! sucio**; en **OKLab** (Bjorn Ottosson) el camino pasa por un naranja/rosa
//! creible. Por eso el cielo interpola sus keyframes en OKLab.
//!
//! Este modulo es puro y vive en `math` (no en `scene`) porque `render` tambien
//! necesita la conversion sRGB -> lineal para el color de clear, y `scene` no
//! puede depender de `render`.

/// Convierte un canal de color de sRGB a lineal (norma sRGB).
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convierte un canal de color de lineal a sRGB (norma sRGB).
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Convierte un color sRGB (0..1) a lineal, componente a componente.
pub fn srgb_to_linear3(c: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
    ]
}

/// Convierte un color lineal (0..1) a sRGB, componente a componente.
pub fn linear_to_srgb3(c: [f32; 3]) -> [f32; 3] {
    [
        linear_to_srgb(c[0]),
        linear_to_srgb(c[1]),
        linear_to_srgb(c[2]),
    ]
}

/// Color sRGB a partir de un literal `0xRRGGBB` (comodo para tablas de arte).
///
/// `const` para poder declarar la paleta del cielo como constante.
pub const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    ]
}

/// Luminancia perceptual aproximada (Rec. 709) de un color sRGB.
pub fn luminance_srgb(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Desatura un color sRGB hacia su gris de igual luminancia.
///
/// `amount = 0` no cambia nada; `amount = 1` lo deja gris. Se usa como control de
/// direccion de arte (`TWILIGHT_SATURATION` del crepusculo).
pub fn desaturate_srgb(c: [f32; 3], amount: f32) -> [f32; 3] {
    let amount = amount.clamp(0.0, 1.0);
    let g = luminance_srgb(c);
    [
        c[0] + (g - c[0]) * amount,
        c[1] + (g - c[1]) * amount,
        c[2] + (g - c[2]) * amount,
    ]
}

/// Convierte sRGB a OKLab (asume entrada ya en sRGB 0..1).
pub fn srgb_to_oklab(c: [f32; 3]) -> [f32; 3] {
    linear_srgb_to_oklab(srgb_to_linear3(c))
}

/// Convierte OKLab a sRGB.
pub fn oklab_to_srgb(lab: [f32; 3]) -> [f32; 3] {
    linear_to_srgb3(oklab_to_linear_srgb(lab))
}

/// sRGB **lineal** a OKLab (las matrices de Ottosson asumen espacio lineal).
pub fn linear_srgb_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

/// OKLab a sRGB **lineal**.
pub fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    let (l_, a_, b_) = (lab[0], lab[1], lab[2]);
    let l = l_ + 0.396_337_78 * a_ + 0.215_803_76 * b_;
    let m = l_ - 0.105_561_346 * a_ - 0.063_854_17 * b_;
    let s = l_ - 0.089_484_18 * a_ - 1.291_485_5 * b_;
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// Mezcla dos colores sRGB en OKLab (`t = 0` -> `a`, `t = 1` -> `b`).
///
/// Evita el "gris sucio" de mezclar naranja y azul en sRGB/lineal. Devuelve sRGB.
pub fn mix_srgb_oklab(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let la = srgb_to_oklab(a);
    let lb = srgb_to_oklab(b);
    let t = t.clamp(0.0, 1.0);
    let lab = [
        la[0] + (lb[0] - la[0]) * t,
        la[1] + (lb[1] - la[1]) * t,
        la[2] + (lb[2] - la[2]) * t,
    ];
    oklab_to_srgb(lab)
}

/// Suavizado de Hermite (`smoothstep`) sobre `x` en `[0, 1]`.
pub fn smoothstep01(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `smoothstep` con bordes; `edge0 > edge1` invierte el resultado.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smoothstep01(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < eps)
    }

    #[test]
    fn srgb_lineal_ida_y_vuelta() {
        for i in 0..=20 {
            let c = i as f32 / 20.0;
            let back = linear_to_srgb(srgb_to_linear(c));
            assert!((c - back).abs() < 1e-4, "c={c} back={back}");
        }
    }

    #[test]
    fn hex_a_srgb() {
        assert!(approx(rgb(0xFF0000), [1.0, 0.0, 0.0], 1e-6));
        assert!(approx(rgb(0x00FF00), [0.0, 1.0, 0.0], 1e-6));
        assert!(approx(rgb(0x0000FF), [0.0, 0.0, 1.0], 1e-6));
        assert!(approx(rgb(0xFFFFFF), [1.0, 1.0, 1.0], 1e-6));
    }

    #[test]
    fn oklab_ida_y_vuelta() {
        for c in [
            [0.5, 0.2, 0.1],
            [0.95, 0.52, 0.28],
            [0.18, 0.12, 0.45],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ] {
            let back = oklab_to_srgb(srgb_to_oklab(c));
            assert!(approx(back, c, 1e-3), "{c:?} -> {back:?}");
        }
    }

    #[test]
    fn mezcla_oklab_respeta_los_extremos() {
        let a = [0.95, 0.52, 0.28];
        let b = [0.18, 0.12, 0.45];
        assert!(approx(mix_srgb_oklab(a, b, 0.0), a, 1e-3));
        assert!(approx(mix_srgb_oklab(a, b, 1.0), b, 1e-3));
    }

    #[test]
    fn smoothstep_es_monotono_y_acotado() {
        assert!((smoothstep(0.0, 1.0, -1.0)).abs() < 1e-6);
        assert!((smoothstep(0.0, 1.0, 2.0) - 1.0).abs() < 1e-6);
        let mut prev = -1.0;
        for i in 0..=50 {
            let v = smoothstep(0.0, 1.0, i as f32 / 50.0);
            assert!(v >= prev - 1e-6);
            prev = v;
        }
    }

    #[test]
    fn desaturate_deja_el_gris_al_maximo() {
        let g = desaturate_srgb([0.9, 0.1, 0.1], 1.0);
        assert!((g[0] - g[1]).abs() < 1e-5 && (g[1] - g[2]).abs() < 1e-5);
    }
}
