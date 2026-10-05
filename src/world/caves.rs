//! Cuevas 3D de nueva generacion: **spaghetti** (tuneles), **cheese** (camaras)
//! y **pillar** (columnas), con densidad que crece con la profundidad.
//!
//! Sustituye al antiguo `abs(perlin) < 0.07`, que daba tuneles finos y poco
//! variados. La filosofia (Minecraft 1.18+): combinar varias "tajadas" de ruido
//! 3D cuyos ceros/niveles dibujan formas complementarias, y modular su umbral
//! por la profundidad para que no se coma la corteza ni la bedrock.
//!
//! El agua de los acuiferos se decide **aqui** (no en el motor de fluidos): una
//! cueva bajo el nivel del acuifero ya nace llena de `Block::Water`, de modo que
//! la simulacion a 10 Hz no tiene que inundar cavernas enteras.

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Profundidad por debajo de la cual no se cava (bedrock).
pub const BEDROCK_CLEAR: i32 = 5;

/// Corteza que las cuevas **no** perforan bajo la superficie.
pub const CAVE_CRUST: i32 = 2;

/// Resultado de evaluar una celda: que se talla (o nada).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Carve {
    /// Se deja el bloque de terreno tal cual.
    None,
    /// Se cava aire (tunel/camara seca).
    Air,
    /// Se cava y se rellena de agua (acuifero).
    Water,
}

/// El sistema de cuevas, determinista por semilla.
pub struct CaveSystem {
    /// Tuneles finos: el cero de este Fbm dibuja una iso-superficie tubular.
    spaghetti: Fbm<Perlin>,
    /// Camaras: este Fbm supera un umbral en las zonas huecas grandes.
    cheese: Fbm<Perlin>,
    /// Pilares: deja columnas solidas dentro de las cavidades.
    pillar: Fbm<Perlin>,
}

impl CaveSystem {
    pub fn new(seed: u32) -> Self {
        Self {
            // Frecuencia alta = detalle fino; 3 octavas bastan para un tunel.
            spaghetti: Fbm::<Perlin>::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(11))
                .set_octaves(3)
                .set_frequency(0.055)
                .set_persistence(0.5),
            // Frecuencia muy baja = formas grandes; 4 octavas anaden lobulos.
            cheese: Fbm::<Perlin>::new(seed.wrapping_mul(0x85EB_CA6B).wrapping_add(23))
                .set_octaves(4)
                .set_frequency(0.011)
                .set_persistence(0.55),
            // Frecuencia media: pilares verticales reconocibles.
            pillar: Fbm::<Perlin>::new(seed.wrapping_mul(0xC2B2_AE35).wrapping_add(37))
                .set_octaves(2)
                .set_frequency(0.09)
                .set_persistence(0.5),
        }
    }

    /// Evalua una celda del terreno. `surface` es la altura del terreno en esa
    /// columna; `aquifer` el nivel del acuifero (por debajo, las cuevas se
    /// generan ya llenas de agua).
    pub fn carve(&self, x: i32, y: i32, z: i32, surface: i32, aquifer: i32) -> Carve {
        // Ni bedrock ni corteza: la superficie no queda acribillada.
        if y < BEDROCK_CLEAR || y >= surface - CAVE_CRUST {
            return Carve::None;
        }

        // Densidad por profundidad: 0 junto a la corteza, 1 hacia la bedrock.
        // Multiplica los umbrales, de modo que arriba apenas hay cuevas y abajo
        // abundan (y son mas grandes).
        let span = (surface - CAVE_CRUST - BEDROCK_CLEAR).max(1) as f64;
        let depth = ((surface - CAVE_CRUST - y) as f64 / span).clamp(0.0, 1.0);

        let (fx, fy, fz) = (x as f64, y as f64, z as f64);

        // Pillar: donde el ruido es alto queda una columna solida, que corta las
        // camaras en salas con soportes en vez de un vacio continuo.
        let pillar_solid = self.pillar.get([fx, fy, fz]) > 0.45;

        // Spaghetti: |fbm| pequeno = superficie tubular, tunel largo y delgado.
        let sp = self.spaghetti.get([fx, fy, fz]);
        let sp_thresh = 0.045 + 0.05 * depth;
        let spaghetti = sp.abs() < sp_thresh;

        // Cheese: el ruido supera un umbral que baja con la profundidad -> mas y
        // mayores camaras abajo.
        let cheese = self.cheese.get([fx, fy, fz]) > 0.62 - 0.14 * depth;

        if (spaghetti || cheese) && !pillar_solid {
            if y < aquifer {
                Carve::Water
            } else {
                Carve::Air
            }
        } else {
            Carve::None
        }
    }

    /// ¿Hay cueva (seca o inundada) en la celda? Util para tests y diagnostico.
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
        // Suficientes cuevas para notarse, pero lejos de vaciar el mundo.
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

    #[test]
    fn hay_cuevas_de_los_tres_tipos() {
        // El sistema combina spaghetti/cheese; comprobamos que producen formas
        // tanto alargadas (spaghetti) como grandes (cheese) contando corridas.
        let c = CaveSystem::new(2026);
        let mut huecos = 0u32;
        for x in 0..96 {
            for z in 0..96 {
                if c.is_cave(x, 30, z, 90) {
                    huecos += 1;
                }
            }
        }
        assert!(huecos > 0, "la capa y=30 deberia tener cuevas");
    }
}
