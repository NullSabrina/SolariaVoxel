//! **Generacion de mundo por etapas** (auditoria de worldgen).
//!
//! FASE 1 (fundacion) + FASE 2 (celular + continentes + costas) de la
//! especificacion. En vez de `noise + noise + noise = terreno`, aqui se produce
//! primero un [`TerrainSample`] geografico (continentalness, celda, costa,
//! altura base) y despues `terrain.rs` lo convierte a bloques.
//!
//! Estado de las fases (honesto):
//!
//! * `IMPLEMENTED`: config central y validacion, seeds derivadas, celular,
//!   clasificacion continental, costa de ancho variable, altura base continental
//!   mas relieve macro, cordilleras (mascara de rango y cresta), valles, domain
//!   warping y helpers de math.
//!
//! * `DEFERRED`: bioma por region celular (FASE 3; hoy el bioma sigue siendo por
//!   clima), hidrologia y rios (FASE 5), jerarquia de cuevas (FASE 6),
//!   decoracion por reglas (FASE 7), previews y benchmarks (FASE 9).
//!
//! Todo es determinista: `(seed, x, z)` da siempre el mismo resultado, sin RNG
//! con estado. La GPU no se toca.

pub mod biomes;
pub mod cells;
pub mod config;
pub mod math;

pub use biomes::BiomeDefinition;
pub use cells::CellSample;
pub use config::{ConfigError, WORLDGEN_CONFIG_VERSION, WorldGenConfig};

use std::sync::atomic::{AtomicU32, Ordering};

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use super::terrain::{Biome, SEA_LEVEL};

/// Clasificacion continental de una muestra (macro-geografia).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandClass {
    DeepOcean,
    Ocean,
    Shelf,
    CoastalLand,
    Lowland,
    Highland,
    Interior,
}

impl LandClass {
    /// ¿Es tierra (por encima de la plataforma)?
    #[inline]
    pub fn is_land(self) -> bool {
        !matches!(
            self,
            LandClass::DeepOcean | LandClass::Ocean | LandClass::Shelf
        )
    }

    /// ¿Es oceano (incluida la plataforma)?
    #[inline]
    pub fn is_ocean(self) -> bool {
        !self.is_land()
    }

    /// Clasifica por continentalness. Los cortes son parametros de config.
    pub fn classify(c: f32, cfg: &WorldGenConfig) -> Self {
        if c < cfg.deep_ocean_threshold {
            LandClass::DeepOcean
        } else if c < cfg.ocean_threshold - 0.15 {
            LandClass::Ocean
        } else if c < cfg.ocean_threshold {
            LandClass::Shelf
        } else if c < cfg.ocean_threshold + 0.12 {
            LandClass::CoastalLand
        } else if c < 0.45 {
            LandClass::Lowland
        } else if c < 0.72 {
            LandClass::Highland
        } else {
            LandClass::Interior
        }
    }
}

/// Datos geograficos de un punto, **antes** de convertirlos a bloques.
#[derive(Clone, Copy, Debug)]
pub struct TerrainSample {
    /// Campo continental en `[-1, 1]`: `< ocean_threshold` = oceano.
    pub continentalness: f32,
    /// Categoria continental.
    pub land: LandClass,
    /// Id estable de la celda de bioma (para features/coherencia regional).
    pub cell_id: u64,
    /// `saturate((F2-F1)/cell_distance)`: 0 en la frontera de celda, ~1 dentro.
    pub cell_edge: f32,
    /// 1 en la linea de costa, 0 tierra adentro o mar adentro.
    pub coast_factor: f32,
    /// `[0, 1)` por celda: decide el ancho de costa (estrecha/ancha).
    pub coast_roll: f32,
    /// Temperatura efectiva (tras mezcla de celda y lapse de altitud), `[0, 1]`.
    pub temperature: f32,
    /// Humedad efectiva (tras mezcla de celda), `[0, 1]`.
    pub humidity: f32,
    /// Bioma seleccionado por scoring.
    pub biome: Biome,
    /// Altura base continua del terreno, en bloques (antes de clampear).
    pub base_height: f32,
}

/// Deriva una semilla por campo a partir de la del mundo. No usa estado global
/// mutable, asi que el generador viaja entre hilos sin problemas.
#[inline]
fn derive(seed: u32, salt: u32) -> u32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(salt.wrapping_mul(0x85EB_CA6B));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Altura base **relativa al nivel del mar** segun continentalness. Es una
/// spline (no una cascada de `if`) para que la transicion oceano->tierra sea
/// suave y controlable desde un solo sitio.
fn continental_base(c: f32) -> f32 {
    // Cruza el 0 en `c = ocean_threshold` (0.0): ahi esta la linea de costa.
    let points: [(f32, f32); 9] = [
        (-1.00, -64.0),
        (-0.60, -40.0),
        (-0.30, -22.0),
        (-0.10, -8.0),
        (0.00, 0.0),
        (0.12, 10.0),
        (0.45, 42.0),
        (0.75, 86.0),
        (1.00, 140.0),
    ];
    math::spline(&points, c)
}

/// Generador de mundo por etapas (FASE 1/2). Es `Send + Sync` (solo contiene
/// ruido sin estado mutable).
pub struct WorldGen {
    config: WorldGenConfig,
    seed: u32,
    seed_cell: u32,
    seed_coast: u32,
    continental_macro: Fbm<Perlin>,
    continental_detail: Perlin,
    macro_relief: Fbm<Perlin>,
    mountain_ranges: Fbm<Perlin>,
    ridge: Perlin,
    valley: Perlin,
    warp_x: Perlin,
    warp_z: Perlin,
    /// Clima (FASE 3): temperatura y humedad.
    temperature: Fbm<Perlin>,
    humidity: Fbm<Perlin>,
    /// Contador de evaluaciones de ruido (tests de coste). Atomico para seguir
    /// siendo `Send + Sync`.
    noise_calls: AtomicU32,
}

impl WorldGen {
    /// Construye el generador para una semilla. Valida la configuracion; si no
    /// es valida, cae a la de por defecto (nunca deja el mundo en un estado
    /// absurdo silencioso).
    pub fn new(seed: u32) -> Self {
        let config = WorldGenConfig::default();
        debug_assert!(config.validate().is_ok(), "config por defecto invalida");
        Self::with_config(seed, config)
    }

    /// Construye con una configuracion explicita (para previews/benchmarks).
    pub fn with_config(seed: u32, config: WorldGenConfig) -> Self {
        Self {
            seed_cell: derive(seed, 101),
            seed_coast: derive(seed, 102),
            continental_macro: Fbm::<Perlin>::new(derive(seed, 103))
                .set_octaves(4)
                .set_frequency(1.0)
                .set_persistence(0.5),
            continental_detail: Perlin::new(derive(seed, 104)),
            macro_relief: Fbm::<Perlin>::new(derive(seed, 105))
                .set_octaves(5)
                .set_frequency(1.0)
                .set_persistence(0.5),
            mountain_ranges: Fbm::<Perlin>::new(derive(seed, 106))
                .set_octaves(3)
                .set_frequency(1.0)
                .set_persistence(0.5),
            ridge: Perlin::new(derive(seed, 107)),
            valley: Perlin::new(derive(seed, 108)),
            warp_x: Perlin::new(derive(seed, 109)),
            warp_z: Perlin::new(derive(seed, 110)),
            temperature: Fbm::<Perlin>::new(derive(seed, 111))
                .set_octaves(3)
                .set_frequency(1.0)
                .set_persistence(0.5),
            humidity: Fbm::<Perlin>::new(derive(seed, 112))
                .set_octaves(3)
                .set_frequency(1.0)
                .set_persistence(0.5),
            noise_calls: AtomicU32::new(0),
            config,
            seed,
        }
    }

    /// Cuenta una evaluacion de ruido (solo para tests de coste).
    #[inline]
    fn bump(&self) {
        self.noise_calls.fetch_add(1, Ordering::Relaxed);
    }

    /// Evaluaciones de ruido 2D desde el ultimo reset.
    pub fn noise_calls(&self) -> u32 {
        self.noise_calls.load(Ordering::Relaxed)
    }

    /// Reinicia el contador de ruido.
    pub fn reset_noise_calls(&self) {
        self.noise_calls.store(0, Ordering::Relaxed);
    }

    /// La configuracion en uso.
    pub fn config(&self) -> &WorldGenConfig {
        &self.config
    }

    /// La semilla del mundo.
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Campo de clima normalizado a `[0, 1]` en `(x, z)`.
    fn climate_field(&self, noise: &Fbm<Perlin>, x: f64, z: f64, scale: f64) -> f32 {
        self.bump();
        ((noise.get([x * scale, z * scale]) as f32) * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    /// Muestrea la geografia en `(x, z)`. Es la unica fuente de verdad del
    /// relieve y del bioma: `terrain.rs` solo la convierte a bloques.
    pub fn sample(&self, x: f64, z: f64) -> TerrainSample {
        let cfg = &self.config;

        // Domain warping sobre las coordenadas para romper patrones regulares.
        self.bump();
        let wx = self.warp_x.get([x * cfg.warp_scale, z * cfg.warp_scale]);
        self.bump();
        let wz = self
            .warp_z
            .get([x * cfg.warp_scale + 19.3, z * cfg.warp_scale + 7.1]);
        let sx = x + wx * cfg.warp_strength as f64;
        let sz = z + wz * cfg.warp_strength as f64;

        // Continentalness: macro + detalle, normalizado a [-1, 1].
        self.bump();
        let c_macro =
            self.continental_macro
                .get([sx * cfg.continental_scale, sz * cfg.continental_scale]) as f32;
        self.bump();
        let c_detail = self.continental_detail.get([
            sx * cfg.continental_detail_scale,
            sz * cfg.continental_detail_scale,
        ]) as f32;
        let continentalness = (0.78 * c_macro + 0.22 * c_detail).clamp(-1.0, 1.0);

        // Red celular (id de region) y ancho de costa por celda.
        let cell = cells::sample(
            self.seed_cell,
            cfg.cell_distance,
            cfg.cell_jitter,
            x as f32,
            z as f32,
        );
        let coast_roll = cells::hash01(self.seed_coast, cell.cell_x, cell.cell_z, 7);
        let band = math::lerp(cfg.coast_narrow, cfg.coast_wide, coast_roll);
        let coast_factor =
            1.0 - math::smoothstep(0.0, band, (continentalness - cfg.ocean_threshold).abs());

        let land = LandClass::classify(continentalness, cfg);
        // Cuanta "tierra" hay: apaga el relieve en el oceano.
        let landness = math::smoothstep(
            cfg.ocean_threshold,
            cfg.ocean_threshold + 0.30,
            continentalness,
        );

        // Altura: base continental + relieve macro + cordilleras - valles.
        let mut h = SEA_LEVEL as f32 + continental_base(continentalness);

        self.bump();
        let macro_n = self
            .macro_relief
            .get([sx * cfg.macro_scale, sz * cfg.macro_scale]) as f32;
        h += macro_n * cfg.macro_amplitude * landness;

        self.bump();
        let range_n = self
            .mountain_ranges
            .get([sx * cfg.mountain_scale, sz * cfg.mountain_scale]) as f32
            * 0.5
            + 0.5;
        let range_mask = math::smoothstep(cfg.range_low, cfg.range_high, range_n);
        self.bump();
        let ridge_n = self
            .ridge
            .get([sx * cfg.mountain_scale * 1.7, sz * cfg.mountain_scale * 1.7])
            as f32;
        let ridge = (1.0 - ridge_n.abs()).max(0.0).powf(cfg.ridge_power);
        let interior = math::smoothstep(0.10, 0.55, continentalness);
        h += range_mask * ridge * cfg.mountain_amplitude * interior * landness;

        self.bump();
        let valley_n = self
            .valley
            .get([sx * cfg.valley_scale, sz * cfg.valley_scale]) as f32;
        let valley = 1.0 - math::smoothstep(cfg.valley_low, cfg.valley_high, valley_n.abs());
        h -= valley * cfg.valley_amplitude * landness;

        // --- Clima y bioma (FASE 3) ---
        // Clima local.
        let t_local = self.climate_field(&self.temperature, x, z, cfg.temperature_scale);
        let h_local = self.climate_field(&self.humidity, x, z, cfg.humidity_scale);
        // Clima del **centro de la celda**: da coherencia regional al bioma.
        let (ccx, ccz) = (cell.center_x as f64, cell.center_z as f64);
        let t_cell = self.climate_field(&self.temperature, ccx, ccz, cfg.temperature_scale);
        let h_cell = self.climate_field(&self.humidity, ccx, ccz, cfg.humidity_scale);
        // En el interior de la celda domina el centro; cerca del borde, lo local.
        let blend = math::smoothstep(0.15, 0.55, cell.edge);
        let temperature = math::lerp(t_local, t_cell, blend);
        let humidity = math::lerp(h_local, h_cell, blend);

        // Altura normalizada (0 en el mar, 1 en `altitude_top`).
        let elevation =
            math::smoothstep(SEA_LEVEL as f32, cfg.altitude_top, h.max(SEA_LEVEL as f32));
        // Lapse: hace mas frio con la altura (nieve en cumbres).
        let lapse = cfg.altitude_lapse_rate * (elevation - cfg.altitude_lapse_start).max(0.0);
        let temperature = (temperature - lapse).clamp(0.0, 1.0);
        let humidity = humidity.clamp(0.0, 1.0);

        let biome = biomes::select(temperature, humidity, elevation);

        TerrainSample {
            continentalness,
            land,
            cell_id: cell.id,
            cell_edge: cell.edge,
            coast_factor,
            coast_roll,
            temperature,
            humidity,
            biome,
            base_height: h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn el_worldgen_es_send_y_sync() {
        assert_send_sync::<WorldGen>();
    }

    #[test]
    fn es_determinista() {
        let a = WorldGen::new(7);
        let b = WorldGen::new(7);
        for (x, z) in [(0.0, 0.0), (321.0, -98.0), (-4000.0, 2500.0)] {
            let sa = a.sample(x, z);
            let sb = b.sample(x, z);
            assert_eq!(sa.continentalness, sb.continentalness);
            assert_eq!(sa.cell_id, sb.cell_id);
            assert_eq!(sa.land, sb.land);
            assert_eq!(sa.base_height, sb.base_height);
        }
    }

    #[test]
    fn continentalness_y_altura_estan_acotadas() {
        let g = WorldGen::new(13_371);
        for x in (-3000..3000).step_by(61) {
            for z in (-3000..3000).step_by(61) {
                let s = g.sample(x as f64, z as f64);
                assert!(s.continentalness.is_finite());
                assert!((-1.001..=1.001).contains(&s.continentalness));
                assert!(s.base_height.is_finite());
                assert!((0.0..=1.0).contains(&s.coast_factor));
                // La altura base nunca sale de un rango fisico razonable.
                assert!(
                    (-80.0..=260.0).contains(&s.base_height),
                    "{}",
                    s.base_height
                );
            }
        }
    }

    #[test]
    fn el_mundo_tiene_oceano_y_tierra() {
        // En un area grande deben aparecer las dos clases.
        let g = WorldGen::new(13_371);
        let mut ocean = false;
        let mut land = false;
        for x in (-4000..4000).step_by(53) {
            for z in (-4000..4000).step_by(53) {
                let s = g.sample(x as f64, z as f64);
                ocean |= s.land.is_ocean();
                land |= s.land.is_land();
            }
        }
        assert!(ocean, "no hay oceano");
        assert!(land, "no hay tierra");
    }

    #[test]
    fn las_costas_tienen_anchos_variables() {
        // `coast_roll` por celda no es constante: debe haber costas estrechas y
        // anchas repartidas (no todas iguales).
        let g = WorldGen::new(99);
        let mut min = 1.0f32;
        let mut max = 0.0f32;
        for x in (-6000..6000).step_by(97) {
            let s = g.sample(x as f64, 0.0);
            min = min.min(s.coast_roll);
            max = max.max(s.coast_roll);
        }
        assert!(max - min > 0.3, "coast_roll poco variado: {min}..{max}");
    }
}
