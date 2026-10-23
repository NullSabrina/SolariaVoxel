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

pub mod aquifer;
pub mod biome;
pub mod climate;
pub mod config;
pub mod density;
pub mod erosion;
pub mod height;
pub mod hydro;
pub mod noise;
pub mod rivers;
pub mod spline;

pub use aquifer::{Aquifer, AquiferFluid, AquiferSample};
pub use biome::{BiomeBlend, BiomeNode, BiomeSelector};
pub use climate::ClimatePoint;
pub use config::{HydroConfig, LARION_CONFIG_VERSION, LarionConfig, LarionConfigError};
pub use density::DensityField;
pub use height::LarionSample;
pub use hydro::{CELL, HALO, HydroCache, HydroSample, HydroWindow, MAX_WINDOWS, WINDOW};
pub use noise::{
    Fractal2D, Fractal3D, FractalField2D, Ridged2D, RidgedField2D, ScalarField2D, SimdFractal2D,
    SimdRidged2D, Warp2D,
};
pub use spline::{Spline, SplineError};

use std::f32::consts::TAU;

use crate::world::terrain::Biome;
use crate::world::worldgen::math;

use self::height::{ComposedHeight, HeightInputs};

/// Resultado de la geografia pura (sin clima/bioma/rios). Se reutiliza para
/// componer la muestra completa sin repetir ruido.
#[derive(Clone, Copy, Debug)]
struct GeoFields {
    composed: ComposedHeight,
    continental_raw: f32,
    erosion: f32,
    peaks01: f32,
    valley_raw: f32,
    warped: (f64, f64),
}

/// Generador Larion, determinista por semilla.
pub struct LarionGenerator {
    config: LarionConfig,
    seed: u32,

    /// Domain warping horizontal de la continentalidad.
    warp: Warp2D,
    continental: noise::FractalField2D,
    erosion: noise::FractalField2D,
    peaks: noise::RidgedField2D,
    valley: noise::FractalField2D,
    macro_relief: noise::FractalField2D,
    detail: noise::FractalField2D,
    temperature: noise::FractalField2D,
    humidity: noise::FractalField2D,
    river_ridge: noise::FractalField2D,
    river_warp: noise::FractalField2D,
    river_width: noise::FractalField2D,
    lake_basin: noise::FractalField2D,

    /// `continentalness -> altura`.
    base_curve: Spline,
    /// `erosion -> ganancia de detalle`.
    detail_curve: Spline,
    /// `cresta cruda [-1,1] -> cresta normalizada [0,1]`.
    peaks_curve: Spline,

    density: DensityField,
    selector: BiomeSelector,
    /// Version del pipeline: 1 = historico, 2 = hidrologia P0.1, 3 = arboles
    /// organicos + SIMD + `density_youth`.
    config_version: u32,
    /// Cache de ventanas hidro (solo se usa en `config_version >= 2`).
    hydro: HydroCache,
}

impl LarionGenerator {
    /// Crea el generador con la configuracion por defecto.
    pub fn new(seed: u32) -> Self {
        Self::with_version(seed, LarionConfig::default(), 1)
    }

    /// Crea el generador con una configuracion explicita y la version del
    /// pipeline. `version = 1` conserva byte a byte la generacion historica;
    /// `version = 2` activa la hidrologia P0.1; `version = 3` anade el backend
    /// SIMD y la densidad variable. Si la configuracion no es valida, cae a la
    /// de por defecto (nunca deja el mundo en un estado absurdo silencioso).
    pub fn with_config(seed: u32, config: LarionConfig) -> Self {
        Self::with_version(seed, config, 1)
    }

    /// Igual que [`LarionGenerator::with_config`] pero eligiendo la version del
    /// pipeline. Los mundos existentes usan 1-3; los nuevos, 4.
    pub fn with_version(seed: u32, config: LarionConfig, version: u32) -> Self {
        let mut config = if config.validate().is_ok() {
            config
        } else {
            LarionConfig::default()
        };
        // config 4: regiones climaticas mas amplias y menos contraste extremo.
        // El clima manda en el bioma (ver `BiomeSelector::with_defaults_v4`).
        if version >= 4 {
            config.temperature_scale = 1.0 / 700.0;
            config.humidity_scale = 1.0 / 650.0;
            config.climate_contrast = 1.5;
            // Menos arroyos: solo los cauces con caudal real llevan agua. Antes
            // cada linea de drenaje generaba un rio de 2 bloques.
            config.hydro.river_min_flow = 0.10;
        }
        let s = |k: u32| {
            seed.wrapping_mul(0x9E37_79B9)
                .wrapping_add(k.wrapping_mul(0x85EB_CA6B))
        };
        // v3 usa el backend SIMD; v1 y v2 conservan `noise-rs` byte a byte.
        let simd = version >= 3;
        Self {
            warp: if simd {
                Warp2D::new_simd(
                    s(1),
                    config.continental_warp_scale,
                    config.continental_warp_strength,
                )
            } else {
                Warp2D::new(
                    s(1),
                    config.continental_warp_scale,
                    config.continental_warp_strength,
                )
            },
            continental: noise::FractalField2D::new(
                s(2),
                config.continental_octaves,
                config.continental_scale,
                simd,
            ),
            erosion: noise::FractalField2D::new(
                s(3),
                config.erosion_octaves,
                config.erosion_scale,
                simd,
            ),
            // Las crestas se quedan en `noise-rs`: `quick-noise::Ridged` tiene
            // otra distribucion (mediana ~3.7 en 5 octavas) y saturaba
            // `peaks01` a 1.0 en v2. El resto de campos 2D si usan SIMD.
            peaks: noise::RidgedField2D::new(s(4), config.peaks_octaves, config.peaks_scale, false),
            valley: noise::FractalField2D::new(s(5), 2, config.valley_scale, simd),
            macro_relief: noise::FractalField2D::new(s(6), 2, config.macro_scale, simd),
            detail: noise::FractalField2D::new(
                s(7),
                config.detail_octaves,
                config.detail_scale,
                simd,
            ),
            temperature: noise::FractalField2D::new(
                s(8),
                config.temperature_octaves,
                config.temperature_scale,
                simd,
            ),
            humidity: noise::FractalField2D::new(
                s(9),
                config.humidity_octaves,
                config.humidity_scale,
                simd,
            ),
            river_ridge: noise::FractalField2D::new(s(10), 1, config.river_scale, simd),
            river_warp: noise::FractalField2D::new(s(11), 2, config.river_warp_scale, simd),
            river_width: noise::FractalField2D::new(s(12), 1, config.river_scale * 0.5, simd),
            lake_basin: noise::FractalField2D::new(s(13), 1, config.river_scale * 2.1, simd),
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
            selector: if version >= 4 {
                BiomeSelector::with_defaults_v4()
            } else {
                BiomeSelector::with_defaults()
            },
            config_version: version.max(1),
            hydro: HydroCache::default(),
            config,
            seed,
        }
    }

    /// Version del pipeline en uso (1 = historico, 2 = P0.1).
    pub fn config_version(&self) -> u32 {
        self.config_version
    }

    /// Numero de ventanas hidro construidas (diagnostico/benchmarks).
    pub fn hydro_builds(&self) -> usize {
        self.hydro.builds()
    }

    /// Ventana que cubre la columna `(x, z)`. El halo de `HALO` bloques cubre
    /// toda la columna y su margen de decoracion, asi que una columna usa **una
    /// sola** ventana (evita el thrash de `MAX_WINDOWS` en bordes).
    pub fn hydro_window(&self, x: i32, z: i32) -> std::sync::Arc<HydroWindow> {
        self.hydro.get(self, x, z)
    }

    /// La configuracion en uso.
    pub fn config(&self) -> &LarionConfig {
        &self.config
    }

    /// La semilla del mundo.
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Geografia pura: continentalidad, erosion, crestas, macro, valle y detalle.
    /// No toca clima, bioma ni rios. `include_detail = false` omite el
    /// micro-relieve de 2 bloques (lo usa la ventana hidro, donde no aporta a la
    /// red de drenaje y dispararia el coste).
    fn geography(&self, x: f64, z: f64, include_detail: bool) -> GeoFields {
        let cfg = &self.config;
        let (sx, sz) = self.warp.apply(x, z);

        let continental_raw = (self.continental.sample(sx, sz) as f32).clamp(-1.0, 1.0);
        let erosion = erosion::erosion_from_raw(self.erosion.sample(sx, sz) as f32);
        let peaks01 = self
            .peaks_curve
            .eval(self.peaks.sample(sx, sz) as f32)
            .clamp(0.0, 1.0);
        let macro_n = self.macro_relief.sample(sx, sz) as f32;
        let valley_raw = self.valley.sample(sx, sz) as f32;
        let detail = if include_detail {
            self.detail.sample(x, z) as f32
        } else {
            0.0
        };
        let detail_gain = self.detail_curve.eval(erosion);

        GeoFields {
            composed: height::compose_height(
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
            ),
            continental_raw,
            erosion,
            peaks01,
            valley_raw,
            warped: (sx, sz),
        }
    }

    /// Version por lotes de [`LarionGenerator::geography`]. Usa `sample_many`
    /// para que el backend SIMD procese todos los puntos de una columna (o de la
    /// ventana hidro) en pocas llamadas.
    fn geography_grid(&self, xs: &[f64], zs: &[f64], include_detail: bool) -> Vec<GeoFields> {
        let cfg = &self.config;
        let n = xs.len();
        assert_eq!(zs.len(), n);
        let mut sx = vec![0.0f64; n];
        let mut sz = vec![0.0f64; n];
        self.warp.apply_many(xs, zs, &mut sx, &mut sz);

        // Un unico buffer para los seis campos de la geografia: antes eran seis
        // `Vec` por columna (mas el resto de la composicion).
        let mut fields = vec![0.0f32; n * 6];
        let (continental, rest) = fields.split_at_mut(n);
        let (erosion_raw, rest) = rest.split_at_mut(n);
        let (peaks_raw, rest) = rest.split_at_mut(n);
        let (macro_raw, rest) = rest.split_at_mut(n);
        let (valley_raw, rest) = rest.split_at_mut(n);
        let detail = rest;
        self.continental.sample_many(&sx, &sz, continental);
        self.erosion.sample_many(&sx, &sz, erosion_raw);
        self.peaks.sample_many(&sx, &sz, peaks_raw);
        self.macro_relief.sample_many(&sx, &sz, macro_raw);
        self.valley.sample_many(&sx, &sz, valley_raw);
        if include_detail {
            self.detail.sample_many(xs, zs, detail);
        }

        (0..n)
            .map(|i| {
                let continental_raw = continental[i].clamp(-1.0, 1.0);
                let erosion = erosion::erosion_from_raw(erosion_raw[i]);
                let peaks01 = self.peaks_curve.eval(peaks_raw[i]).clamp(0.0, 1.0);
                let valley_raw = valley_raw[i];
                let detail_gain = self.detail_curve.eval(erosion);
                GeoFields {
                    composed: height::compose_height(
                        cfg,
                        &self.base_curve,
                        &HeightInputs {
                            continental_raw,
                            erosion,
                            peaks01,
                            macro_n: macro_raw[i],
                            valley_raw,
                            detail: detail[i],
                            detail_gain,
                        },
                    ),
                    continental_raw,
                    erosion,
                    peaks01,
                    valley_raw,
                    warped: (sx[i], sz[i]),
                }
            })
            .collect()
    }

    /// Altura geografica (sin clima/bioma/rios) por punto, en f32. Camino
    /// escalar historico: lo usa la hidrologia de v1/v2 para no cambiar su hash.
    pub(super) fn geography_height(&self, x: f64, z: f64) -> f32 {
        self.geography(x, z, false).composed.height
    }

    /// Altura geografica (sin clima/bioma/rios). La usa [`HydroWindow`].
    pub(super) fn geography_height_many(&self, xs: &[f64], zs: &[f64], out: &mut [f32]) {
        assert_eq!(xs.len(), zs.len());
        assert_eq!(xs.len(), out.len());
        let geos = self.geography_grid(xs, zs, false);
        for (i, g) in geos.iter().enumerate() {
            out[i] = g.composed.height;
        }
    }

    /// Muestrea una rejilla rectangular `w x h` (fila a fila, X avanza primero)
    /// con backend SIMD y una sola ventana hidro. Equivale a llamar
    /// `sample_with_window` en cada punto, pero agrupa cada campo de ruido.
    pub fn sample_grid(
        &self,
        x0: f64,
        z0: f64,
        w: usize,
        h: usize,
        window: Option<&HydroWindow>,
    ) -> Vec<LarionSample> {
        let n = w * h;
        let mut xs = Vec::with_capacity(n);
        let mut zs = Vec::with_capacity(n);
        for j in 0..h {
            for i in 0..w {
                xs.push(x0 + i as f64);
                zs.push(z0 + j as f64);
            }
        }
        let geos = self.geography_grid(&xs, &zs, true);
        geos.into_iter()
            .enumerate()
            .map(|(i, geo)| self.compose_with_geo(geo, xs[i], zs[i], window))
            .collect()
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
        self.density
            .density(x, y, z, sample.height, Self::overhang_strength(sample))
    }

    /// Densidad segun la version del pipeline. v1 y v2 conservan la banda fija
    /// historica; v3 usa banda/amplitud variables (P1.3).
    #[inline]
    pub fn density_versioned(
        &self,
        x: i32,
        y: i32,
        z: i32,
        sample: &LarionSample,
        version: u32,
    ) -> f32 {
        if version >= 3 {
            self.density
                .density_youth(x, y, z, sample.height, Self::overhang_strength(sample))
        } else {
            self.density(x, y, z, sample)
        }
    }

    /// Muestrea la columna `(x, z)`. Punto de entrada unico del generador.
    pub fn sample(&self, x: f64, z: f64) -> LarionSample {
        self.sample_with_window(x, z, None)
    }

    /// Muestrea usando una ventana hidro ya resuelta. La generacion de columnas
    /// la resuelve **una vez** por columna y la pasa a sus 18x18 muestras para
    /// no expulsar ventanas en cada borde. `None` resuelve por punto (API suelta).
    pub fn sample_with_window(&self, x: f64, z: f64, window: Option<&HydroWindow>) -> LarionSample {
        let geo = self.geography(x, z, true);
        self.compose_with_geo(geo, x, z, window)
    }

    /// Compone una muestra completa a partir de una geografia ya calculada.
    fn compose_with_geo(
        &self,
        geo: GeoFields,
        x: f64,
        z: f64,
        window: Option<&HydroWindow>,
    ) -> LarionSample {
        let cfg = &self.config;
        let composed = geo.composed;
        let continental_raw = geo.continental_raw;
        let erosion = geo.erosion;
        let peaks01 = geo.peaks01;

        // Clima en bandas: temperatura latitudinal (eje Z) + ruido, con lapse.
        let temperature = self.temperature_at(x as f32, z as f32, &composed, cfg);
        let humidity = self.humidity_at(x as f32, z as f32, composed.interior, cfg);

        // Bioma multi-parametrico.
        let climate = ClimatePoint {
            continentalness: continental_raw * 0.5 + 0.5,
            erosion,
            peaks: peaks01,
            temperature,
            humidity,
        };
        let blend = self.selector.select(&climate);

        // config 4: el relieve depende del bioma (pantanos bajos, llanuras y
        // sabanas planas, desierto suave). Se aplica despues de elegir el bioma
        // para no realimentar el clima.
        let composed = if self.config_version >= 4 {
            let mut c = composed;
            c.height = biome::height_modifier(blend, c.height, cfg.sea_level)
                .clamp(cfg.min_height, cfg.max_height);
            c
        } else {
            composed
        };

        // Mascara de montana (cresta x interior x tierra, atenuada por erosion).
        let mountain = (peaks01 * composed.interior * composed.landness * (1.0 - erosion * 0.85))
            .clamp(0.0, 1.0);

        let (height, river_proximity, surface_water, hydro_erosion, tree_density) =
            if self.config_version >= 2 {
                // P0.1: drenaje global + perfil en V del cauce.
                let hydro = match window {
                    Some(window) => window.sample(x as f32, z as f32),
                    None => self
                        .hydro
                        .get(self, x as i32, z as i32)
                        .sample(x as f32, z as f32),
                };
                let v_depth = if self.config_version >= 3 {
                    hydro.v_depth_v3(&cfg.hydro)
                } else {
                    hydro.v_depth(&cfg.hydro)
                };
                let height =
                    (composed.height + hydro.dh - v_depth).clamp(cfg.min_height, cfg.max_height);
                let proximity = if hydro.flow > cfg.hydro.river_min_flow {
                    hydro.proximity() * composed.landness
                } else {
                    0.0
                };
                // Lago/cauce: `hydro.water` ya es 0 fuera del agua y la superficie
                // del priority-flood dentro (incluido el ancho completo del cauce).
                // Antes se exigia `water > height + 0.25`, lo que descartaba el
                // agua de un rio cuyo nivel coincide con la superficie: el canal
                // quedaba seco salvo una lama central.
                let mut water = hydro.water;
                if water < 0.0 || !water.is_finite() {
                    water = 0.0;
                }
                if self.config_version >= 4 {
                    // Solo se fuerza el nivel del mar si la columna es oceanica
                    // o la hidrologia ya la considera mojada. Antes, cualquier
                    // depresion interior bajo el mar se convertia en un "mar"
                    // aunque no tuviera conexion.
                    let oceanic = composed.landness < 0.25;
                    if height < cfg.sea_level && (oceanic || water > 0.0) {
                        water = water.max(cfg.sea_level);
                    }
                    // Un desierto seco no inventa lagos en cada hueco. Los
                    // cauces con caudal real (rios) se conservan.
                    if !oceanic
                        && blend.primary == Biome::Desert
                        && hydro.flow <= cfg.hydro.river_min_flow
                    {
                        water = 0.0;
                    }
                } else if height < cfg.sea_level {
                    water = water.max(cfg.sea_level);
                }
                let tree_density = tree_density_value(
                    blend.primary,
                    blend.secondary,
                    blend.mix,
                    humidity,
                    temperature,
                    Some((hydro.dist, hydro.width, hydro.water)),
                );
                (height, proximity, water, (-hydro.dh).max(0.0), tree_density)
            } else {
                // v1: comportamiento historico, byte a byte. NO tocar el orden de
                // las muestras de ruido.
                let (sx, sz) = geo.warped;
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
                let valley_raw = geo.valley_raw;
                let basin = self.lake_basin.sample(sx, sz) as f32 * 0.5 + 0.5;
                let lake_ness = (1.0 - math::saturate(valley_raw.abs()))
                    * math::smoothstep(0.5, 0.8, humidity)
                    * math::smoothstep(0.62, 1.0, basin)
                    * composed.landness;
                let lake_cut = lake_ness * 5.0;

                let carved = (composed.height - cut - lake_cut).max(cfg.min_height);

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
                let tree_density = tree_density_value(
                    blend.primary,
                    blend.secondary,
                    blend.mix,
                    humidity,
                    temperature,
                    None,
                ) * (1.0 - 0.8 * river_proximity);
                (carved, river_proximity, surface_water, 0.0, tree_density)
            };

        LarionSample {
            height,
            continentalness: continental_raw * 0.5 + 0.5,
            continental_raw,
            erosion,
            peaks: peaks01,
            temperature,
            humidity,
            mountain,
            river_proximity,
            surface_water,
            hydro_erosion,
            tree_density,
            biome: blend.primary,
            blend,
            ocean: height <= cfg.sea_level,
        }
    }

    /// Temperatura efectiva: banda latitudinal en Z + ruido, contraste y lapse.
    fn temperature_at(&self, x: f32, z: f32, composed: &ComposedHeight, cfg: &LarionConfig) -> f32 {
        let band = 0.5 + 0.5 * (z * TAU / cfg.latitude_band as f32).cos();
        // config 4: comprime la banda latitudinal hacia el centro. Con la banda
        // pura, casi toda el area cae en los extremos (frio/calor) y los biomas
        // templados (llanura/bosque/pantano) quedan en minoria.
        let band = if self.config_version >= 4 {
            0.5 + (band - 0.5) * 0.55
        } else {
            band
        };
        let n = self.temperature.sample(x as f64, z as f64) as f32 * 0.5 + 0.5;
        let raw = band * 0.7 + n * 0.3;
        let contraste = math::saturate(0.5 + (raw - 0.5) * cfg.climate_contrast);
        let alt = math::smoothstep(
            cfg.sea_level,
            cfg.max_height,
            composed.height.max(cfg.sea_level),
        );
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

/// Densidad base de un bioma normalizada a `[0, 1]`. Solo pesa un 8% en la
/// densidad final: el grueso viene de campos **continuos** de clima, para que la
/// frontera de bioma no produzca un escalon de arboles.
fn biome_tree_base(biome: crate::world::terrain::Biome) -> f32 {
    use crate::world::terrain::Biome;
    match biome {
        Biome::Forest => 1.0,
        Biome::Taiga => 0.9,
        Biome::Swamp => 0.8,
        Biome::Plains => 0.55,
        Biome::Savanna => 0.40,
        Biome::Tundra => 0.15,
        Biome::Desert => 0.0,
    }
}

/// Densidad de vegetacion de una columna (P0.3). Es una funcion **continua** de
/// humedad/temperatura, con el bioma mezclado por `mix` (que vale 0.5 justo en la
/// frontera y da el mismo resultado al intercambiar primario/secundario). El
/// cauce y el agua anulan la densidad; el borde del cauce se suaviza en 2 anchos
/// para que no haya un muro de arboles junto al rio.
fn tree_density_value(
    primary: crate::world::terrain::Biome,
    secondary: crate::world::terrain::Biome,
    mix: f32,
    humidity: f32,
    temperature: f32,
    hydro: Option<(f32, f32, f32)>,
) -> f32 {
    let base = biome_tree_base(primary) * (1.0 - mix) + biome_tree_base(secondary) * mix;
    let wet = math::smoothstep(0.30, 0.80, humidity);
    let arid =
        math::smoothstep(0.72, 0.95, temperature) * (1.0 - math::smoothstep(0.18, 0.55, humidity));
    let cold = math::smoothstep(0.02, 0.22, temperature);
    let mut density = wet * (1.0 - 0.75 * arid) * (0.35 + 0.65 * cold) + 0.08 * base;
    if let Some((dist, width, water)) = hydro {
        if water > 0.0 || dist < width {
            density = 0.0;
        } else {
            density *= math::smoothstep(width, width * 2.0 + 2.0, dist);
        }
    }
    density.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
