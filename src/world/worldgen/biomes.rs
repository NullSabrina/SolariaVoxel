//! Biomas: **definiciones** y **seleccion por scoring** (FASE 3).
//!
//! Antes el bioma salia de una cascada de `if` sobre umbrales de clima. Aqui
//! cada bioma es un [`BiomeDefinition`] con rangos preferidos de temperatura,
//! humedad y altura, y la seleccion elige el de mayor puntuacion (producto de
//! bandas suaves). Asi anadir un bioma es anadir una fila, no tocar una cascada.
//!
//! La **regionalizacion** (bioma coherente por celda) la hace `WorldGen` al
//! mezclar el clima local con el clima del **centro de la celda**: en el interior
//! de una region domina el centro (bioma unico) y cerca del borde domina el
//! clima local (transicion suave).

use super::math;
use crate::world::terrain::Biome;

/// Rango preferido `(min, max)` de un parametro normalizado `[0, 1]`.
pub type Band = (f32, f32);

/// Definicion de un bioma. Datos, no codigo: anadir un bioma es anadir una fila.
#[derive(Clone, Copy, Debug)]
pub struct BiomeDefinition {
    pub biome: Biome,
    pub name: &'static str,
    pub temperature: Band,
    pub humidity: Band,
    /// Altura **normalizada** (0 = nivel del mar, 1 = techo del terreno).
    pub elevation: Band,
    /// Densidad de arboles por columna (fraccion de columnas con arbol).
    pub tree_density: f32,
}

/// Tabla de biomas. El **orden importa** como desempate estable: gana el primero
/// con mayor puntuacion. Se ponen antes los biomas mas especificos.
pub const BIOMES: [BiomeDefinition; 7] = [
    BiomeDefinition {
        biome: Biome::Desert,
        name: "desert",
        temperature: (0.62, 1.05),
        humidity: (0.0, 0.40),
        elevation: (0.0, 0.55),
        tree_density: 0.0,
    },
    BiomeDefinition {
        biome: Biome::Tundra,
        name: "tundra",
        temperature: (-0.05, 0.28),
        humidity: (0.0, 0.55),
        elevation: (0.10, 1.10),
        tree_density: 0.0,
    },
    BiomeDefinition {
        biome: Biome::Swamp,
        name: "swamp",
        temperature: (0.40, 0.80),
        humidity: (0.70, 1.05),
        elevation: (0.0, 0.38),
        tree_density: 0.04,
    },
    BiomeDefinition {
        biome: Biome::Taiga,
        name: "taiga",
        temperature: (0.10, 0.42),
        humidity: (0.42, 0.90),
        elevation: (0.12, 0.90),
        tree_density: 0.05,
    },
    BiomeDefinition {
        biome: Biome::Savanna,
        name: "savanna",
        temperature: (0.56, 0.88),
        humidity: (0.32, 0.60),
        elevation: (0.0, 0.58),
        tree_density: 0.02,
    },
    BiomeDefinition {
        biome: Biome::Forest,
        name: "forest",
        temperature: (0.33, 0.72),
        humidity: (0.48, 0.95),
        elevation: (0.0, 0.62),
        tree_density: 0.07,
    },
    BiomeDefinition {
        biome: Biome::Plains,
        name: "plains",
        temperature: (0.30, 0.68),
        humidity: (0.25, 0.62),
        elevation: (0.0, 0.55),
        tree_density: 0.012,
    },
];

/// Definicion de un bioma (siempre existe).
pub fn definition(biome: Biome) -> &'static BiomeDefinition {
    BIOMES
        .iter()
        .find(|d| d.biome == biome)
        .expect("todo bioma tiene definicion")
}

/// Puntuacion de banda: 1 dentro del rango, cae suavemente fuera (ancho `fall`).
fn band_score(v: f32, range: Band) -> f32 {
    const FALL: f32 = 0.18;
    let (lo, hi) = range;
    if v < lo {
        math::smoothstep(lo - FALL, lo, v)
    } else if v > hi {
        1.0 - math::smoothstep(hi, hi + FALL, v)
    } else {
        1.0
    }
}

/// Selecciona el bioma de mayor puntuacion para `(temperature, humidity,
/// elevation)`, todos en `[0, 1]` (la temperatura puede pasarse un poco por el
/// lapse de altitud).
pub fn select(temperature: f32, humidity: f32, elevation: f32) -> Biome {
    let mut best = BIOMES[0].biome;
    let mut best_score = f32::NEG_INFINITY;
    for def in &BIOMES {
        let score = band_score(temperature, def.temperature)
            * band_score(humidity, def.humidity)
            * band_score(elevation, def.elevation);
        // `>` estricto: en empate gana el primero (desempate estable).
        if score > best_score {
            best_score = score;
            best = def.biome;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_bioma_tiene_definicion_y_nombre() {
        for def in &BIOMES {
            assert_eq!(definition(def.biome).biome, def.biome);
            assert!(!def.name.is_empty());
        }
    }

    #[test]
    fn la_seleccion_cubre_los_siete_biomas() {
        // Barrido de clima/altura: deben aparecer los 7 biomas.
        let mut seen = std::collections::HashSet::new();
        for ti in 0..=10 {
            for hi in 0..=10 {
                for ei in 0..=5 {
                    let t = ti as f32 / 10.0;
                    let h = hi as f32 / 10.0;
                    let e = ei as f32 / 5.0;
                    seen.insert(select(t, h, e));
                }
            }
        }
        assert_eq!(seen.len(), 7, "faltan biomas: {seen:?}");
    }

    #[test]
    fn casos_representativos() {
        assert_eq!(select(0.9, 0.1, 0.1), Biome::Desert);
        assert_eq!(select(0.05, 0.3, 0.3), Biome::Tundra);
        assert_eq!(select(0.9, 0.95, 0.1), Biome::Swamp);
        assert_eq!(select(0.5, 0.6, 0.1), Biome::Forest);
        assert_eq!(select(0.5, 0.75, 0.8), Biome::Taiga);
    }

    #[test]
    fn es_determinista() {
        for (t, h, e) in [(0.5, 0.5, 0.5), (0.1, 0.9, 0.2), (0.9, 0.2, 0.8)] {
            assert_eq!(select(t, h, e), select(t, h, e));
        }
    }
}
