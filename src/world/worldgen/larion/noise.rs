//! **Pila de ruido** del generador Larion: campos 2D por escala, crestas
//! multifractales, ruido 3D para el detalle vertical y domain warping.
//!
//! Todo ruido es `noise` gradiente (`OpenSimplex`), no ruido de valor: no tiene
//! los artefactos de cuadricula/terrazas del `Value2D` que usaba el camino
//! `graph`. Cada campo se identifica por una **semilla derivada** de la del
//! mundo, de modo que dos campos nunca comparten patron.
//!
//! El trait [`ScalarField2D`] aisla la implementacion: sustituir `noise` por un
//! ruido propio (p. ej. con SIMD o FastNoise2) no obliga a tocar la logica de
//! altura ni los tests.

use noise::{Fbm, MultiFractal, NoiseFn, OpenSimplex, RidgedMulti};

/// Campo escalar 2D en `[-1, 1]` (el ruido devuelve aproximadamente ese rango).
///
/// Es un trait para poder instrumentar (tests de coste) o cambiar de motor de
/// ruido sin tocar la composicion de altura.
pub trait ScalarField2D: Send + Sync {
    /// Muestrea el campo en `(x, z)` (coordenadas de mundo en bloques).
    fn sample(&self, x: f64, z: f64) -> f64;
}

/// Adaptador fractal gradiente (`Fbm<OpenSimplex>`): colinas y detalle a varias
/// octavas. La frecuencia va en **1/bloques**.
#[derive(Clone)]
pub struct Fractal2D {
    inner: Fbm<OpenSimplex>,
}

impl Fractal2D {
    /// Crea un ruido fractal con `octaves` octavas y frecuencia `frequency`
    /// (1/bloques). Persistencia 0.5 y lacunaridad 2.0 (valores clasicos de fBm).
    pub fn new(seed: u32, octaves: usize, frequency: f64) -> Self {
        Self {
            inner: Fbm::<OpenSimplex>::new(seed)
                .set_octaves(octaves.max(1))
                .set_frequency(frequency)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
        }
    }
}

impl ScalarField2D for Fractal2D {
    #[inline]
    fn sample(&self, x: f64, z: f64) -> f64 {
        self.inner.get([x, z])
    }
}

/// Crestas multifractales (`RidgedMulti<OpenSimplex>`): lineas de cresta con
/// detalle a varias escalas. Salida aproximada en `[-1, 1]`; quien lo usa la
/// normaliza a `[0, 1]`. La atenuacion por octava (2.0 por defecto) hace que las
/// crestas grandes dominen y las pequenas las matizen.
#[derive(Clone)]
pub struct Ridged2D {
    inner: RidgedMulti<OpenSimplex>,
}

impl Ridged2D {
    /// Crea un ruido de crestas con `octaves` octavas y frecuencia `frequency`.
    pub fn new(seed: u32, octaves: usize, frequency: f64) -> Self {
        Self {
            inner: RidgedMulti::<OpenSimplex>::new(seed)
                .set_octaves(octaves.max(1))
                .set_frequency(frequency)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
        }
    }
}

impl ScalarField2D for Ridged2D {
    #[inline]
    fn sample(&self, x: f64, z: f64) -> f64 {
        self.inner.get([x, z])
    }
}

/// Ruido fractal 3D para el detalle vertical (voladizos y roca en banda). La
/// frecuencia va en 1/bloques y se aplica por igual en los tres ejes.
#[derive(Clone)]
pub struct Fractal3D {
    inner: Fbm<OpenSimplex>,
}

impl Fractal3D {
    /// Crea el ruido 3D con `octaves` octavas y frecuencia `frequency`.
    pub fn new(seed: u32, octaves: usize, frequency: f64) -> Self {
        Self {
            inner: Fbm::<OpenSimplex>::new(seed)
                .set_octaves(octaves.max(1))
                .set_frequency(frequency)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
        }
    }

    /// Muestrea el campo 3D en `[-1, 1]`.
    #[inline]
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.inner.get([x, y, z])
    }
}

/// Domain warping **horizontal** (solo X y Z): deforma las coordenadas para
/// romper los contornos circulares del ruido isotropo. Es el principio central
/// que describe Larion ("deforma solo los ejes horizontales").
#[derive(Clone)]
pub struct Warp2D {
    x: Fractal2D,
    z: Fractal2D,
    /// Desplazamiento maximo en bloques.
    strength: f64,
}

impl Warp2D {
    /// Crea un warp con dos campos independientes (uno por eje) a la frecuencia
    /// y fuerza dadas.
    pub fn new(seed: u32, frequency: f64, strength: f64) -> Self {
        Self {
            x: Fractal2D::new(seed, 2, frequency),
            z: Fractal2D::new(seed ^ 0x5bd1_e995, 2, frequency),
            strength,
        }
    }

    /// Devuelve las coordenadas deformadas `(x + wx, z + wz)`. Nunca toca Y.
    #[inline]
    pub fn apply(&self, x: f64, z: f64) -> (f64, f64) {
        let wx = self.x.sample(x, z) * self.strength;
        let wz = self.z.sample(x, z) * self.strength;
        (x + wx, z + wz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_campos_son_deterministas() {
        let a = Fractal2D::new(7, 4, 0.001);
        let b = Fractal2D::new(7, 4, 0.001);
        for i in 0..200 {
            let x = i as f64 * 13.0 - 900.0;
            let z = i as f64 * -7.0 + 400.0;
            assert_eq!(a.sample(x, z), b.sample(x, z));
        }
    }

    #[test]
    fn semillas_distintas_dan_campos_distintos() {
        let a = Fractal2D::new(1, 3, 0.002);
        let b = Fractal2D::new(2, 3, 0.002);
        let mut distintos = 0;
        for i in 0..50 {
            let x = i as f64 * 37.0;
            if (a.sample(x, x) - b.sample(x, x)).abs() > 1e-6 {
                distintos += 1;
            }
        }
        assert!(distintos > 40, "los campos apenas difieren");
    }

    #[test]
    fn los_campos_estan_acotados() {
        let f = Fractal2D::new(9, 4, 0.0015);
        let r = Ridged2D::new(9, 5, 0.0025);
        for i in 0..500 {
            let x = i as f64 * 91.0 - 20_000.0;
            let z = i as f64 * -53.0 + 12_000.0;
            assert!((-1.2..=1.2).contains(&f.sample(x, z)));
            assert!((-1.2..=1.2).contains(&r.sample(x, z)));
        }
    }

    #[test]
    fn el_warp_solo_desplaza_en_horizontal() {
        let w = Warp2D::new(3, 0.001, 400.0);
        let (x, z) = w.apply(1000.0, -2000.0);
        assert!((x - 1000.0).abs() <= 400.0 + 1e-6);
        assert!((z + 2000.0).abs() <= 400.0 + 1e-6);
        // Determinista.
        assert_eq!(w.apply(1000.0, -2000.0), (x, z));
    }

    #[test]
    fn el_ruido_3d_es_determinista_y_acotado() {
        let n = Fractal3D::new(11, 2, 0.01);
        for i in 0..200 {
            let (x, y, z) = (i as f64 * 7.0, i as f64 * 3.0, i as f64 * -5.0);
            let v = n.sample(x, y, z);
            assert_eq!(v, n.sample(x, y, z));
            assert!((-1.2..=1.2).contains(&v));
        }
    }
}
