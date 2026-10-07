//! Configuracion central del generador de mundo (FASE 1).
//!
//! Antes las constantes estaban dispersas por `terrain.rs`. Aqui viven todas las
//! escalas y umbrales en un unico sitio, con validacion, para poder calibrar el
//! generador con previews y benchmarks sin tocar 20 funciones.
//!
//! `WORLDGEN_CONFIG_VERSION` versiona la configuracion de cara a depurar y a
//! futuros guardados: un cambio de valores que altere el mundo deberia subir
//! tambien `GENERATOR_VERSION` (el resultado cambia), pero esta version permite
//! distinguir "resultado distinto por formula" de "resultado distinto por
//! parametros".

/// Version de la configuracion de worldgen.
pub const WORLDGEN_CONFIG_VERSION: u32 = 3;

/// Parametros de la generacion de mundo. Valores iniciales a calibrar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGenConfig {
    /// Distancia entre centros de celda (bioma regional), en bloques.
    pub cell_distance: f32,
    /// Jitter de los centros celulares (0 = rejilla, 1 = Worley completo).
    pub cell_jitter: f32,

    /// Valor continental por debajo del cual la celda es oceano.
    pub ocean_threshold: f32,
    /// Valor continental por debajo del cual el oceano es abisal.
    pub deep_ocean_threshold: f32,

    /// Ancho (en unidades continentales) de una costa estrecha.
    pub coast_narrow: f32,
    /// Ancho de una costa ancha.
    pub coast_wide: f32,

    /// Frecuencia del ruido continental de gran escala.
    pub continental_scale: f64,
    /// Frecuencia del detalle del ruido continental.
    pub continental_detail_scale: f64,

    /// Frecuencia de los campos de clima (temperatura y humedad).
    pub temperature_scale: f64,
    pub humidity_scale: f64,
    /// Cuanto baja la temperatura con la altura (lapse rate).
    pub altitude_lapse_rate: f32,
    /// Altura normalizada a partir de la cual empieza a notarse el lapse.
    pub altitude_lapse_start: f32,
    /// Altura (bloques) a la que la normalizacion de altitud llega a 1.
    pub altitude_top: f32,
    /// Contraste del clima: > 1 empuja temperatura/humedad a los extremos
    /// (mas desiertos, tundras y selvas; antes el clima quedaba casi todo en el
    /// centro `0.5` y los biomas frios/calidos eran raros).
    pub climate_contrast: f32,

    /// Frecuencia y amplitud del relieve macro (colinas grandes).
    pub macro_scale: f64,
    pub macro_amplitude: f32,

    /// Frecuencia y amplitud de las cordilleras.
    pub mountain_scale: f64,
    pub mountain_amplitude: f32,
    /// Exponente de la cresta (`1 - |n|`): mas alto, cumbres mas afiladas.
    pub ridge_power: f32,
    /// Umbrales de la mascara de cordillera (bajo/alto) sobre el ruido `[0,1]`.
    pub range_low: f32,
    pub range_high: f32,

    /// Frecuencia y amplitud de los valles.
    pub valley_scale: f64,
    pub valley_amplitude: f32,
    /// Umbrales del valle (bajo/alto) sobre `|n|`.
    pub valley_low: f32,
    pub valley_high: f32,

    /// Domain warping: frecuencia y fuerza (bloques) para romper la regularidad.
    pub warp_scale: f64,
    pub warp_strength: f32,

    // --- Hidrologia (FASE 5) ---
    /// Frecuencia de la cresta que define las lineas de rio.
    pub river_scale: f64,
    /// Domain warping propio del rio (trazado sinuoso).
    pub river_warp_scale: f64,
    pub river_warp_strength: f32,
    /// Ancho (en unidades de cresta) de un rio seco / caudaloso.
    pub river_min_width: f32,
    pub river_max_width: f32,
    /// Profundidad del cauce (bloques) segun caudal.
    pub river_min_depth: f32,
    pub river_max_depth: f32,
    /// Exponente del perfil del cauce (mas alto = mas estrecho y profundo).
    pub river_depth_power: f32,
    /// Lagos: frecuencia del campo de cuencas, umbral y profundidad.
    pub lake_scale: f64,
    pub lake_threshold: f32,
    pub lake_depth: f32,

    // --- Landforms (FASE 4) ---
    /// Frecuencia del ruido que reparte los landforms por region.
    pub landform_scale: f64,
    /// Altura (bloques) de los escalones de meseta / terraza / acantilado.
    pub plateau_step: f32,
    pub terrace_step: f32,
    pub cliff_step: f32,
    /// Umbral del ruido de landform por encima del cual hay terrazas.
    pub terrace_region: f32,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            cell_distance: 320.0,
            cell_jitter: 0.85,

            ocean_threshold: -0.05,
            deep_ocean_threshold: -0.38,

            coast_narrow: 0.045,
            coast_wide: 0.16,

            continental_scale: 0.00045,
            continental_detail_scale: 0.0016,

            temperature_scale: 0.004,
            humidity_scale: 0.004,
            altitude_lapse_rate: 0.16,
            altitude_lapse_start: 0.20,
            altitude_top: 180.0,
            climate_contrast: 1.5,

            macro_scale: 0.0018,
            macro_amplitude: 26.0,

            mountain_scale: 0.0026,
            mountain_amplitude: 120.0,
            ridge_power: 2.0,
            range_low: 0.46,
            range_high: 0.72,

            valley_scale: 0.0040,
            valley_amplitude: 30.0,
            valley_low: 0.06,
            valley_high: 0.22,

            warp_scale: 0.0016,
            warp_strength: 55.0,

            river_scale: 0.0013,
            river_warp_scale: 0.0007,
            river_warp_strength: 90.0,
            river_min_width: 0.030,
            river_max_width: 0.115,
            river_min_depth: 2.0,
            river_max_depth: 9.0,
            river_depth_power: 1.7,
            lake_scale: 0.0025,
            lake_threshold: 0.62,
            lake_depth: 5.0,

            landform_scale: 0.0012,
            plateau_step: 8.0,
            terrace_step: 4.0,
            cliff_step: 14.0,
            terrace_region: 0.18,
        }
    }
}

/// Error de validacion de la configuracion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    NotPositive(&'static str),
    OutOfRange(&'static str),
    Unordered(&'static str),
}

impl WorldGenConfig {
    /// Comprueba invariantes: frecuencias positivas, umbrales ordenados,
    /// probabilidades en rango. Un error claro evita mundos absurdos silenciosos.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (name, v) in [
            ("cell_distance", self.cell_distance),
            ("continental_scale", self.continental_scale as f32),
            (
                "continental_detail_scale",
                self.continental_detail_scale as f32,
            ),
            ("temperature_scale", self.temperature_scale as f32),
            ("humidity_scale", self.humidity_scale as f32),
            ("altitude_top", self.altitude_top),
            ("macro_scale", self.macro_scale as f32),
            ("mountain_scale", self.mountain_scale as f32),
            ("valley_scale", self.valley_scale as f32),
            ("warp_scale", self.warp_scale as f32),
            ("river_scale", self.river_scale as f32),
            ("river_warp_scale", self.river_warp_scale as f32),
            ("lake_scale", self.lake_scale as f32),
            ("landform_scale", self.landform_scale as f32),
            ("plateau_step", self.plateau_step),
            ("terrace_step", self.terrace_step),
            ("cliff_step", self.cliff_step),
            ("climate_contrast", self.climate_contrast),
        ] {
            if v <= 0.0 {
                return Err(ConfigError::NotPositive(name));
            }
        }
        if self.river_min_width <= 0.0 || self.river_max_width < self.river_min_width {
            return Err(ConfigError::Unordered("river_min_width <= river_max_width"));
        }
        if self.river_min_depth <= 0.0 || self.river_max_depth < self.river_min_depth {
            return Err(ConfigError::Unordered("river_min_depth <= river_max_depth"));
        }
        if !(0.0..=1.0).contains(&self.lake_threshold) {
            return Err(ConfigError::OutOfRange("lake_threshold"));
        }
        if !(0.0..=1.0).contains(&self.terrace_region) {
            return Err(ConfigError::OutOfRange("terrace_region"));
        }
        if !(0.0..=1.0).contains(&self.cell_jitter) {
            return Err(ConfigError::OutOfRange("cell_jitter"));
        }
        if !(-1.0..=0.0).contains(&self.ocean_threshold) {
            return Err(ConfigError::OutOfRange("ocean_threshold"));
        }
        if self.deep_ocean_threshold >= self.ocean_threshold {
            return Err(ConfigError::Unordered(
                "deep_ocean_threshold < ocean_threshold",
            ));
        }
        if self.coast_narrow <= 0.0 || self.coast_wide < self.coast_narrow {
            return Err(ConfigError::Unordered("coast_narrow <= coast_wide"));
        }
        if self.range_low >= self.range_high {
            return Err(ConfigError::Unordered("range_low < range_high"));
        }
        if self.valley_low >= self.valley_high {
            return Err(ConfigError::Unordered("valley_low < valley_high"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_config_por_defecto_es_valida() {
        assert_eq!(WorldGenConfig::default().validate(), Ok(()));
    }

    #[test]
    fn la_validacion_detecta_valores_absurdos() {
        let c = WorldGenConfig {
            cell_distance: 0.0,
            ..Default::default()
        };
        assert_eq!(c.validate(), Err(ConfigError::NotPositive("cell_distance")));

        let c = WorldGenConfig {
            range_low: 0.9,
            ..Default::default()
        };
        assert!(matches!(c.validate(), Err(ConfigError::Unordered(_))));

        let c = WorldGenConfig {
            ocean_threshold: 0.5,
            ..Default::default()
        };
        assert!(matches!(c.validate(), Err(ConfigError::OutOfRange(_))));
    }
}
