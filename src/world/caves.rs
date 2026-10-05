//! Cuevas por **campo de densidad 3D** (estilo Minecraft 1.18+).
//!
//! En lugar de un solo `|perlin| < umbral`, combinamos dos "tajadas" de ruido:
//!
//! ```text
//! densidad = ruido_tuneles * 0.7 + ruido_camaras * 0.3
//! ```
//!
//! * **tuneles** — `Fbm` de frecuencia media-alta: sus valores cercanos a cero
//!   dibujan tubos largos y delgados.
//! * **camaras** — `Fbm` de frecuencia muy baja: al sumarlo, los picos abren
//!   cavernas grandes ademas de tuneles.
//!
//! La densidad se **atenua** por profundidad: ~0 junto a la corteza, maxima a
//! `FULL_DEPTH_Y` y de nuevo 0 en la bedrock. Si `densidad * atenuacion` supera
//! el umbral, la celda se cava (`Air`, o `Water` bajo el acuifero).

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Profundidad por debajo de la cual no se cava (bedrock).
pub const BEDROCK_CLEAR: i32 = 5;

/// Corteza que las cuevas **no** perforan bajo la superficie.
pub const CAVE_CRUST: i32 = 2;

/// Profundidad a la que la densidad de cueva es maxima.
const FULL_DEPTH_Y: i32 = 10;

/// Umbral de densidad para cavar. Mas alto = menos cuevas.
const DENSITY_THRESHOLD: f64 = 0.06;

/// Resultado de evaluar una celda.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Carve {
    None,
    Air,
    Water,
}

/// El sistema de cuevas, determinista por semilla.
pub struct CaveSystem {
    tunnels: Fbm<Perlin>,
    chambers: Fbm<Perlin>,
    /// Columnas solidas que subdividen las camaras.
    pillar: Fbm<Perlin>,
}

impl CaveSystem {
    pub fn new(seed: u32) -> Self {
        Self {
            tunnels: Fbm::<Perlin>::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(11))
                .set_octaves(3)
                .set_frequency(0.06)
                .set_persistence(0.5),
            chambers: Fbm::<Perlin>::new(seed.wrapping_mul(0x85EB_CA6B).wrapping_add(23))
                .set_octaves(3)
                .set_frequency(0.010)
                .set_persistence(0.5),
            pillar: Fbm::<Perlin>::new(seed.wrapping_mul(0xC2B2_AE35).wrapping_add(37))
                .set_octaves(2)
                .set_frequency(0.09)
                .set_persistence(0.5),
        }
    }

    /// Campo de densidad combinado, aproximadamente en `[-1, 1]`. Positivo =
    /// tendencia a hueco.
    fn density(&self, x: i32, y: i32, z: i32) -> f64 {
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);
        let tuneles = self.tunnels.get([fx, fy, fz]);
        let camaras = self.chambers.get([fx, fy, fz]);
        tuneles * 0.7 + camaras * 0.3
    }

    /// Atenuacion por profundidad en `0..=1`: 0 en la corteza, 1 en
    /// `FULL_DEPTH_Y` y de nuevo 0 en la bedrock. El producto de dos rampas
    /// (superior e inferior) deja intactos ambos extremos.
    fn attenuation(y: i32, surface: i32) -> f64 {
        let crust = surface - CAVE_CRUST;
        if y < BEDROCK_CLEAR || y >= crust {
            return 0.0;
        }
        let upper = ((crust - y) as f64 / (crust - FULL_DEPTH_Y).max(1) as f64).clamp(0.0, 1.0);
        let lower = ((y - BEDROCK_CLEAR) as f64 / (FULL_DEPTH_Y - BEDROCK_CLEAR).max(1) as f64)
            .clamp(0.0, 1.0);
        (upper * lower).clamp(0.0, 1.0)
    }

    /// Evalua una celda del terreno. `surface` es la altura del terreno;
    /// `aquifer` el nivel del acuifero (por debajo, la cueva nace con agua).
    pub fn carve(&self, x: i32, y: i32, z: i32, surface: i32, aquifer: i32) -> Carve {
        if y < BEDROCK_CLEAR || y >= surface - CAVE_CRUST {
            return Carve::None;
        }
        let at = Self::attenuation(y, surface);
        if at <= 0.0 {
            return Carve::None;
        }
        // La densidad debe superar el umbral tras la atenuacion.
        if self.density(x, y, z) * at <= DENSITY_THRESHOLD {
            return Carve::None;
        }
        // Pilares: columnas solidas dentro de las camaras.
        if self.pillar.get([x as f64, y as f64, z as f64]) > 0.55 {
            return Carve::None;
        }
        if y < aquifer {
            Carve::Water
        } else {
            Carve::Air
        }
    }

    /// ¿Hay cueva (seca o inundada) en la celda?
    pub fn is_cave(&self, x: i32, y: i32, z: i32, surface: i32) -> bool {
        !matches!(self.carve(x, y, z, surface, i32::MIN), Carve::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_misma_semilla_da_las_mismas_cuevas() {
        let a = CaveSystem::new(5);
        let b = CaveSystem::new(5);
        for y in 10..60 {
            assert_eq!(a.is_cave(3, y, 7, 70), b.is_cave(3, y, 7, 70), "y={y}");
        }
    }

    #[test]
    fn la_atenuacion_es_cero_en_los_extremos_y_maxima_abajo() {
        let surface = 70;
        assert_eq!(CaveSystem::attenuation(4, surface), 0.0, "bedrock");
        assert_eq!(CaveSystem::attenuation(69, surface), 0.0, "corteza");
        assert!((CaveSystem::attenuation(FULL_DEPTH_Y, surface) - 1.0).abs() < 1e-6);
        let a = CaveSystem::attenuation(60, surface);
        let b = CaveSystem::attenuation(30, surface);
        let c = CaveSystem::attenuation(15, surface);
        assert!(a < b && b < c && c <= 1.0);
    }

    #[test]
    fn hay_cuevas_pero_no_en_toda_la_columna() {
        let c = CaveSystem::new(13_371);
        let mut carved = 0;
        let total = 60 * 60 * 60;
        for x in 0..60 {
            for z in 0..60 {
                for y in 6..66 {
                    if c.is_cave(x, y, z, 70) {
                        carved += 1;
                    }
                }
            }
        }
        let frac = carved as f64 / total as f64;
        assert!(frac > 0.005, "apenas hay cuevas: {frac}");
        assert!(frac < 0.30, "demasiadas cuevas: {frac}");
    }

    #[test]
    fn no_se_cava_la_bedrock_ni_la_corteza() {
        let c = CaveSystem::new(99);
        for x in 0..40 {
            for z in 0..40 {
                for y in 0..BEDROCK_CLEAR {
                    assert_eq!(c.carve(x, y, z, 70, 50), Carve::None, "bedrock y={y}");
                }
                for y in (70 - CAVE_CRUST)..70 {
                    assert_eq!(c.carve(x, y, z, 70, 50), Carve::None, "corteza y={y}");
                }
            }
        }
    }

    #[test]
    fn las_cuevas_bajo_el_acuifero_se_llenan_de_agua() {
        let c = CaveSystem::new(7);
        let (mut agua, mut aire) = (0u32, 0u32);
        for x in 0..80 {
            for z in 0..80 {
                for y in 6..66 {
                    match c.carve(x, y, z, 70, 40) {
                        Carve::Water => {
                            assert!(y < 40, "agua por encima del acuifero: y={y}");
                            agua += 1;
                        }
                        Carve::Air => {
                            assert!(y >= 40, "aire bajo el acuifero: y={y}");
                            aire += 1;
                        }
                        Carve::None => {}
                    }
                }
            }
        }
        assert!(agua > 0, "no se genero agua de acuifero");
        assert!(aire > 0, "no se genero aire de cueva");
    }
}
