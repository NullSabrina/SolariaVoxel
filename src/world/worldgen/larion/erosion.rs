//! **Campo de erosion** y relieve que este escala (seccion 4.2).
//!
//! El cambio conceptual principal del generador Larion: la erosion es un campo
//! **continuo y suave** que decide cuanta montana hay, no una mascara binaria.
//! `relief_amplitude = lerp(MAX_RELIEF, MIN_RELIEF, erosion)`: erosion alta =
//! llanura; erosion baja = montana joven. Una curva de respuesta (no un `if`)
//! reparte el detalle de baja amplitud: las llanuras tienen mas micro-relieve y
//! las montanas crestas afiladas.

use super::config::LarionConfig;
use super::spline::Spline;
use crate::world::worldgen::math;

/// Convierte el ruido de erosion `[-1, 1]` en `[0, 1]` con una transicion suave
/// en torno a 0. La erosion no satura a 0/1 en la mayoria del mundo.
#[inline]
pub fn erosion_from_raw(raw: f32) -> f32 {
    math::saturate(raw * 0.5 + 0.5)
}

/// Amplitud de relieve (bloques) para una erosion dada. Erosion 0 = montana
/// joven (amplitud maxima); erosion 1 = llanura (amplitud minima).
#[inline]
pub fn relief_amplitude(cfg: &LarionConfig, erosion: f32) -> f32 {
    math::lerp(cfg.max_relief, cfg.min_relief, math::saturate(erosion))
}

/// Curva `erosion -> ganancia de detalle`. Alta erosion (llanura) = mas detalle
/// de baja amplitud; baja erosion (montana joven) = menos, porque las crestas ya
/// dominan.
pub fn default_detail_curve() -> Spline {
    Spline::new(&[(0.0, 0.45), (0.5, 0.85), (1.0, 1.30)])
        .expect("la curva de detalle por defecto es valida")
}

/// Curva `ruido de crestas crudo ([-1,1]) -> cresta normalizada ([0,1])`.
///
/// `RidgedMulti` concentra su salida en torno a valores negativos (las crestas
/// son la cola alta), asi que un simple `r*0.5+0.5` dejaria el mundo plano. Esta
/// curva empuja la cola alta hacia 1 sin deformar la monotonia.
pub fn default_peaks_curve() -> Spline {
    Spline::new(&[
        (-1.0, 0.0),
        (-0.6, 0.04),
        (-0.2, 0.16),
        (0.2, 0.44),
        (0.6, 0.86),
        (1.0, 1.0),
    ])
    .expect("la curva de crestas por defecto es valida")
}

/// Termino de **cresta** (peaks): la parte alta del relieve. `peaks01` en
/// `[0, 1]` (cresta normalizada), ya atenuado por erosion, continentalidad y
/// tierra desde quien lo llama.
#[inline]
pub fn peak_term(cfg: &LarionConfig, peaks01: f32, relief_amp: f32) -> f32 {
    math::saturate(peaks01) * relief_amp * cfg.peaks_gain
}

/// Termino de **valle**: `(1 - |n|)^p` restado del relieve. Se atenua en las
/// crestas (`peak_attenuation` cerca de 1 apaga el valle) para que las montanas
/// no queden cortadas por un valle que las cruza.
#[inline]
pub fn valley_term(cfg: &LarionConfig, valley_raw: f32, peak_attenuation: f32, landness: f32) -> f32 {
    let v = 1.0 - math::saturate(valley_raw.abs());
    let valley = v.powf(cfg.valley_power);
    valley * cfg.valley_amplitude * landness * (1.0 - math::saturate(peak_attenuation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_erosion_es_continua_y_acotada() {
        assert_eq!(erosion_from_raw(-1.0), 0.0);
        assert_eq!(erosion_from_raw(1.0), 1.0);
        assert!((erosion_from_raw(0.0) - 0.5).abs() < 1e-6);
        let mut prev = f32::NEG_INFINITY;
        let mut x = -1.0f32;
        while x <= 1.0 {
            let e = erosion_from_raw(x);
            assert!(e >= prev - 1e-6);
            prev = e;
            x += 0.01;
        }
    }

    #[test]
    fn la_amplitud_de_relieve_baja_con_la_erosion() {
        let cfg = LarionConfig::default();
        assert!(relief_amplitude(&cfg, 0.0) > relief_amplitude(&cfg, 1.0));
        assert_eq!(relief_amplitude(&cfg, 0.0), cfg.max_relief);
        assert_eq!(relief_amplitude(&cfg, 1.0), cfg.min_relief);
    }

    #[test]
    fn la_curva_de_detalle_crece_con_la_erosion() {
        let c = default_detail_curve();
        assert!(c.eval(1.0) > c.eval(0.5));
        assert!(c.eval(0.5) > c.eval(0.0));
    }

    #[test]
    fn el_valle_se_apaga_en_las_crestas() {
        let cfg = LarionConfig::default();
        let abierto = valley_term(&cfg, 0.0, 0.0, 1.0);
        let en_cresta = valley_term(&cfg, 0.0, 1.0, 1.0);
        assert!(abierto > en_cresta);
        assert_eq!(en_cresta, 0.0);
    }
}
