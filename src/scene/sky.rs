//! Cielo y atmosfera: la **unica fuente de verdad** del color del cielo.
//!
//! A partir de la hora del mundo ([`DayCycle`]) este modulo calcula un
//! [`SkyState`] **en CPU y sin GPU**: posicion del sol y de la luna, gradiente
//! cenit <-> horizonte (que ademas depende del azimut), tinte de luz solar,
//! visibilidad de estrellas y fase lunar. De aqui salen el color del cielo, el
//! color de la **niebla** (mismo horizonte, sin costura) y el `day_factor` que ya
//! consume `scene.wgsl`. Nada de duplicar constantes en el shader.
//!
//! El gradiente se define por **keyframes de angulo solar** (grados) y se mezcla
//! en **OKLab** (evita el gris sucio entre naranja y azul) con `smoothstep`, para
//! que recorrer 24 h no de saltos.

use std::f32::consts::TAU;

use crate::math::Vec3;
use crate::math::color::{
    desaturate_srgb, mix_srgb_oklab, rgb, smoothstep, smoothstep01, srgb_to_linear3,
};

use super::DayCycle;

/// Inclinacion del plano de orbita del sol, en grados. Evita que el sol pase por
/// el cenit exacto: la elevacion maxima es `90 - tilt`.
pub const SUN_TILT_DEG: f32 = 20.0;

/// Exponente de la forma vertical del gradiente (`pow(h, EXP)`). Mas bajo = banda
/// de horizonte mas fina, como en la realidad.
pub const SKY_EXPONENT: f32 = 0.45;

/// Cuanto se conserva la saturacion del crepusculo (1 = tal cual, 0 = gris).
pub const TWILIGHT_SATURATION: f32 = 0.7;

/// Bruma del horizonte (Mie/particulas): 0 = aire limpio, 1 = muy brumoso.
pub const HAZE: f32 = 0.6;

/// Numero de fases lunares (ciclo de 8 dias de juego).
pub const MOON_PHASES: u8 = 8;

/// Giro propio de los astros por dia de juego, en grados. Hace que el cubo del
/// sol/luna muestre 2-3 caras y cambie su sombreado (se lee como cubo, no sprite).
pub const SELF_SPIN_DEG_PER_DAY: f32 = 45.0;

/// Parametros artisticos del cielo, sobreescribibles por configuracion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyParams {
    /// Inclinacion de la orbita solar, grados.
    pub sun_tilt_deg: f32,
    /// Exponente de la banda vertical cenit <-> horizonte.
    pub exponent: f32,
    /// Saturacion conservada en el crepusculo (`0..1`).
    pub twilight_saturation: f32,
    /// Bruma del horizonte (`0..1`).
    pub haze: f32,
}

impl Default for SkyParams {
    fn default() -> Self {
        Self {
            sun_tilt_deg: SUN_TILT_DEG,
            exponent: SKY_EXPONENT,
            twilight_saturation: TWILIGHT_SATURATION,
            haze: HAZE,
        }
    }
}

/// Un keyframe de la paleta: color de cenit y de horizonte (lado del sol y lado
/// opuesto) para un angulo solar dado, en **sRGB**.
struct SkyKey {
    angle: f32,
    zenith: [f32; 3],
    horizon_sun: [f32; 3],
    horizon_anti: [f32; 3],
}

/// Paleta de la tabla de direccion de arte (angulo solar -> colores sRGB).
///
/// Los keyframes van en el **punto medio de cada banda**; fuera del rango se hace
/// *clamp*. La tabla completa:
///
/// | Fase | Angulo | Cenit | Horizonte |
/// |------|--------|-------|-----------|
/// | Noche profunda | <= -18 | `#03071E` | `#0F172A` |
/// | Crep. astronomico | -15 | `#0B132B` | `#1C2541` |
/// | Crep. nautico (hora azul) | -9 | `#182851` | `#5B21B6` |
/// | Crep. civil | -3 | `#1E3A8A` | `#F97316` sol / `#EC4899` anti |
/// | Golden hour | +3 | `#3B82F6` | `#F59E0B` |
/// | Manana/tarde | +18 | `#2563EB` | `#93C5FD` |
/// | Mediodia | >= +30 | `#1D4ED8` | `#BAE6FD` |
const PALETTE: [SkyKey; 7] = [
    SkyKey {
        angle: -18.0,
        zenith: rgb(0x03071E),
        horizon_sun: rgb(0x0F172A),
        horizon_anti: rgb(0x0F172A),
    },
    SkyKey {
        angle: -15.0,
        zenith: rgb(0x0B132B),
        horizon_sun: rgb(0x1C2541),
        horizon_anti: rgb(0x1C2541),
    },
    SkyKey {
        angle: -9.0,
        zenith: rgb(0x182851),
        horizon_sun: rgb(0x5B21B6),
        horizon_anti: rgb(0x5B21B6),
    },
    SkyKey {
        angle: -3.0,
        zenith: rgb(0x1E3A8A),
        horizon_sun: rgb(0xF97316),
        horizon_anti: rgb(0xEC4899),
    },
    SkyKey {
        angle: 3.0,
        zenith: rgb(0x3B82F6),
        horizon_sun: rgb(0xF59E0B),
        horizon_anti: rgb(0xF59E0B),
    },
    SkyKey {
        angle: 18.0,
        zenith: rgb(0x2563EB),
        horizon_sun: rgb(0x93C5FD),
        horizon_anti: rgb(0x93C5FD),
    },
    SkyKey {
        angle: 30.0,
        zenith: rgb(0x1D4ED8),
        horizon_sun: rgb(0xBAE6FD),
        horizon_anti: rgb(0xBAE6FD),
    },
];

/// Blanco calido hacia el que tiende el horizonte con bruma.
const HAZE_WHITE: [f32; 3] = rgb(0xE8F0FA);
/// Color del sol bajo (cerca del horizonte), sRGB.
const SUN_LOW: [f32; 3] = rgb(0xFF9A3C);
/// Color del sol alto (mediodia, ~6500 K), sRGB.
const SUN_HIGH: [f32; 3] = rgb(0xFFF3E0);

/// Estado del cielo para una hora: todo lo que el render necesita, ya resuelto.
///
/// Los colores son **lineales** (los keyframes se convierten desde sRGB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyState {
    /// Direccion del sol (unitaria), en coordenadas de mundo.
    pub sun_dir: Vec3,
    /// Direccion de la luna (unitaria).
    pub moon_dir: Vec3,
    /// Elevacion solar en grados (negativa = bajo el horizonte).
    pub sun_elevation_deg: f32,
    /// Color del cenit (lineal).
    pub zenith: Vec3,
    /// Color del horizonte hacia el sol (lineal).
    pub horizon_sun_side: Vec3,
    /// Color del horizonte en el lado opuesto al sol (lineal).
    pub horizon_anti_side: Vec3,
    /// Bruma del horizonte (`0..1`).
    pub haze: f32,
    /// Color de la luz solar (lineal).
    pub sun_color: Vec3,
    /// Fuerza del halo solar (lobulo Mie hacia adelante).
    pub sun_glow: f32,
    /// Intensidad del Cinturon de Venus (`0..1`).
    pub belt_of_venus: f32,
    /// Visibilidad de las estrellas (`0..1`).
    pub star_visibility: f32,
    /// Giro propio de los astros (radianes), acumulado a lo largo del dia.
    pub self_spin: f32,
    /// Fase lunar (`0..MOON_PHASES`).
    pub moon_phase: u8,
    /// Cuanto se apaga la luz del sol (`0..1`); lo consume `scene.wgsl`.
    pub day_factor: f32,
}

impl SkyState {
    /// Calcula el estado del cielo para el ciclo dado (funcion **pura**).
    pub fn at(cycle: &DayCycle, params: &SkyParams) -> Self {
        let sun_dir = sun_direction(cycle.time_of_day, params.sun_tilt_deg);
        // v0.30.0: luna opuesta al sol (la fase y el giro propio llegan en v0.31).
        let moon_dir = -sun_dir;

        let elevation = sun_dir.y.clamp(-1.0, 1.0).asin().to_degrees();
        let (zenith, horizon_sun, horizon_anti) = eval_palette(elevation);

        // Control de arte: el violeta de la hora azul viene muy saturado de la
        // tabla; lo desaturamos hacia su gris de igual luminancia.
        let desat = 1.0 - params.twilight_saturation.clamp(0.0, 1.0);
        let mut horizon_sun = desaturate_srgb(horizon_sun, desat);
        let mut horizon_anti = desaturate_srgb(horizon_anti, desat);

        // Bruma: durante el dia el horizonte nunca es azul puro, tira a blanco.
        let day_amount = smoothstep(-6.0, 12.0, elevation);
        let haze_mix = params.haze.clamp(0.0, 1.0) * day_amount * 0.5;
        horizon_sun = mix_srgb_oklab(horizon_sun, HAZE_WHITE, haze_mix);
        horizon_anti = mix_srgb_oklab(horizon_anti, HAZE_WHITE, haze_mix);

        // Luz solar: naranja baja, blanca calida alta.
        let sun_high = smoothstep(0.0, 25.0, elevation);
        let sun_color = mix_srgb_oklab(SUN_LOW, SUN_HIGH, sun_high);

        let day_factor = smoothstep(-6.0, 10.0, elevation);
        let star_visibility = 1.0 - smoothstep(-14.0, -6.0, elevation);
        // Cinturon de Venus: banda rosa sobre la sombra de la Tierra, un poco
        // despues del atardecer / antes del amanecer.
        let belt_of_venus =
            smoothstep(-8.0, -3.0, elevation) * (1.0 - smoothstep(-3.0, 2.0, elevation));
        let sun_glow = smoothstep(-8.0, 2.0, elevation) * (0.5 + 0.5 * (1.0 - sun_high));

        Self {
            sun_dir,
            moon_dir,
            sun_elevation_deg: elevation,
            zenith: linear_vec(zenith),
            horizon_sun_side: linear_vec(horizon_sun),
            horizon_anti_side: linear_vec(horizon_anti),
            haze: params.haze.clamp(0.0, 1.0),
            sun_color: linear_vec(sun_color),
            sun_glow,
            belt_of_venus,
            star_visibility,
            self_spin: SELF_SPIN_DEG_PER_DAY.to_radians() * cycle.time_of_day,
            moon_phase: (cycle.day_count % MOON_PHASES as u64) as u8,
            day_factor,
        }
    }

    /// Direccion horizontal (normalizada) del sol, para mezclar el horizonte.
    pub fn sun_azimuth(&self) -> Vec3 {
        let h = Vec3::new(self.sun_dir.x, 0.0, self.sun_dir.z);
        if h.length_squared() > 1e-8 {
            h.normalize()
        } else {
            Vec3::Z
        }
    }

    /// Color del horizonte en la direccion de mirada horizontal `view_dir`.
    ///
    /// Es la funcion que comparten el **cielo** (en `h ~ 0`) y la **niebla** del
    /// terreno: al usar la misma, no hay costura entre cielo y terreno lejano.
    pub fn horizon_color(&self, view_dir: Vec3) -> Vec3 {
        let h = Vec3::new(view_dir.x, 0.0, view_dir.z);
        let sd = if h.length_squared() > 1e-8 {
            h.normalize().dot(self.sun_azimuth())
        } else {
            0.0
        };
        let w = smoothstep(-1.0, 1.0, sd);
        self.horizon_anti_side.lerp(self.horizon_sun_side, w)
    }

    /// Color del cielo en la direccion `dir` (gradiente cenit <-> horizonte).
    ///
    /// Bajo el horizonte (`dir.y < 0`) devuelve el color del horizonte, de modo
    /// que el cielo "se apoya" en la niebla sin costura.
    pub fn sample(&self, dir: Vec3, exponent: f32) -> Vec3 {
        let base = self.horizon_color(dir);
        let t = smoothstep01(dir.y.max(0.0).powf(exponent));
        base.lerp(self.zenith, t)
    }
}

/// Direccion del sol para `time_of_day` (`[0,1)`, 0 = medianoche) y una
/// inclinacion en grados.
///
/// `theta = 2pi (t - 0.25)`: en `t = 0.25` el sol sale por `+X`, en `t = 0.5`
/// esta en lo mas alto y en `t = 0.75` se pone por `-X`.
pub fn sun_direction(time_of_day: f32, tilt_deg: f32) -> Vec3 {
    let theta = TAU * (time_of_day - 0.25);
    let tilt = tilt_deg.to_radians();
    Vec3::new(
        theta.cos(),
        theta.sin() * tilt.cos(),
        theta.sin() * tilt.sin(),
    )
    .normalize()
}

/// Convierte un color sRGB a `Vec3` lineal.
#[inline]
fn linear_vec(c: [f32; 3]) -> Vec3 {
    let l = srgb_to_linear3(c);
    Vec3::new(l[0], l[1], l[2])
}

/// Evalua la paleta a un angulo solar (grados), con `smoothstep` en OKLab.
fn eval_palette(angle: f32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let first = &PALETTE[0];
    let last = &PALETTE[PALETTE.len() - 1];
    if angle <= first.angle {
        return (first.zenith, first.horizon_sun, first.horizon_anti);
    }
    if angle >= last.angle {
        return (last.zenith, last.horizon_sun, last.horizon_anti);
    }
    for w in PALETTE.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if angle >= a.angle && angle <= b.angle {
            let t = smoothstep01((angle - a.angle) / (b.angle - a.angle));
            return (
                mix_srgb_oklab(a.zenith, b.zenith, t),
                mix_srgb_oklab(a.horizon_sun, b.horizon_sun, t),
                mix_srgb_oklab(a.horizon_anti, b.horizon_anti, t),
            );
        }
    }
    (first.zenith, first.horizon_sun, first.horizon_anti)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle(t: f32) -> DayCycle {
        DayCycle::new(t)
    }

    fn state(t: f32) -> SkyState {
        SkyState::at(&cycle(t), &SkyParams::default())
    }

    #[test]
    fn el_sol_sale_por_mas_x_y_se_pone_por_menos_x() {
        let rising = state(0.25).sun_dir;
        assert!((rising.x - 1.0).abs() < 1e-4, "amanecer x={}", rising.x);
        assert!(rising.y.abs() < 1e-4);
        let setting = state(0.75).sun_dir;
        assert!((setting.x + 1.0).abs() < 1e-4, "atardecer x={}", setting.x);
    }

    #[test]
    fn a_medianoche_el_sol_esta_bajo_y_al_mediodia_alto() {
        assert!(state(0.0).sun_dir.y < -0.9);
        // Elevacion maxima = 90 - tilt.
        let noon = state(0.5);
        assert!((noon.sun_elevation_deg - (90.0 - SUN_TILT_DEG)).abs() < 0.5);
    }

    #[test]
    fn la_luna_es_opuesta_al_sol() {
        for i in 0..8 {
            let s = state(i as f32 / 8.0);
            assert!((s.moon_dir + s.sun_dir).length() < 1e-4);
        }
    }

    #[test]
    fn los_keyframes_coinciden_en_sus_angulos() {
        // En el punto medio exacto de una banda, la paleta devuelve su color.
        for k in &PALETTE {
            let (z, hs, ha) = eval_palette(k.angle);
            for i in 0..3 {
                assert!((z[i] - k.zenith[i]).abs() < 1e-4);
                assert!((hs[i] - k.horizon_sun[i]).abs() < 1e-4);
                assert!((ha[i] - k.horizon_anti[i]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn la_paleta_es_continua() {
        let mut prev = eval_palette(-30.0);
        let mut a = -30.0;
        while a <= 45.0 {
            let cur = eval_palette(a);
            for i in 0..3 {
                assert!(
                    (cur.0[i] - prev.0[i]).abs() < 0.02,
                    "cenit salta en {a}: {prev:?} -> {cur:?}"
                );
                assert!(
                    (cur.1[i] - prev.1[i]).abs() < 0.02,
                    "horizonte salta en {a}"
                );
            }
            prev = cur;
            a += 0.1;
        }
    }

    #[test]
    fn el_factor_dia_crece_en_el_amanecer() {
        let mut prev = -1.0;
        let mut t = 0.20;
        while t <= 0.35 {
            let f = state(t).day_factor;
            assert!(f >= prev - 1e-5, "day_factor baja en t={t}");
            prev = f;
            t += 0.005;
        }
        assert!(state(0.5).day_factor > 0.99);
        assert!(state(0.0).day_factor < 0.01);
    }

    #[test]
    fn las_estrellas_solo_se_ven_de_noche() {
        assert!(state(0.0).star_visibility > 0.99);
        assert!(state(0.5).star_visibility < 0.01);
        assert!(state(0.25).star_visibility < 0.6);
    }

    #[test]
    fn la_fase_lunar_cicla_cada_ocho_dias() {
        for day in 0..24u64 {
            let mut c = cycle(0.5);
            c.day_count = day;
            let s = SkyState::at(&c, &SkyParams::default());
            assert_eq!(s.moon_phase, (day % 8) as u8);
        }
    }

    #[test]
    fn el_cinturon_de_venus_solo_en_su_ventana() {
        assert!(state(0.5).belt_of_venus < 0.01); // mediodia
        assert!(state(0.0).belt_of_venus < 0.01); // noche
        // Justo antes del amanecer (elevacion ~ -3) debe haber cinturon.
        let mut found = false;
        let mut t = 0.20;
        while t <= 0.25 {
            if state(t).belt_of_venus > 0.5 {
                found = true;
            }
            t += 0.002;
        }
        assert!(found, "no se encontro el Cinturon de Venus en el amanecer");
    }

    #[test]
    fn el_horizonte_cambia_con_el_azimut() {
        let s = state(0.24); // crepusculo
        let toward = s.horizon_color(s.sun_azimuth());
        let away = s.horizon_color(-s.sun_azimuth());
        assert!(
            (toward - away).length() > 0.02,
            "el horizonte deberia diferir sol vs anti-sol"
        );
    }

    #[test]
    fn los_astros_giran_lentamente_a_lo_largo_del_dia() {
        let dawn = state(0.25).self_spin;
        let noon = state(0.5).self_spin;
        assert!((noon - dawn).abs() > 1e-3, "el giro propio debe avanzar");
        // Medio dia de juego son la mitad de SELF_SPIN_DEG_PER_DAY.
        let expected = SELF_SPIN_DEG_PER_DAY.to_radians() * 0.25;
        assert!((noon - dawn - expected).abs() < 1e-4);
    }

    #[test]
    fn el_sample_bajo_el_horizonte_usa_el_horizonte() {
        let s = state(0.5);
        let below = s.sample(Vec3::new(0.0, -0.5, -0.866), SKY_EXPONENT);
        let horizon = s.horizon_color(Vec3::new(0.0, 0.0, -1.0));
        assert!((below - horizon).length() < 1e-5);
    }
}
