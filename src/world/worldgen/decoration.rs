//! Decoracion por **reglas** (FASE 7): hoy solo **rocas** (boulders).
//!
//! Los arboles se movieron a [`super::trees`] (MEGA PROMPT 1, Fase D): su
//! decision es ahora una funcion pura por coordenada global con copa
//! procedimental. Aqui queda la decoracion de rocas en laderas altas, tambien
//! por reglas (condiciones sobre la muestra + pendiente + probabilidad).

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use super::TerrainSample;
use crate::world::terrain::Biome;

/// Escala del ruido de manchas de roca.
const ROCK_SCALE: f64 = 0.05;

/// Que coloca una regla.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecorationKind {
    /// Roca/boulder sobre la superficie.
    Boulder,
}

/// Una regla de decoracion. Todas las condiciones deben cumplirse.
#[derive(Clone, Copy, Debug)]
pub struct DecorationRule {
    pub name: &'static str,
    pub kind: DecorationKind,
    /// Biomas donde aplica.
    pub biomes: &'static [Biome],
    /// Rango de altura del terreno (bloques).
    pub min_height: i32,
    pub max_height: i32,
    /// Pendiente maxima (diferencia de altura con las 4 vecinas).
    pub max_slope: i32,
    /// Rango de humedad del clima `[0, 1]`.
    pub min_humidity: f32,
    pub max_humidity: f32,
    /// No aparece si la proximidad al cauce de un rio supera esto (`0` = ignora).
    pub max_river: f32,
    /// Umbral del ruido de agrupacion (manchas de roca).
    pub cluster: f32,
    /// Probabilidad por columna elegible.
    pub chance: f64,
}

/// Tabla de reglas. **El orden importa**: gana la primera que casa.
pub const RULES: &[DecorationRule] = &[DecorationRule {
    name: "mountain_boulder",
    kind: DecorationKind::Boulder,
    biomes: &[
        Biome::Tundra,
        Biome::Taiga,
        Biome::Plains,
        Biome::Savanna,
        Biome::Desert,
    ],
    min_height: 88,
    max_height: 255,
    max_slope: 2,
    min_humidity: 0.0,
    max_humidity: 1.0,
    max_river: 0.0,
    cluster: 0.15,
    chance: 0.10,
}];

/// El decorador, determinista por semilla.
pub struct Decorator {
    boulders: Fbm<Perlin>,
}

impl Decorator {
    pub fn new(seed: u32) -> Self {
        let s = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        Self {
            boulders: Fbm::<Perlin>::new(s(67))
                .set_octaves(2)
                .set_frequency(1.0)
                .set_persistence(0.5),
        }
    }

    /// Decide que decoracion va en `(wx, wz)`, o `None`. `slope` es la pendiente
    /// maxima con las 4 vecinas; `height` la altura del terreno.
    pub fn decide(
        &self,
        wx: i32,
        wz: i32,
        sample: &TerrainSample,
        height: i32,
        slope: i32,
    ) -> Option<DecorationKind> {
        for rule in RULES {
            if !rule.biomes.contains(&sample.biome) {
                continue;
            }
            if height < rule.min_height || height > rule.max_height {
                continue;
            }
            if slope > rule.max_slope {
                continue;
            }
            if sample.humidity < rule.min_humidity || sample.humidity > rule.max_humidity {
                continue;
            }
            if rule.max_river > 0.0 && sample.river_proximity > rule.max_river {
                continue;
            }
            match rule.kind {
                DecorationKind::Boulder => {
                    // Mancha de rocas: ruido propio (solo se evalua si la regla
                    // llego hasta aqui, para no pagarlo en cada columna).
                    let rock = self
                        .boulders
                        .get([wx as f64 * ROCK_SCALE, wz as f64 * ROCK_SCALE])
                        as f32;
                    if rock >= rule.cluster && hash01(wx ^ 0x1234_5678, wz) < rule.chance as f32 {
                        return Some(DecorationKind::Boulder);
                    }
                }
            }
        }
        None
    }
}

/// Hash determinista de `(x, z)` en `[0, 1)`.
fn hash01(x: i32, z: i32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((z as u32).wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h % 100_000) as f32 / 100_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldgen::{LandClass, LandformProfile, TerrainSample};

    /// Muestra sintetica para probar las reglas sin generar mundo.
    fn sample_for_test(biome: Biome, humidity: f32, height: i32) -> TerrainSample {
        TerrainSample {
            continentalness: 0.5,
            land: LandClass::Interior,
            cell_id: 0,
            cell_edge: 1.0,
            coast_factor: 0.0,
            mountain_mask: 0.5,
            coast_roll: 0.5,
            temperature: 0.5,
            humidity,
            biome,
            base_height: height as f32,
            river_proximity: 0.0,
            surface_water: 0.0,
            landform: LandformProfile::Rolling,
        }
    }

    #[test]
    fn aparecen_rocas_en_laderas_altas() {
        let d = Decorator::new(2_024);
        let high = sample_for_test(Biome::Tundra, 0.3, 130);
        let rocks = (0..120)
            .flat_map(|x| (0..120).map(move |z| (x, z)))
            .filter(|&(x, z)| d.decide(x, z, &high, 130, 1) == Some(DecorationKind::Boulder))
            .count();
        assert!(rocks > 0, "no aparecen rocas en altura");
    }

    #[test]
    fn no_hay_rocas_en_pendiente_fuerte_ni_bajo() {
        let d = Decorator::new(99);
        let high = sample_for_test(Biome::Tundra, 0.3, 130);
        let low = sample_for_test(Biome::Tundra, 0.3, 40);
        for x in 0..40 {
            for z in 0..40 {
                assert_eq!(d.decide(x, z, &high, 130, 3), None, "roca en ladera");
                assert_eq!(d.decide(x, z, &low, 40, 0), None, "roca bajo el umbral");
            }
        }
    }

    #[test]
    fn la_decoracion_es_determinista() {
        let a = Decorator::new(5);
        let b = Decorator::new(5);
        let s = sample_for_test(Biome::Tundra, 0.3, 120);
        for x in 0..40 {
            for z in 0..40 {
                assert_eq!(a.decide(x, z, &s, 120, 1), b.decide(x, z, &s, 120, 1));
            }
        }
    }
}
