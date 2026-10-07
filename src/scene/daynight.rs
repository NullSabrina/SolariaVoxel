//! Ciclo dia/noche: la **hora del mundo** y nada mas.
//!
//! Es estado de escena puro (sin GPU). Guarda la hora del dia en `[0, 1)` y, ademas,
//! un contador de **dias de juego** (`day_count`, para las fases lunares). El
//! color del cielo, la luz y los astros ya no se calculan aqui: la **unica fuente
//! de verdad** es [`crate::scene::SkyState`], que lee la hora y deriva todo.
//!
//! Convenio de la hora: `0.0` = medianoche, `0.25` = amanecer, `0.5` = mediodia,
//! `0.75` = atardecer.
//!
//! Desde v0.30.0 se separa "que hora es" (este modulo) de "como se ve el cielo"
//! (`sky.rs`): antes el color vivia duplicado entre `DayCycle::sky_color` y el
//! shader, lo que hacia imposible afinar la atmosfera.

/// Cuanto dura un dia completo, en segundos (por defecto).
pub const DEFAULT_DAY_LENGTH: f32 = 600.0; // 10 minutos

/// La hora del mundo, su duracion y cuantos dias han pasado.
#[derive(Debug, Clone, Copy)]
pub struct DayCycle {
    /// Hora del dia en `[0, 1)`. Ver el convenio del modulo.
    pub time_of_day: f32,
    /// Segundos que dura un dia completo.
    pub day_length: f32,
    /// Dias de juego completados desde el arranque (para las fases lunares).
    pub day_count: u64,
}

impl Default for DayCycle {
    /// Empieza a media manana (0.35), con la duracion por defecto.
    fn default() -> Self {
        Self {
            time_of_day: 0.35,
            day_length: DEFAULT_DAY_LENGTH,
            day_count: 0,
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
    ///
    /// Cada vez que la hora cruza el final del dia sube `day_count`: asi la fase
    /// lunar cambia una vez por dia de juego. Se usa `div_euclid` para soportar
    /// tambien `dt` negativos (rebobinar el tiempo en pruebas).
    pub fn advance(&mut self, dt: f32) {
        if self.day_length <= 0.0 {
            return;
        }
        let raw = self.time_of_day + dt / self.day_length;
        let wraps = raw.floor();
        self.time_of_day = raw.rem_euclid(1.0);
        if wraps > 0.0 {
            self.day_count = self.day_count.wrapping_add(wraps as u64);
        } else if wraps < 0.0 {
            self.day_count = self.day_count.saturating_sub((-wraps) as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avanzar_envuelve_al_cabo_de_un_dia() {
        let mut c = DayCycle::new(0.9);
        c.day_length = 10.0;
        c.advance(2.0); // +0.2 -> 1.1 -> 0.1
        assert!((c.time_of_day - 0.1).abs() < 1e-4);
    }

    #[test]
    fn cada_vuelta_sube_el_contador_de_dias() {
        let mut c = DayCycle::new(0.9);
        c.day_length = 10.0;
        assert_eq!(c.day_count, 0);
        c.advance(2.0); // cruza el final del dia
        assert_eq!(c.day_count, 1);
        c.advance(2.0); // 0.1 -> 0.3, aun no cruza
        assert_eq!(c.day_count, 1);
        c.advance(20.0); // dos dias mas
        assert_eq!(c.day_count, 3);
    }

    #[test]
    fn un_dt_gigante_cruza_varios_dias() {
        let mut c = DayCycle::new(0.0);
        c.day_length = 10.0;
        c.advance(35.0); // 3.5 dias
        assert_eq!(c.day_count, 3);
        assert!((c.time_of_day - 0.5).abs() < 1e-4);
    }

    #[test]
    fn la_hora_nueva_envuelve() {
        assert!((DayCycle::new(1.25).time_of_day - 0.25).abs() < 1e-4);
        assert!((DayCycle::new(-0.25).time_of_day - 0.75).abs() < 1e-4);
    }
}
