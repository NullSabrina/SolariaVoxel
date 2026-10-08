//! **Configuracion versionada** del generador Larion.
//!
//! Todos los numeros de la seccion 4 del prompt viven aqui, no como constantes
//! sueltas: asi se calibran desde un unico sitio y se validan antes de generar
//! un mundo. `LARION_CONFIG_VERSION` permite distinguir un mundo distinto por
//! *parametros* de uno distinto por *formula* (lo que versiona
//! `GENERATOR_VERSION` en `world/save.rs`).

/// Version de la configuracion del generador Larion.
pub const LARION_CONFIG_VERSION: u32 = 1;

/// Parametros del pipeline Larion. Los valores por defecto son el punto de
/// partida calibrado con `examples/larion_preview.rs` (galeria de semillas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LarionConfig {
    // --- Continentalidad y macro-escala (4.1) ---
    /// Frecuencia del ruido continental de gran escala (1/bloques). Escala de
    /// continentes de ~2000-6000 bloques.
    pub continental_scale: f64,
    /// Octavas del campo continental.
    pub continental_octaves: usize,
    /// Frecuencia del domain warping horizontal (1/bloques).
    pub continental_warp_scale: f64,
    /// Fuerza del domain warping horizontal (bloques). Solo X y Z.
    pub continental_warp_strength: f64,

    // --- Erosion (4.2) ---
    /// Frecuencia del campo de erosion (1/bloques).
    pub erosion_scale: f64,
    /// Octavas del campo de erosion.
    pub erosion_octaves: usize,

    // --- Meso-escala: crestas, valles, macro y detalle (4.3 / 4.4) ---
    /// Frecuencia del ruido de crestas (1/bloques).
    pub peaks_scale: f64,
    /// Octavas del ruido de crestas.
    pub peaks_octaves: usize,
    /// Frecuencia del campo de valles (1/bloques).
    pub valley_scale: f64,
    /// Frecuencia del relieve macro (colinas grandes) (1/bloques).
    pub macro_scale: f64,
    /// Amplitud del relieve macro (bloques).
    pub macro_amplitude: f32,
    /// Amplitud de relieve con erosion 0 (montana joven), en bloques.
    pub max_relief: f32,
    /// Amplitud de relieve con erosion 1 (llanura muy erosionada), en bloques.
    pub min_relief: f32,
    /// Ganancia de las crestas dentro del relieve.
    pub peaks_gain: f32,
    /// Amplitud de los valles (bloques).
    pub valley_amplitude: f32,
    /// Exponente del perfil de valle (`(1-|n|)^p`): mas alto, mas estrecho.
    pub valley_power: f32,
    /// Frecuencia del micro-relieve (1/bloques).
    pub detail_scale: f64,
    /// Amplitud del micro-relieve (bloques).
    pub detail_amplitude: f32,
    /// Octavas del micro-relieve.
    pub detail_octaves: usize,

    // --- Densidad 3D en banda (4.3) ---
    /// Semi-ancho (bloques) de la banda vertical alrededor de `H` donde se
    /// evalua el ruido 3D. Fuera, la densidad es solo la altura 2D.
    pub density_band: f32,
    /// Frecuencia del ruido 3D de voladizos (1/bloques).
    pub density_scale: f64,
    /// Octavas del ruido 3D.
    pub density_octaves: usize,
    /// Amplitud del desplazamiento vertical del ruido 3D (bloques).
    pub overhang_amplitude: f32,

    // --- Clima en bandas (4.5) ---
    /// Ancho de banda latitudinal (bloques) a lo largo de Z.
    pub latitude_band: f64,
    /// Frecuencia del ruido de temperatura (1/bloques).
    pub temperature_scale: f64,
    /// Octavas del ruido de temperatura.
    pub temperature_octaves: usize,
    /// Frecuencia del ruido de humedad (1/bloques).
    pub humidity_scale: f64,
    /// Octavas del ruido de humedad.
    pub humidity_octaves: usize,
    /// Contraste climatico (>1 empuja a los extremos).
    pub climate_contrast: f32,
    /// Enfriamiento por altura (lapse rate), en fraccion de temperatura por
    /// bloque normalizado de altura.
    pub altitude_lapse_rate: f32,
    /// Sesgo de humedad cerca de la costa (mas humedo), 0..1.
    pub coast_humidity_bias: f32,

    // --- Rios (4.6) ---
    /// Frecuencia de la cresta de rio (1/bloques).
    pub river_scale: f64,
    /// Frecuencia del domain warping propio del rio (1/bloques).
    pub river_warp_scale: f64,
    /// Fuerza del warp del rio (bloques).
    pub river_warp_strength: f32,
    /// Ancho (en unidades de cresta) de un rio seco / caudaloso.
    pub river_min_width: f32,
    pub river_max_width: f32,
    /// Profundidad del cauce (bloques) segun caudal.
    pub river_min_depth: f32,
    pub river_max_depth: f32,
    /// Exponente del perfil del cauce.
    pub river_depth_power: f32,
    /// Factor de profundizacion del cauce en montana (hasta este multiplo).
    pub river_mountain_depth: f32,

    // --- Escala vertical y superficie (5 / 7) ---
    /// Nivel del mar (bloques). Igual que `terrain::SEA_LEVEL`.
    pub sea_level: f32,
    /// Fondo minimo del terreno (bloques).
    pub min_height: f32,
    /// Techo del terreno Larion (bloques). Sustituye el clamp 200 del legacy,
    /// que se conserva intacto para compatibilidad de mundos.
    pub max_height: f32,
    /// Pendiente (diferencia con las 4 vecinas) a partir de la cual aflora roca.
    pub rock_slope: f32,
    /// Temperatura por debajo de la cual la cima alta se cubre de nieve.
    pub snow_temperature: f32,
    /// Borde inferior de la ventana de mezcla de bioma.
    pub biome_blend_lo: f32,
    /// Borde superior de la ventana de mezcla de bioma.
    pub biome_blend_hi: f32,
}

impl Default for LarionConfig {
    fn default() -> Self {
        Self {
            continental_scale: 1.0 / 4000.0,
            continental_octaves: 4,
            continental_warp_scale: 1.0 / 1500.0,
            continental_warp_strength: 520.0,

            erosion_scale: 1.0 / 1200.0,
            erosion_octaves: 3,

            peaks_scale: 1.0 / 380.0,
            peaks_octaves: 5,
            valley_scale: 1.0 / 700.0,
            macro_scale: 1.0 / 1800.0,
            macro_amplitude: 24.0,
            max_relief: 340.0,
            min_relief: 10.0,
            peaks_gain: 1.0,
            valley_amplitude: 36.0,
            valley_power: 2.0,
            detail_scale: 1.0 / 18.0,
            detail_amplitude: 2.4,
            detail_octaves: 2,

            density_band: 16.0,
            density_scale: 1.0 / 90.0,
            density_octaves: 1,
            overhang_amplitude: 13.0,

            latitude_band: 7000.0,
            temperature_scale: 1.0 / 1500.0,
            temperature_octaves: 3,
            humidity_scale: 1.0 / 1200.0,
            humidity_octaves: 3,
            climate_contrast: 1.8,
            altitude_lapse_rate: 0.42,
            coast_humidity_bias: 0.14,

            river_scale: 1.0 / 1300.0,
            river_warp_scale: 1.0 / 900.0,
            river_warp_strength: 150.0,
            river_min_width: 0.028,
            river_max_width: 0.115,
            river_min_depth: 2.0,
            river_max_depth: 9.0,
            river_depth_power: 1.7,
            river_mountain_depth: 2.5,

            sea_level: 64.0,
            min_height: 4.0,
            max_height: 300.0,
            rock_slope: 3.0,
            snow_temperature: 0.30,
            biome_blend_lo: 0.35,
            biome_blend_hi: 0.65,
        }
    }
}

/// Error de validacion de [`LarionConfig`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LarionConfigError {
    /// Un parametro que debe ser positivo no lo es.
    NotPositive(&'static str),
    /// Un rango esta desordenado (min >= max).
    Unordered(&'static str),
    /// Una probabilidad/valor normalizado se sale de `[0, 1]`.
    OutOfRange(&'static str),
}

impl std::fmt::Display for LarionConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LarionConfigError::NotPositive(n) => write!(f, "{n} debe ser positivo"),
            LarionConfigError::Unordered(n) => write!(f, "{n} debe estar ordenado"),
            LarionConfigError::OutOfRange(n) => write!(f, "{n} se sale de rango"),
        }
    }
}

impl std::error::Error for LarionConfigError {}

impl LarionConfig {
    /// Comprueba invariantes: frecuencias positivas, rangos ordenados y
    /// probabilidades en `[0, 1]`. Un error claro evita mundos absurdos.
    pub fn validate(&self) -> Result<(), LarionConfigError> {
        for (name, v) in [
            ("continental_scale", self.continental_scale),
            ("continental_warp_scale", self.continental_warp_scale),
            ("erosion_scale", self.erosion_scale),
            ("peaks_scale", self.peaks_scale),
            ("valley_scale", self.valley_scale),
            ("macro_scale", self.macro_scale),
            ("detail_scale", self.detail_scale),
            ("density_scale", self.density_scale),
            ("temperature_scale", self.temperature_scale),
            ("humidity_scale", self.humidity_scale),
            ("latitude_band", self.latitude_band),
        ] {
            if v <= 0.0 || !v.is_finite() {
                return Err(LarionConfigError::NotPositive(name));
            }
        }
        for (name, v) in [
            ("continentality_warps", self.continental_warp_strength),
            ("macro_amplitude", self.macro_amplitude as f64),
            ("max_relief", self.max_relief as f64),
            ("min_relief", self.min_relief as f64),
            ("peaks_gain", self.peaks_gain as f64),
            ("valley_power", self.valley_power as f64),
            ("detail_amplitude", self.detail_amplitude as f64),
            ("density_band", self.density_band as f64),
            ("overhang_amplitude", self.overhang_amplitude as f64),
            ("altitude_lapse_rate", self.altitude_lapse_rate as f64),
            ("river_scale", self.river_scale),
            ("river_warp_scale", self.river_warp_scale),
        ] {
            if !v.is_finite() {
                return Err(LarionConfigError::NotPositive(name));
            }
        }
        // Amplitudes estrictamente positivas.
        for (name, v) in [
            ("macro_amplitude", self.macro_amplitude),
            ("max_relief", self.max_relief),
            ("min_relief", self.min_relief),
            ("peaks_gain", self.peaks_gain),
            ("valley_power", self.valley_power),
            ("detail_amplitude", self.detail_amplitude),
            ("density_band", self.density_band),
            ("overhang_amplitude", self.overhang_amplitude),
            ("altitude_lapse_rate", self.altitude_lapse_rate),
        ] {
            if v <= 0.0 {
                return Err(LarionConfigError::NotPositive(name));
            }
        }

        if self.min_relief >= self.max_relief {
            return Err(LarionConfigError::Unordered("min_relief < max_relief"));
        }
        if self.min_height >= self.max_height {
            return Err(LarionConfigError::Unordered("min_height < max_height"));
        }
        if self.sea_level <= self.min_height || self.sea_level >= self.max_height {
            return Err(LarionConfigError::OutOfRange("sea_level"));
        }
        if self.river_min_width <= 0.0 || self.river_max_width < self.river_min_width {
            return Err(LarionConfigError::Unordered(
                "river_min_width <= river_max_width",
            ));
        }
        if self.river_min_depth <= 0.0 || self.river_max_depth < self.river_min_depth {
            return Err(LarionConfigError::Unordered(
                "river_min_depth <= river_max_depth",
            ));
        }
        if self.river_mountain_depth < 1.0 {
            return Err(LarionConfigError::Unordered("river_mountain_depth >= 1"));
        }
        if self.climate_contrast <= 0.0 || !self.climate_contrast.is_finite() {
            return Err(LarionConfigError::OutOfRange("climate_contrast"));
        }
        if !(0.0..=1.0).contains(&self.coast_humidity_bias) {
            return Err(LarionConfigError::OutOfRange("coast_humidity_bias"));
        }
        if !(0.0..=1.0).contains(&self.snow_temperature) {
            return Err(LarionConfigError::OutOfRange("snow_temperature"));
        }
        if !(0.0..=1.0).contains(&self.biome_blend_lo)
            || !(0.0..=1.0).contains(&self.biome_blend_hi)
            || self.biome_blend_lo >= self.biome_blend_hi
        {
            return Err(LarionConfigError::Unordered("biome_blend_lo < biome_blend_hi"));
        }
        if self.rock_slope <= 0.0 {
            return Err(LarionConfigError::NotPositive("rock_slope"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_config_por_defecto_es_valida() {
        assert_eq!(LarionConfig::default().validate(), Ok(()));
    }

    #[test]
    fn la_validacion_detecta_valores_absurdos() {
        let c = LarionConfig {
            continental_scale: -1.0,
            ..Default::default()
        };
        assert!(matches!(
            c.validate(),
            Err(LarionConfigError::NotPositive("continental_scale"))
        ));

        let c = LarionConfig {
            min_relief: 500.0,
            ..Default::default()
        };
        assert!(matches!(c.validate(), Err(LarionConfigError::Unordered(_))));

        let c = LarionConfig {
            river_max_depth: 1.0,
            ..Default::default()
        };
        assert!(matches!(c.validate(), Err(LarionConfigError::Unordered(_))));

        let c = LarionConfig {
            coast_humidity_bias: 2.0,
            ..Default::default()
        };
        assert!(matches!(c.validate(), Err(LarionConfigError::OutOfRange(_))));
    }
}
