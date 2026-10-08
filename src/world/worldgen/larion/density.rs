//! **Densidad 3D en banda vertical** (seccion 4.3): voladizos y roca solo cerca
//! de la superficie.
//!
//! En vez de evaluar un campo 3D en todo el volumen (caro y con la resolucion
//! gruesa del camino `graph`), aqui se evalua **solo** en `|y - H| < band`. Por
//! fuera de la banda el signo es trivial (solido por debajo, aire por encima).
//! Dentro, un ruido 3D desplaza la superficie y permite voladizos reales. La
//! amplitud se apaga en los bordes de la banda para no dejar escalones.

use super::noise::Fractal3D;

/// Campo de densidad 3D en banda, determinista por semilla.
pub struct DensityField {
    noise: Fractal3D,
    band: f32,
    amplitude: f32,
}

impl DensityField {
    /// Crea el campo con el semi-ancho de banda, la frecuencia y las octavas del
    /// ruido 3D, y la amplitud del desplazamiento vertical.
    pub fn new(seed: u32, band: f32, frequency: f64, octaves: usize, amplitude: f32) -> Self {
        Self {
            noise: Fractal3D::new(seed, octaves, frequency),
            band: band.max(1.0),
            amplitude: amplitude.max(0.0),
        }
    }

    /// Semi-ancho de la banda (bloques).
    pub fn band(&self) -> f32 {
        self.band
    }

    /// Densidad en `(x, y, z)` para una altura de superficie `H`. `overhang` en
    /// `[0, 1]` escala cuanto puede desplazarse la superficie (0 = liso, 1 =
    /// montana joven). Positivo = solido.
    #[inline]
    pub fn density(&self, x: i32, y: i32, z: i32, h: f32, overhang: f32) -> f32 {
        let d = h - y as f32;
        if d > self.band {
            return 1.0;
        }
        if d < -self.band {
            return -1.0;
        }
        let fade = 1.0 - d.abs() / self.band;
        let n = self.noise.sample(x as f64, y as f64, z as f64) as f32;
        d + n * self.amplitude * overhang.clamp(0.0, 1.0) * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuera_de_la_banda_el_signo_es_trivial() {
        let f = DensityField::new(1, 16.0, 1.0 / 90.0, 2, 13.0);
        // Muy por encima de H -> aire.
        assert!(f.density(10, 200, 10, 80.0, 1.0) < 0.0);
        // Muy por debajo -> solido.
        assert!(f.density(10, 20, 10, 80.0, 1.0) > 0.0);
    }

    #[test]
    fn sin_voladizos_la_superficie_es_lisa() {
        let f = DensityField::new(2, 16.0, 1.0 / 90.0, 2, 13.0);
        for y in 60..=100 {
            let d = f.density(3, y, 7, 80.0, 0.0);
            assert_eq!(d.signum(), (80.0 - y as f32).signum());
        }
    }

    #[test]
    fn con_voladizos_el_ruido_desplaza_la_superficie() {
        let f = DensityField::new(3, 20.0, 1.0 / 60.0, 2, 14.0);
        let mut distintos = 0;
        for x in 0..40 {
            for z in 0..40 {
                if (f.density(x, 80, z, 80.0, 0.0) - f.density(x, 80, z, 80.0, 1.0)).abs() > 1e-4
                {
                    distintos += 1;
                }
            }
        }
        assert!(distintos > 400, "el ruido 3D apenas actua: {distintos}");
    }

    #[test]
    fn es_determinista() {
        let a = DensityField::new(5, 16.0, 1.0 / 80.0, 2, 12.0);
        let b = DensityField::new(5, 16.0, 1.0 / 80.0, 2, 12.0);
        for y in 60..90 {
            assert_eq!(
                a.density(1, y, 2, 75.0, 0.8),
                b.density(1, y, 2, 75.0, 0.8)
            );
        }
    }
}
