//! **Spline monotona por tramos** (Fritsch-Carlson) para el generador Larion.
//!
//! Es la herramienta con la que se convierte un campo normalizado en una
//! relacion del mundo (continentalidad -> altura, erosion -> ganancia de
//! detalle, ...). Propiedades que se exigen:
//!
//! * **Monotona:** en un tramo creciente, la salida nunca decrece (no hay
//!   sobreoscilacion entre nodos, el defecto clasico de una Hermite con
//!   pendientes "bonitas").
//! * **Exacta en los nodos:** `eval(x_i) == y_i` sin error acumulado.
//! * **Extrapolacion constante** fuera del rango: una spline de altura nunca
//!   crece sin limite por debajo/encima de sus puntos de control.
//! * **Sin asignaciones en la evaluacion:** la busqueda del tramo es binaria.
//!
//! La validacion ocurre en `new` (nunca en `eval`): un conjunto de puntos con
//! abscisas no crecientes o valores no finitos devuelve `Err`.

use std::fmt;

/// Error de construccion de una [`Spline`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplineError {
    /// Menos de dos puntos de control.
    TooFewPoints,
    /// Las abscisas no son estrictamente crecientes.
    NotIncreasing,
    /// Alguna coordenada no es finita (`NaN`/`inf`).
    NotFinite,
}

impl fmt::Display for SplineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SplineError::TooFewPoints => write!(f, "la spline necesita al menos 2 puntos"),
            SplineError::NotIncreasing => write!(f, "las abscisas deben ser estrictamente crecientes"),
            SplineError::NotFinite => write!(f, "las coordenadas de la spline deben ser finitas"),
        }
    }
}

impl std::error::Error for SplineError {}

/// Spline cubica de Hermite con pendientes Fritsch-Carlson.
///
/// Ver el modulo para las garantias. Es `Clone` para poder guardarse dentro de
/// una configuracion sin coste de reconstruccion.
#[derive(Clone, Debug, PartialEq)]
pub struct Spline {
    xs: Box<[f32]>,
    ys: Box<[f32]>,
    /// Pendientes Hermite precalculadas por nodo.
    ms: Box<[f32]>,
}

impl Spline {
    /// Construye la spline a partir de puntos `(x, y)` con `x` estrictamente
    /// creciente. Calcula las pendientes monotonas y las valida aqui, de modo
    /// que `eval` no puede fallar.
    pub fn new(points: &[(f32, f32)]) -> Result<Spline, SplineError> {
        if points.len() < 2 {
            return Err(SplineError::TooFewPoints);
        }
        let n = points.len();
        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        for (i, &(x, y)) in points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(SplineError::NotFinite);
            }
            if i > 0 && x <= xs[i - 1] {
                return Err(SplineError::NotIncreasing);
            }
            xs.push(x);
            ys.push(y);
        }

        // Secantes d_i = (y_{i+1} - y_i) / (x_{i+1} - x_i).
        let mut d = vec![0.0f32; n - 1];
        for i in 0..n - 1 {
            d[i] = (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]);
        }

        // Pendientes iniciales: en los extremos, la secante del tramo adyacente;
        // en el interior, la media (puede sobreoscila; se corrige abajo).
        let mut ms = vec![0.0f32; n];
        ms[0] = d[0];
        ms[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            ms[i] = if d[i - 1] * d[i] <= 0.0 {
                0.0
            } else {
                (d[i - 1] + d[i]) * 0.5
            };
        }

        // Filtro de monotonia (Fritsch-Carlson): limita la pendiente si el
        // circulo unitario de (alpha, beta) se sale del radio 3.
        for i in 0..n - 1 {
            if d[i] == 0.0 {
                ms[i] = 0.0;
                ms[i + 1] = 0.0;
                continue;
            }
            let a = ms[i] / d[i];
            let b = ms[i + 1] / d[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                ms[i] = t * a * d[i];
                ms[i + 1] = t * b * d[i];
            }
        }

        Ok(Spline {
            xs: xs.into_boxed_slice(),
            ys: ys.into_boxed_slice(),
            ms: ms.into_boxed_slice(),
        })
    }

    /// Numero de puntos de control.
    pub fn len(&self) -> usize {
        self.xs.len()
    }

    /// ¿Esta vacia? (nunca, pero pedido por clippy junto a `len`).
    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// Evalua la spline en `x`. Fuera del rango extrapola **constante** (el
    /// valor del extremo), nunca lineal.
    #[inline]
    pub fn eval(&self, x: f32) -> f32 {
        let xs = &self.xs;
        if x <= xs[0] {
            return self.ys[0];
        }
        let last = xs.len() - 1;
        if x >= xs[last] {
            return self.ys[last];
        }
        // Primer indice cuyo abscisa ya supera `x`; el tramo es `i..i+1`.
        let i = xs.partition_point(|&v| v <= x) - 1;
        let h = xs[i + 1] - xs[i];
        let t = (x - xs[i]) / h;
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * self.ys[i] + h10 * h * self.ms[i] + h01 * self.ys[i + 1] + h11 * h * self.ms[i + 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spline_creciente() -> Spline {
        Spline::new(&[(-1.0, 10.0), (-0.2, 30.0), (0.0, 64.0), (0.5, 120.0), (1.0, 210.0)]).unwrap()
    }

    #[test]
    fn la_spline_es_monotona_y_no_oscila() {
        let s = spline_creciente();
        let mut prev = f32::NEG_INFINITY;
        let mut x = -1.0f32;
        while x <= 1.0 {
            let y = s.eval(x);
            assert!(y >= prev - 1e-3, "no monotona en x={x}: {prev} -> {y}");
            prev = y;
            x += 0.002;
        }
    }

    #[test]
    fn la_spline_es_exacta_en_los_nodos() {
        let s = spline_creciente();
        for &(x, y) in &[(-1.0, 10.0), (-0.2, 30.0), (0.0, 64.0), (0.5, 120.0), (1.0, 210.0)] {
            assert!((s.eval(x) - y).abs() < 1e-4, "nodo {x}: {}", s.eval(x));
        }
    }

    #[test]
    fn la_spline_extrapola_constante() {
        let s = spline_creciente();
        assert_eq!(s.eval(-50.0), 10.0);
        assert_eq!(s.eval(50.0), 210.0);
        // En el tramo decreciente tambien se mantiene el extremo.
        let d = Spline::new(&[(0.0, 100.0), (1.0, 0.0)]).unwrap();
        assert_eq!(d.eval(-1.0), 100.0);
        assert_eq!(d.eval(2.0), 0.0);
    }

    #[test]
    fn la_construccion_es_determinista() {
        let a = spline_creciente();
        let b = spline_creciente();
        assert_eq!(a, b);
        for i in 0..100 {
            let x = i as f32 * 0.02 - 1.0;
            assert_eq!(a.eval(x), b.eval(x));
        }
    }

    #[test]
    fn la_validacion_detecta_puntos_absurdos() {
        assert_eq!(Spline::new(&[(0.0, 1.0)]), Err(SplineError::TooFewPoints));
        assert_eq!(
            Spline::new(&[(0.0, 1.0), (0.0, 2.0)]),
            Err(SplineError::NotIncreasing)
        );
        assert_eq!(
            Spline::new(&[(0.0, 1.0), (-1.0, 2.0)]),
            Err(SplineError::NotIncreasing)
        );
        assert_eq!(
            Spline::new(&[(0.0, 1.0), (f32::NAN, 2.0)]),
            Err(SplineError::NotFinite)
        );
    }
}
