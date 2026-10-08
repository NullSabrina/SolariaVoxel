//! **Altura de superficie** `H(x, z)` del generador Larion (seccion 4) y el tipo
//! [`LarionSample`] que resume una columna.
//!
//! `compose_height` es una funcion pura sobre escalares: recibe los campos ya
//! muestreados y produce la altura, la "tierra" (landness), el "interior" y la
//! amplitud de relieve. Al ser pura, se testea sin ruido.

use super::biome::BiomeBlend;
use super::config::LarionConfig;
use super::erosion;
use super::spline::Spline;
use crate::world::terrain::Biome;
use crate::world::worldgen::math;

/// Muestra geografica de una columna Larion, antes de convertirla a bloques.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LarionSample {
    /// Altura de la superficie de terreno (bloques), ya cavada por rios.
    pub height: f32,
    /// Continentalidad normalizada a `[0, 1]` (0 = oceano, 1 = interior).
    pub continentalness: f32,
    /// Continentalidad cruda en `[-1, 1]`.
    pub continental_raw: f32,
    /// Erosion en `[0, 1]` (0 = montana joven, 1 = llanura).
    pub erosion: f32,
    /// Cresta local en `[0, 1]`.
    pub peaks: f32,
    /// Temperatura efectiva `[0, 1]`.
    pub temperature: f32,
    /// Humedad efectiva `[0, 1]`.
    pub humidity: f32,
    /// Mascara de montana `[0, 1]` (cresta x tierra x poca erosion), para
    /// cuevas y decoracion.
    pub mountain: f32,
    /// Proximidad al cauce de un rio `[0, 1]`.
    pub river_proximity: f32,
    /// Nivel hasta el que llenar agua (0 = sin agua).
    pub surface_water: f32,
    /// Bioma principal seleccionado.
    pub biome: Biome,
    /// Bioma principal + secundario + peso de mezcla.
    pub blend: BiomeBlend,
    /// `true` si la columna es oceano o plataforma (para materiales costeros).
    pub ocean: bool,
}

/// Curva `continentalness ([-1,1]) -> altura (bloques)`. Cruza el nivel del mar
/// en `c = 0` (la linea de costa). El tramo cerca de 0 es empinado para que la
/// playa sea estrecha; el interior sube hasta una meseta base.
pub fn default_base_curve() -> Spline {
    Spline::new(&[
        (-1.00, 8.0),
        (-0.50, 26.0),
        (-0.15, 50.0),
        (-0.05, 64.0),
        (0.00, 72.0),
        (0.06, 78.0),
        (0.20, 88.0),
        (0.50, 100.0),
        (0.80, 112.0),
        (1.00, 124.0),
    ])
    .expect("la curva base por defecto es valida")
}

/// Entradas escalares de la composicion de altura.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeightInputs {
    /// Continentalidad cruda `[-1, 1]`.
    pub continental_raw: f32,
    /// Erosion `[0, 1]`.
    pub erosion: f32,
    /// Cresta normalizada `[0, 1]`.
    pub peaks01: f32,
    /// Relieve macro `[-1, 1]`.
    pub macro_n: f32,
    /// Ruido de valle `[-1, 1]`.
    pub valley_raw: f32,
    /// Micro-relieve `[-1, 1]`.
    pub detail: f32,
    /// Ganancia de detalle segun erosion.
    pub detail_gain: f32,
}

/// Resultado de componer la altura.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposedHeight {
    /// Altura final (bloques) acotada a `[min_height, max_height]`.
    pub height: f32,
    /// 0 en el oceano, 1 tierra adentro.
    pub landness: f32,
    /// 0 en la costa, 1 en el interior.
    pub interior: f32,
    /// Amplitud de relieve aplicada (bloques).
    pub relief_amp: f32,
}

/// Compone la altura de superficie a partir de los campos. Pura y determinista.
pub fn compose_height(cfg: &LarionConfig, base: &Spline, i: &HeightInputs) -> ComposedHeight {
    let c = i.continental_raw;
    let base_h = base.eval(c);
    let landness = math::smoothstep(-0.10, 0.22, c);
    let interior = math::smoothstep(-0.05, 0.35, c);
    let relief_amp = erosion::relief_amplitude(cfg, i.erosion);

    // Crestas: solo en tierra y hacia el interior. El interior no apaga del todo
    // (0.30..1.0) para que las montanas puedan empezar cerca de la costa.
    let peak_weight = math::saturate(i.peaks01) * interior * landness;
    let interior_gate = 0.30 + 0.70 * interior;
    let peaks = erosion::peak_term(cfg, i.peaks01, relief_amp) * interior_gate * landness;

    let macro_term = i.macro_n * cfg.macro_amplitude * landness;
    let valley = erosion::valley_term(cfg, i.valley_raw, peak_weight, landness);
    let detail = i.detail * cfg.detail_amplitude * i.detail_gain * (0.4 + 0.6 * landness);

    let h = (base_h + macro_term + peaks - valley + detail).clamp(cfg.min_height, cfg.max_height);
    ComposedHeight {
        height: h,
        landness,
        interior,
        relief_amp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> HeightInputs {
        HeightInputs {
            continental_raw: 0.5,
            erosion: 0.1,
            peaks01: 0.9,
            macro_n: 0.0,
            valley_raw: 1.0,
            detail: 0.0,
            detail_gain: 1.0,
        }
    }

    #[test]
    fn la_curva_base_cruza_el_nivel_del_mar_en_la_costa() {
        let c = default_base_curve();
        assert!((c.eval(-0.05) - 64.0).abs() < 1e-3);
        assert!(c.eval(-1.0) < 30.0);
        assert!(c.eval(1.0) > 110.0);
    }

    #[test]
    fn la_altura_esta_acotada_y_es_finita() {
        let cfg = LarionConfig::default();
        let base = default_base_curve();
        for ci in -10..=10 {
            for ei in 0..=10 {
                let mut i = inputs();
                i.continental_raw = ci as f32 / 10.0;
                i.erosion = ei as f32 / 10.0;
                let h = compose_height(&cfg, &base, &i).height;
                assert!(h.is_finite());
                assert!((cfg.min_height..=cfg.max_height).contains(&h), "{h}");
            }
        }
    }

    #[test]
    fn las_crestas_superan_los_200_en_interior_joven() {
        let cfg = LarionConfig::default();
        let base = default_base_curve();
        let mut i = inputs();
        i.continental_raw = 0.9;
        i.erosion = 0.0;
        i.peaks01 = 1.0;
        let h = compose_height(&cfg, &base, &i).height;
        assert!(h > 200.0, "cumbre joven demasiado baja: {h}");
    }

    #[test]
    fn el_oceano_queda_bajo_el_nivel_del_mar() {
        let cfg = LarionConfig::default();
        let base = default_base_curve();
        let mut i = inputs();
        i.continental_raw = -0.9;
        i.peaks01 = 0.0;
        let h = compose_height(&cfg, &base, &i).height;
        assert!(h < cfg.sea_level - 10.0, "oceano poco profundo: {h}");
        // Sin tierra, el relieve macro no debe levantarlo.
        i.macro_n = 1.0;
        let h2 = compose_height(&cfg, &base, &i).height;
        assert!((h2 - h).abs() < 1e-3);
    }
}
