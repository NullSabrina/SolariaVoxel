//! **Rios sinuosos y valles profundos** (seccion 4.6), como funciones puras.
//!
//! Reutiliza el principio de cresta de `WorldGen` (`1 - |n|`) pero con el
//! **warp propio** del rio aplicado por quien muestrea (para no repetir el ruido
//! aqui) y con profundidad que crece en **montana**: en terreno joven los cauces
//! forman valles profundos, como describe Larion.

use super::config::LarionConfig;
use crate::world::worldgen::math;

/// Caudal `[0, 1]` a partir de la humedad y de un ruido de ancho de cauce.
#[inline]
pub fn river_flow(humidity: f32, width_n: f32) -> f32 {
    (0.35 * humidity + 0.65 * width_n).clamp(0.0, 1.0)
}

/// Ancho del cauce (en unidades de cresta) para un caudal dado.
#[inline]
pub fn river_width(cfg: &LarionConfig, flow: f32) -> f32 {
    math::lerp(cfg.river_min_width, cfg.river_max_width, flow)
}

/// Proximidad `[0, 1]` al eje del cauce (1 en el centro, 0 fuera).
#[inline]
pub fn river_proximity(ridge: f32, width: f32) -> f32 {
    math::smoothstep(1.0 - width, 1.0, ridge)
}

/// Profundidad (bloques) que se resta a la altura. `mountain` en `[0, 1]`
/// profundiza el cauce en montana hasta `river_mountain_depth`.
#[inline]
pub fn river_cut(cfg: &LarionConfig, proximity: f32, flow: f32, mountain: f32) -> f32 {
    let base = math::lerp(cfg.river_min_depth, cfg.river_max_depth, flow);
    let depth = base * math::lerp(1.0, cfg.river_mountain_depth, math::saturate(mountain));
    proximity.powf(cfg.river_depth_power) * depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_cauce_es_mas_profundo_en_montana() {
        let cfg = LarionConfig::default();
        let llano = river_cut(&cfg, 1.0, 0.8, 0.0);
        let montana = river_cut(&cfg, 1.0, 0.8, 1.0);
        assert!(montana > llano);
        assert!((montana / llano - cfg.river_mountain_depth).abs() < 1e-3);
    }

    #[test]
    fn fuera_del_cauce_no_se_cava() {
        let cfg = LarionConfig::default();
        // ridge 0 = lejos del eje.
        let prox = river_proximity(0.0, cfg.river_max_width);
        assert_eq!(prox, 0.0);
        assert_eq!(river_cut(&cfg, prox, 1.0, 1.0), 0.0);
    }

    #[test]
    fn el_ancho_y_la_profundidad_crecen_con_el_caudal() {
        let cfg = LarionConfig::default();
        assert!(river_width(&cfg, 1.0) > river_width(&cfg, 0.0));
        assert!(river_cut(&cfg, 1.0, 1.0, 0.0) > river_cut(&cfg, 1.0, 0.0, 0.0));
    }

    #[test]
    fn la_proximidad_esta_acotada() {
        let cfg = LarionConfig::default();
        for i in 0..100 {
            let ridge = i as f32 / 99.0;
            let p = river_proximity(ridge, cfg.river_max_width);
            assert!((0.0..=1.0).contains(&p));
        }
    }
}
