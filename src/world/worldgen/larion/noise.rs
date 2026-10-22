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

use quick_noise::simd::{SimdSliceIterExt, dispatch_simd};
use quick_noise::{BatchNoise, Fbm as SimdFbm, Perlin as SimdPerlin, Ridged as SimdRidged};

/// Campo escalar 2D en `[-1, 1]` (el ruido devuelve aproximadamente ese rango).
///
/// Es un trait para poder instrumentar (tests de coste) o cambiar de motor de
/// ruido sin tocar la composicion de altura. `sample_many` permite procesar un
/// lote de puntos con SIMD; el default conserva el comportamiento escalar.
pub trait ScalarField2D: Send + Sync {
    /// Muestrea el campo en `(x, z)` (coordenadas de mundo en bloques).
    fn sample(&self, x: f64, z: f64) -> f64;

    /// Muestrea `n` puntos: `xs[i]`, `zs[i]` -> `out[i]`. El default es escalar.
    fn sample_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        assert_eq!(xs.len(), zs.len());
        assert_eq!(xs.len(), out.len());
        for i in 0..xs.len() {
            out[i] = self.sample(xs[i], zs[i]) as f32;
        }
    }
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

/// Cuantizacion del ruido SIMD. Mitiga diferencias de ultimo bit entre
/// arquitecturas (FMA vs no FMA): el resultado queda en una rejilla de 1/4096.
const NOISE_QUANT: f32 = 4096.0;

/// `quick-noise` Fbm entrega ~1.5x la amplitud de `noise-rs` Fbm para la misma
/// configuracion; este gain la iguala para que v2 no sature el relieve.
const SIMD_FBM_GAIN: f32 = 0.65;

#[inline]
fn quantize(out: &mut [f32]) {
    for v in out.iter_mut() {
        *v = (*v * NOISE_QUANT).round() / NOISE_QUANT;
    }
}

/// fBm por lotes con despacho **runtime** a la mejor arquitectura disponible
/// (AVX2/AVX512/SSE/NEON) sin exigir `target-cpu=native`.
#[dispatch_simd(A)]
fn simd_fbm_fill(
    seed: i64,
    octaves: usize,
    frequency: f32,
    xs: &[f32],
    zs: &[f32],
    out: &mut [f32],
) {
    BatchNoise::<2, SimdFbm, SimdPerlin>::builder::<A, _, _>(
        xs.simd_iter::<A>(),
        zs.simd_iter::<A>(),
    )
    .octaves(octaves)
    .frequency(frequency)
    .persistence(0.5)
    .lacunarity(2.0)
    .amplitude(SIMD_FBM_GAIN)
    .seed(seed)
    .fill(out);
    quantize(out);
}

/// Crestas por lotes con despacho runtime. Salida remapeada a `[-1, 1]`.
#[dispatch_simd(A)]
fn simd_ridged_fill(
    seed: i64,
    octaves: usize,
    frequency: f32,
    xs: &[f32],
    zs: &[f32],
    out: &mut [f32],
) {
    BatchNoise::<2, SimdRidged, SimdPerlin>::builder::<A, _, _>(
        xs.simd_iter::<A>(),
        zs.simd_iter::<A>(),
    )
    .octaves(octaves)
    .frequency(frequency)
    .lacunarity(2.0)
    .gain(2.0)
    .seed(seed)
    .fill(out);
    for v in out.iter_mut() {
        *v = (*v * 2.0 - 1.0).clamp(-1.0, 1.0);
    }
    quantize(out);
}

/// fBm SIMD (`quick-noise`): misma firma que [`Fractal2D`] pero procesa lotes.
///
/// Es el backend de `generator_config >= 2`. v1 sigue usando `noise-rs` para
/// conservar su hash byte a byte.
#[derive(Clone, Copy, Debug)]
pub struct SimdFractal2D {
    seed: u32,
    octaves: usize,
    frequency: f64,
}

impl SimdFractal2D {
    pub fn new(seed: u32, octaves: usize, frequency: f64) -> Self {
        Self {
            seed,
            octaves: octaves.max(1),
            frequency,
        }
    }
}

impl ScalarField2D for SimdFractal2D {
    fn sample(&self, x: f64, z: f64) -> f64 {
        // Sin asignaciones: el camino de una sola muestra se usa en el anillo de
        // arboles y en `cave_carve_at`.
        let xf = [x as f32];
        let zf = [z as f32];
        let mut out = [0.0f32];
        simd_fbm_fill(
            self.seed as i64,
            self.octaves,
            self.frequency as f32,
            &xf,
            &zf,
            &mut out,
        );
        out[0] as f64
    }

    fn sample_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        assert_eq!(xs.len(), zs.len());
        assert_eq!(xs.len(), out.len());
        if xs.is_empty() {
            return;
        }
        let xf: Vec<f32> = xs.iter().map(|&v| v as f32).collect();
        let zf: Vec<f32> = zs.iter().map(|&v| v as f32).collect();
        simd_fbm_fill(
            self.seed as i64,
            self.octaves,
            self.frequency as f32,
            &xf,
            &zf,
            out,
        );
    }
}

/// Crestas SIMD (`quick-noise::Ridged`). `quick-noise` devuelve `[0, 1]`; se
/// remapea a `[-1, 1]` para que `default_peaks_curve` siga siendo valida.
#[derive(Clone, Copy, Debug)]
pub struct SimdRidged2D {
    seed: u32,
    octaves: usize,
    frequency: f64,
}

impl SimdRidged2D {
    pub fn new(seed: u32, octaves: usize, frequency: f64) -> Self {
        Self {
            seed,
            octaves: octaves.max(1),
            frequency,
        }
    }
}

impl ScalarField2D for SimdRidged2D {
    fn sample(&self, x: f64, z: f64) -> f64 {
        let xf = [x as f32];
        let zf = [z as f32];
        let mut out = [0.0f32];
        simd_ridged_fill(
            self.seed as i64,
            self.octaves,
            self.frequency as f32,
            &xf,
            &zf,
            &mut out,
        );
        out[0] as f64
    }

    fn sample_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        assert_eq!(xs.len(), zs.len());
        assert_eq!(xs.len(), out.len());
        if xs.is_empty() {
            return;
        }
        let xf: Vec<f32> = xs.iter().map(|&v| v as f32).collect();
        let zf: Vec<f32> = zs.iter().map(|&v| v as f32).collect();
        simd_ridged_fill(
            self.seed as i64,
            self.octaves,
            self.frequency as f32,
            &xf,
            &zf,
            out,
        );
    }
}

/// Backend seleccionable de fBm 2D.
#[derive(Clone)]
pub enum FractalField2D {
    Legacy(Fractal2D),
    Simd(SimdFractal2D),
}

impl FractalField2D {
    pub fn new(seed: u32, octaves: usize, frequency: f64, simd: bool) -> Self {
        if simd {
            Self::Simd(SimdFractal2D::new(seed, octaves, frequency))
        } else {
            Self::Legacy(Fractal2D::new(seed, octaves, frequency))
        }
    }
}

impl ScalarField2D for FractalField2D {
    fn sample(&self, x: f64, z: f64) -> f64 {
        match self {
            Self::Legacy(f) => f.sample(x, z),
            Self::Simd(f) => f.sample(x, z),
        }
    }

    fn sample_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        match self {
            Self::Legacy(f) => f.sample_many(xs, zs, out),
            Self::Simd(f) => f.sample_many(xs, zs, out),
        }
    }
}

/// Backend seleccionable de crestas 2D.
#[derive(Clone)]
pub enum RidgedField2D {
    Legacy(Ridged2D),
    Simd(SimdRidged2D),
}

impl RidgedField2D {
    pub fn new(seed: u32, octaves: usize, frequency: f64, simd: bool) -> Self {
        if simd {
            Self::Simd(SimdRidged2D::new(seed, octaves, frequency))
        } else {
            Self::Legacy(Ridged2D::new(seed, octaves, frequency))
        }
    }
}

impl ScalarField2D for RidgedField2D {
    fn sample(&self, x: f64, z: f64) -> f64 {
        match self {
            Self::Legacy(f) => f.sample(x, z),
            Self::Simd(f) => f.sample(x, z),
        }
    }

    fn sample_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        match self {
            Self::Legacy(f) => f.sample_many(xs, zs, out),
            Self::Simd(f) => f.sample_many(xs, zs, out),
        }
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
    x: FractalField2D,
    z: FractalField2D,
    /// Desplazamiento maximo en bloques.
    strength: f64,
}

impl Warp2D {
    /// Crea un warp con dos campos independientes (uno por eje) a la frecuencia
    /// y fuerza dadas, usando el backend legacy.
    pub fn new(seed: u32, frequency: f64, strength: f64) -> Self {
        Self::with_backend(seed, frequency, strength, false)
    }

    /// Igual que [`Warp2D::new`] pero eligiendo backend SIMD (`quick-noise`).
    pub fn new_simd(seed: u32, frequency: f64, strength: f64) -> Self {
        Self::with_backend(seed, frequency, strength, true)
    }

    fn with_backend(seed: u32, frequency: f64, strength: f64, simd: bool) -> Self {
        Self {
            x: FractalField2D::new(seed, 2, frequency, simd),
            z: FractalField2D::new(seed ^ 0x5bd1_e995, 2, frequency, simd),
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

    /// Version por lotes de [`Warp2D::apply`].
    pub fn apply_many(&self, xs: &[f64], zs: &[f64], out_x: &mut [f64], out_z: &mut [f64]) {
        let n = xs.len();
        assert_eq!(zs.len(), n);
        assert_eq!(out_x.len(), n);
        assert_eq!(out_z.len(), n);
        let mut wx = vec![0.0f32; n];
        let mut wz = vec![0.0f32; n];
        self.x.sample_many(xs, zs, &mut wx);
        self.z.sample_many(xs, zs, &mut wz);
        for i in 0..n {
            out_x[i] = xs[i] + wx[i] as f64 * self.strength;
            out_z[i] = zs[i] + wz[i] as f64 * self.strength;
        }
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

    #[test]
    fn los_campos_simd_estan_cuantizados() {
        // Mitiga diferencias de ultimo bit entre AVX/SSE/FMA: todo valor cae en
        // una rejilla de 1/4096.
        let f = SimdFractal2D::new(13, 4, 1.0 / 1200.0);
        let xs: Vec<f64> = (0..256).map(|i| (i % 16) as f64 * 37.0 - 300.0).collect();
        let zs: Vec<f64> = (0..256).map(|i| (i / 16) as f64 * 29.0 - 200.0).collect();
        let mut out = vec![0.0f32; xs.len()];
        f.sample_many(&xs, &zs, &mut out);
        for v in &out {
            let q = (*v * NOISE_QUANT).round() / NOISE_QUANT;
            assert_eq!(*v, q, "valor no cuantizado: {v}");
        }
    }
}
