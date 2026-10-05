//! Ciclo dia/noche: la hora del mundo y como afecta a la luz y al cielo.
//!
//! Es estado de escena puro (sin GPU): guarda la **hora del dia** en `[0, 1)` y
//! sabe calcular dos cosas a partir de ella:
//!
//! * [`DayCycle::day_factor`] — cuanto **apaga** la luz del sol (0 = noche,
//!   1 = pleno dia). Se multiplica por la luz de cielo de cada vertice, de modo
//!   que al anochecer el terreno se oscurece pero **las antorchas siguen
//!   brillando** (su luz de bloque no se toca).
//! * [`DayCycle::sky_color`] — el color de fondo del cielo (sRGB), que va del
//!   azul del dia al naranja del amanecer/atardecer y al azul oscuro de la
//!   noche.
//!
//! Convenio de la hora: `0.0` = medianoche, `0.25` = amanecer, `0.5` =
//! mediodia, `0.75` = atardecer.

use std::f32::consts::{FRAC_PI_2, TAU};

/// Cuanto dura un dia completo, en segundos (por defecto).
pub const DEFAULT_DAY_LENGTH: f32 = 600.0; // 10 minutos

/// Luz de cielo minima (de noche): "luz de luna". Evita que la noche quede
/// totalmente negra; el minimo de ambiente del shader (0.15) hace el resto.
pub const NIGHT_FLOOR: f32 = 0.08;

/// Color de cielo de noche (sRGB).
const NIGHT_SKY: [f32; 3] = [0.02, 0.03, 0.08];
/// Color de cielo al amanecer/atardecer (sRGB).
const SUNSET_SKY: [f32; 3] = [0.95, 0.52, 0.28];
/// Color de cielo de mediodia (sRGB).
const DAY_SKY: [f32; 3] = [0.47, 0.71, 0.97];

/// La hora del mundo y su duracion.
#[derive(Debug, Clone, Copy)]
pub struct DayCycle {
    /// Hora del dia en `[0, 1)`. Ver el convenio del modulo.
    pub time_of_day: f32,
    /// Segundos que dura un dia completo.
    pub day_length: f32,
}

impl Default for DayCycle {
    /// Empieza a media manana (0.35), con la duracion por defecto.
    fn default() -> Self {
        Self {
            time_of_day: 0.35,
            day_length: DEFAULT_DAY_LENGTH,
        }
    }
}

impl DayCycle {
    /// Crea un ciclo en la hora dada (`[0, 1)`; se envuelve si esta fuera).
    pub fn new(time_of_day: f32) -> Self {
        Self {
            time_of_day: time_of_day.rem_euclid(1.0),
            ..Default::default()
        }
    }

    /// Avanza el tiempo `dt` segundos, dando la vuelta al llegar a un dia.
    pub fn advance(&mut self, dt: f32) {
        if self.day_length > 0.0 {
            self.time_of_day = (self.time_of_day + dt / self.day_length).rem_euclid(1.0);
        }
    }

    /// Factor `0..1` que multiplica la **luz de cielo** (no la de bloque).
    ///
    /// Sigue una sinusoide: `0` (minimo, noche) en medianoche, `1` (maximo,
    /// mediodia) en `t = 0.5`, y el punto medio en amanecer/atardecer.
    pub fn day_factor(&self) -> f32 {
        // `raw` va de 0 (medianoche) a 1 (mediodia) pasando por 0.5 en los
        // crepusculos.
        let raw = ((self.time_of_day * TAU - FRAC_PI_2).sin() + 1.0) * 0.5;
        NIGHT_FLOOR + (1.0 - NIGHT_FLOOR) * raw
    }

    /// Color del cielo (sRGB, canales 0..1) para la hora actual.
    ///
    /// Interpola linealmente entre medianoche (noche), amanecer/atardecer
    /// (naranja) y mediodia (azul claro). La banda naranja es **estrecha** a
    /// proposito: el resto del dia es azul, para no quedarse en un rosa
    /// desaturado a media manana.
    pub fn sky_color(&self) -> [f32; 3] {
        // Claves (hora, color): noche, breve amanecer, dia largo, breve
        // atardecer y vuelta a la noche.
        const KEYS: [(f32, [f32; 3]); 8] = [
            (0.00, NIGHT_SKY),
            (0.22, NIGHT_SKY),
            (0.25, SUNSET_SKY),
            (0.30, DAY_SKY),
            (0.70, DAY_SKY),
            (0.75, SUNSET_SKY),
            (0.78, NIGHT_SKY),
            (1.00, NIGHT_SKY),
        ];
        let t = self.time_of_day;
        for pair in KEYS.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            if t >= t0 && t <= t1 {
                let f = if (t1 - t0).abs() < f32::EPSILON {
                    0.0
                } else {
                    (t - t0) / (t1 - t0)
                };
                return lerp3(c0, c1, f);
            }
        }
        NIGHT_SKY
    }
}

/// Interpolacion lineal componente a componente.
fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn al_mediodia_la_luz_es_maxima() {
        let noon = DayCycle::new(0.5);
        assert!((noon.day_factor() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_medianoche_la_luz_es_el_minimo_de_luna() {
        let midnight = DayCycle::new(0.0);
        assert!((midnight.day_factor() - NIGHT_FLOOR).abs() < 1e-4);
    }

    #[test]
    fn los_crepusculos_quedan_a_mitad() {
        // Amanecer y atardecer: el punto medio entre noche y dia.
        let expected = NIGHT_FLOOR + (1.0 - NIGHT_FLOOR) * 0.5;
        for t in [0.25, 0.75] {
            let c = DayCycle::new(t);
            assert!(
                (c.day_factor() - expected).abs() < 1e-4,
                "t={t} -> {}",
                c.day_factor()
            );
        }
    }

    #[test]
    fn la_luz_de_dia_siempre_supera_a_la_de_noche() {
        // El dia (t=0.5) debe iluminar mas que la noche (t=0.0).
        assert!(DayCycle::new(0.5).day_factor() > DayCycle::new(0.0).day_factor());
        // Y ningun factor sale de [NIGHT_FLOOR, 1].
        for i in 0..100 {
            let f = DayCycle::new(i as f32 / 100.0).day_factor();
            assert!((NIGHT_FLOOR..=1.0).contains(&f), "t={i} f={f}");
        }
    }

    #[test]
    fn el_color_del_cielo_coincide_en_las_claves() {
        let approx = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
        assert!(approx(DayCycle::new(0.0).sky_color(), NIGHT_SKY));
        assert!(approx(DayCycle::new(0.5).sky_color(), DAY_SKY));
        assert!(approx(DayCycle::new(0.25).sky_color(), SUNSET_SKY));
    }

    #[test]
    fn el_cielo_de_dia_es_mas_azul_que_el_de_noche() {
        let day = DayCycle::new(0.5).sky_color();
        let night = DayCycle::new(0.0).sky_color();
        // El azul del dia es mucho mas claro que el de la noche.
        assert!(day[2] > night[2] + 0.5);
    }

    #[test]
    fn avanzar_envuelve_al_cabo_de_un_dia() {
        let mut c = DayCycle::new(0.9);
        c.day_length = 10.0;
        c.advance(2.0); // +0.2 -> 1.1 -> 0.1
        assert!((c.time_of_day - 0.1).abs() < 1e-4);
    }
}
