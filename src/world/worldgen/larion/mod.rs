//! **Generador Larion**: pipeline multi-capa de terreno a escala monumental.
//!
//! Es un **tercer** camino de generacion, independiente de `Legacy16` y `Graph`.
//! Los mundos guardados con esos dos generadores **no cambian**: este modulo no
//! toca su codigo. Larion solo se construye cuando el mundo pide
//! `GeneratorKind::Larion`.
//!
//! Principios (segun la referencia Larion, sin copiar su implementacion):
//!
//! 1. **Continentalidad** de frecuencia muy baja como base de la escala, con
//!    **domain warping horizontal** (solo X y Z) para romper los contornos
//!    circulares.
//! 2. **Erosion** como campo continuo que escala la amplitud del relieve
//!    (montana joven vs llanura).
//! 3. **Meso-escala** de crestas multifractales (`RidgedMulti`) y valles.
//! 4. **Densidad 3D en banda** alrededor de `H` para voladizos reales.
//! 5. **Clima en bandas** latitudinales + lapse de altitud.
//! 6. **Rios sinuosos** con warp propio y cauces mas profundos en montana.
//!
//! Todo es determinista: `(seed, x, z) -> LarionSample` es puro y no depende del
//! orden de generacion. El generador es `Send + Sync`.
//!
//! Los numeros viven en [`config::LarionConfig`]; la matematica pura en
//! `erosion`, `height`, `rivers` y `density`; el ruido en `noise`; la spline en
//! `spline`.

pub mod biome;
pub mod climate;
pub mod config;
pub mod density;
pub mod erosion;
pub mod height;
pub mod noise;
pub mod rivers;
pub mod spline;

pub use biome::{BiomeBlend, BiomeNode, BiomeSelector};
pub use climate::ClimatePoint;
pub use config::{LARION_CONFIG_VERSION, LarionConfig, LarionConfigError};
pub use density::DensityField;
pub use height::LarionSample;
pub use noise::{Fractal2D, Fractal3D, Ridged2D, ScalarField2D, Warp2D};
pub use spline::{Spline, SplineError};

use std::f32::consts::TAU;

use crate::world::worldgen::math;

use self::height::{ComposedHeight, HeightInputs};

/// Generador Larion, determinista por semilla.
pub struct LarionGenerator {
    config: LarionConfig,
    seed: u32,

    /// Domain warping horizontal de la continentalidad.
    warp: Warp2D,
    continental: Fractal2D,
    erosion: Fractal2D,
    peaks: Ridged2D,
    valley: Fractal2D,
    macro_relief: Fractal2D,
    detail: Fractal2D,
    temperature: Fractal2D,
    humidity: Fractal2D,
    river_ridge: Fractal2D,
    river_warp: Fractal2D,
    river_width: Fractal2D,
    lake_basin: Fractal2D,

    /// `continentalness -> altura`.
    base_curve: Spline,
    /// `erosion -> ganancia de detalle`.
    detail_curve: Spline,
    /// `cresta cruda [-1,1] -> cresta normalizada [0,1]`.
    peaks_curve: Spline,

    density: DensityField,
    selector: BiomeSelector,
}

impl LarionGenerator {
    /// Crea el generador con la configuracion por defecto.
    pub fn new(seed: u32) -> Self {
        Self::with_config(seed, LarionConfig::default())
    }

    /// Crea el generador con una configuracion explicita. Si la configuracion no
    /// es valida, cae a la de por defecto (nunca deja el mundo en un estado
    /// absurdo silencioso).
    pub fn with_config(seed: u32, config: LarionConfig) -> Self {
        let config = if config.validate().is_ok() {
            config
        } else {
            LarionConfig::default()
        };
        let s = |k: u32| {
            seed.wrapping_mul(0x9E37_79B9)
                .wrapping_add(k.wrapping_mul(0x85EB_CA6B))
        };
        Self {
            warp: Warp2D::new(
                s(1),
                config.continental_warp_scale,
                config.continental_warp_strength,
            ),
            continental: Fractal2D::new(s(2), config.continental_octaves, config.continental_scale),
            erosion: Fractal2D::new(s(3), config.erosion_octaves, config.erosion_scale),
            peaks: Ridged2D::new(s(4), config.peaks_octaves, config.peaks_scale),
            valley: Fractal2D::new(s(5), 2, config.valley_scale),
            macro_relief: Fractal2D::new(s(6), 2, config.macro_scale),
            detail: Fractal2D::new(s(7), config.detail_octaves, config.detail_scale),
            temperature: Fractal2D::new(
                s(8),
                config.temperature_octaves,
                config.temperature_scale,
            ),
            humidity: Fractal2D::new(s(9), config.humidity_octaves, config.humidity_scale),
            river_ridge: Fractal2D::new(s(10), 1, config.river_scale),
            river_warp: Fractal2D::new(s(11), 2, config.river_warp_scale),
            river_width: Fractal2D::new(s(12), 1, config.river_scale * 0.5),
            lake_basin: Fractal2D::new(s(13), 1, config.river_scale * 2.1),
            base_curve: height::default_base_curve(),
            detail_curve: erosion::default_detail_curve(),
            peaks_curve: erosion::default_peaks_curve(),
            density: DensityField::new(
                s(14),
                config.density_band,
                config.density_scale,
                config.density_octaves,
                config.overhang_amplitude,
            ),
            selector: BiomeSelector::with_defaults(),
            config,
            seed,
        }
    }

    /// La configuracion en uso.
    pub fn config(&self) -> &LarionConfig {
        &self.config
    }

    /// La semilla del mundo.
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Fuerza de los voladizos `[0, 1]` para una muestra: solo en montana joven
    /// (poca erosion y relieve).
    #[inline]
    pub fn overhang_strength(sample: &LarionSample) -> f32 {
        let young = 1.0 - sample.erosion;
        let relief = math::smoothstep(0.08, 0.40, sample.mountain);
        (young * relief).clamp(0.0, 1.0)
    }

    /// Densidad 3D en unidad de bloque global. Positivo = solido.
    #[inline]
    pub fn density(&self, x: i32, y: i32, z: i32, sample: &LarionSample) -> f32 {
        self.density.density(
            x,
            y,
            z,
            sample.height,
            Self::overhang_strength(sample),
        )
    }

    /// Muestrea la columna `(x, z)`. Punto de entrada unico del generador.
    pub fn sample(&self, x: f64, z: f64) -> LarionSample {
        let cfg = &self.config;
        let (sx, sz) = self.warp.apply(x, z);

        // 1. Continentalidad (cruda en [-1, 1]) y erosion.
        let continental_raw = (self.continental.sample(sx, sz) as f32).clamp(-1.0, 1.0);
        let erosion = erosion::erosion_from_raw(self.erosion.sample(sx, sz) as f32);

        // 2. Crestas (normalizadas a [0, 1]) y demas campos de relieve.
        let peaks01 = self
            .peaks_curve
            .eval(self.peaks.sample(sx, sz) as f32)
            .clamp(0.0, 1.0);
        let macro_n = self.macro_relief.sample(sx, sz) as f32;
        let valley_raw = self.valley.sample(sx, sz) as f32;
        let detail = self.detail.sample(x, z) as f32;
        let detail_gain = self.detail_curve.eval(erosion);

        let composed = height::compose_height(
            cfg,
            &self.base_curve,
            &HeightInputs {
                continental_raw,
                erosion,
                peaks01,
                macro_n,
                valley_raw,
                detail,
                detail_gain,
            },
        );

        // 3. Clima en bandas: temperatura latitudinal (eje Z) + ruido, con lapse.
        let temperature = self.temperature_at(x as f32, z as f32, &composed, cfg);
        let humidity = self.humidity_at(x as f32, z as f32, composed.interior, cfg);

        // 4. Bioma multi-parametrico.
        let climate = ClimatePoint {
            continentalness: continental_raw * 0.5 + 0.5,
            erosion,
            peaks: peaks01,
            temperature,
            humidity,
        };
        let blend = self.selector.select(&climate);

        // 5. Mascara de montana (cresta x interior x tierra, atenuada por erosion).
        let mountain = (peaks01 * composed.interior * composed.landness * (1.0 - erosion * 0.85))
            .clamp(0.0, 1.0);

        // 6. Rios sinuosos con warp propio; profundidad dependiente de montana.
        let rw = self.river_warp.sample(sx, sz) as f32;
        let rwx = sx + (rw * cfg.river_warp_strength) as f64;
        let ridge_n = self.river_ridge.sample(rwx, sz) as f32;
        let ridge = 1.0 - ridge_n.abs();
        let width_n = self.river_width.sample(sx, sz) as f32 * 0.5 + 0.5;
        let flow = rivers::river_flow(humidity, width_n);
        let width = rivers::river_width(cfg, flow);
        let river_proximity = rivers::river_proximity(ridge, width) * composed.landness;
        let cut = rivers::river_cut(cfg, river_proximity, flow, mountain);

        // Lagos: depresion cerrada en valles humedos.
        let basin = self.lake_basin.sample(sx, sz) as f32 * 0.5 + 0.5;
        let lake_ness = (1.0 - math::saturate(valley_raw.abs()))
            * math::smoothstep(0.5, 0.8, humidity)
            * math::smoothstep(0.62, 1.0, basin)
            * composed.landness;
        let lake_cut = lake_ness * 5.0;

        let carved = (composed.height - cut - lake_cut).max(cfg.min_height);

        // 7. Nivel de agua: mar en el oceano, cauce/lago en tierra.
        let mut surface_water = 0.0f32;
        if cut > 0.25 {
            surface_water = surface_water.max(composed.height - cut * 0.35);
        }
        if lake_ness > 0.02 {
            surface_water = surface_water.max(composed.height - lake_cut * 0.35);
        }
        if carved < cfg.sea_level {
            surface_water = surface_water.max(cfg.sea_level);
        }

        LarionSample {
            height: carved,
            continentalness: continental_raw * 0.5 + 0.5,
            continental_raw,
            erosion,
            peaks: peaks01,
            temperature,
            humidity,
            mountain,
            river_proximity,
            surface_water,
            biome: blend.primary,
            blend,
            ocean: carved <= cfg.sea_level,
        }
    }

    /// Temperatura efectiva: banda latitudinal en Z + ruido, contraste y lapse.
    fn temperature_at(&self, x: f32, z: f32, composed: &ComposedHeight, cfg: &LarionConfig) -> f32 {
        let band = 0.5 + 0.5 * (z * TAU / cfg.latitude_band as f32).cos();
        let n = self.temperature.sample(x as f64, z as f64) as f32 * 0.5 + 0.5;
        let raw = band * 0.7 + n * 0.3;
        let contraste = math::saturate(0.5 + (raw - 0.5) * cfg.climate_contrast);
        let alt = math::smoothstep(cfg.sea_level, cfg.max_height, composed.height.max(cfg.sea_level));
        (contraste - cfg.altitude_lapse_rate * alt).clamp(0.0, 1.0)
    }

    /// Humedad efectiva: ruido de baja frecuencia + sesgo costero (mas humedo
    /// cerca de la costa, mas seco en el interior).
    fn humidity_at(&self, x: f32, z: f32, interior: f32, cfg: &LarionConfig) -> f32 {
        let n = self.humidity.sample(x as f64, z as f64) as f32 * 0.5 + 0.5;
        // Sesgo costero simetrico: +bias en la costa, -bias en el interior.
        let raw = n + cfg.coast_humidity_bias * (1.0 - 2.0 * interior);
        math::saturate(0.5 + (raw - 0.5) * cfg.climate_contrast)
    }
}

#[cfg(test)]
mod tests;
