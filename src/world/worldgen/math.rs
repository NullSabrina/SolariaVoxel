//! Helpers matematicos de worldgen (FASE 1 de la auditoria de generacion).
//!
//! Centraliza `lerp`/`smoothstep`/`remap` para no duplicar formulas en cada
//! etapa. Todo trabaja en `f32`: los campos locales de una columna no necesitan
//! `f64` y `f32` reduce memoria y mejora el uso de cache (el `noise` devuelve
//! `f64` y se convierte en la frontera).

/// Interpolacion lineal.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Recorta a `[0, 1]`.
#[inline]
pub fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// `smoothstep` clasico (`edge0` -> 0, `edge1` -> 1) con salida suave.
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    // El epsilon evita division por cero si los bordes coinciden.
    let t = saturate((x - edge0) / (edge1 - edge0 + f32::EPSILON));
    t * t * (3.0 - 2.0 * t)
}

/// `smootherstep` (derivada segunda tambien suave); util para pendientes.
#[inline]
pub fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = saturate((x - edge0) / (edge1 - edge0 + f32::EPSILON));
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Interpolacion inversa: valor de `x` en `[a, b]` llevado a `[0, 1]` (sin
/// recortar). `inverse_lerp(a, b, a) = 0`, `inverse_lerp(a, b, b) = 1`.
#[inline]
pub fn inverse_lerp(a: f32, b: f32, x: f32) -> f32 {
    (x - a) / (b - a + f32::EPSILON)
}

/// Recoloca `x` del rango `[a, b]` al rango `[c, d]` (sin recortar).
#[inline]
pub fn remap(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    lerp(c, d, inverse_lerp(a, b, x))
}

/// Como [`remap`] pero recortando el parametro a `[0, 1]`.
#[inline]
pub fn remap_clamped(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    lerp(c, d, saturate(inverse_lerp(a, b, x)))
}

/// Perfil de **terrazas/mesetas** (FASE 4): cuantiza `h` a escalones de `step`,
/// con la transicion alrededor del medio controlada por `sharpness` (menor =
/// risers mas verticales, treads mas planos). `sharpness` en `(0, 0.5)`.
#[inline]
pub fn terrace(h: f32, step: f32, sharpness: f32) -> f32 {
    if step <= 0.0 {
        return h;
    }
    let q = h / step;
    let base = q.floor();
    let frac = q - base;
    let s = smoothstep(0.5 - sharpness, 0.5 + sharpness, frac);
    (base + s) * step
}

/// Interpola una **curva** (spline) definida por puntos `(x, y)` ordenados por
/// `x` y estrictamente creciente. Fuera del rango, extrapola las pendientes de
/// los extremos. Es la herramienta para relaciones `continentalness -> altura`
/// sin cascadas de `if`.
pub fn spline(points: &[(f32, f32)], x: f32) -> f32 {
    debug_assert!(points.len() >= 2, "una spline necesita >= 2 puntos");
    if x <= points[0].0 {
        // Pendiente del primer tramo.
        let (x0, y0) = points[0];
        let (x1, y1) = points[1];
        return y0 + (x - x0) * (y1 - y0) / (x1 - x0 + f32::EPSILON);
    }
    let last = points.len() - 1;
    if x >= points[last].0 {
        let (x0, y0) = points[last - 1];
        let (x1, y1) = points[last];
        return y1 + (x - x1) * (y1 - y0) / (x1 - x0 + f32::EPSILON);
    }
    for window in points.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if x <= x1 {
            // Suavizado en el tramo para evitar esquinas duras.
            let t = smoothstep(x0, x1, x);
            return lerp(y0, y1, t);
        }
    }
    points[last].1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_y_saturate_estan_acotados() {
        assert_eq!(saturate(-3.0), 0.0);
        assert_eq!(saturate(4.0), 1.0);
        assert_eq!(smoothstep(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 2.0), 1.0);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn remap_y_inverse_lerp_son_inversas() {
        assert!((inverse_lerp(10.0, 20.0, 15.0) - 0.5).abs() < 1e-6);
        assert!((remap(15.0, 10.0, 20.0, 0.0, 100.0) - 50.0).abs() < 1e-4);
        // remap_clamped recorta fuera de rango.
        assert_eq!(remap_clamped(30.0, 10.0, 20.0, 0.0, 100.0), 100.0);
    }

    #[test]
    fn la_terraza_crea_escalones_planos_y_es_monotona() {
        let step = 4.0;
        // Treads: lejos del medio caen exactamente en multiplos de `step`.
        assert!((terrace(4.1, step, 0.1) - 4.0).abs() < 1e-4);
        assert!((terrace(7.9, step, 0.1) - 8.0).abs() < 1e-4);
        // El medio del riser da el punto medio.
        assert!((terrace(6.0, step, 0.1) - 6.0).abs() < 1e-4);
        // Monotona no decreciente.
        let mut prev = f32::NEG_INFINITY;
        for i in 0..400 {
            let h = i as f32 * 0.25;
            let t = terrace(h, step, 0.1);
            assert!(t >= prev - 1e-4, "no monotona en h={h}");
            prev = t;
        }
    }

    #[test]
    fn la_spline_interpola_y_extrapola() {
        let pts = [(-1.0, -70.0), (0.0, 0.0), (1.0, 90.0)];
        assert!((spline(&pts, 0.0) - 0.0).abs() < 1e-4);
        assert!((spline(&pts, 1.0) - 90.0).abs() < 1e-4);
        // Monotona y suave; en -1 y 1 exactos.
        assert!(spline(&pts, -2.0) < spline(&pts, -1.0));
        assert!(spline(&pts, 2.0) > spline(&pts, 1.0));
    }
}
