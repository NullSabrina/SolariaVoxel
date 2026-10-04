//! Utilidades de color.
//!
//! La GPU trabaja en espacio **lineal**, pero nosotros elegimos colores como en
//! un editor de imagenes: en espacio **sRGB** (el que "se ve"). Si le pasamos
//! un sRGB directo a un objetivo sRGB, la GPU lo vuelve a aclarar y los colores
//! salen lavados. Por eso convertimos aqui de sRGB a lineal.

/// Convierte un canal de color de sRGB a lineal (norma sRGB).
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convierte un color RGB de sRGB a lineal, componente a componente.
pub fn srgb_rgb(rgb: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(rgb[0]),
        srgb_to_linear(rgb[1]),
        srgb_to_linear(rgb[2]),
    ]
}
