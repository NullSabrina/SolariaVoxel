//! **Selector multi-parametrico de biomas** (seccion 3.2): distancia ponderada
//! en el espacio de [`ClimatePoint`], con el vecino mas cercano y el segundo.
//!
//! Sustituye el Voronoi de celdas (que producia bordes poligonales visibles en
//! las fronteras de bioma) por algo **continuo**: en la frontera entre dos nodos
//! la distancia a ambos es parecida, asi que el peso de mezcla cambia de forma
//! suave. `BiomeBlend` permite que el material haga una transicion organica (con
//! un hash global) en vez de un cambio de bloque en una linea recta.
//!
//! Los nodos son **datos** (`DEFAULT_BIOME_NODES`), no codigo.

use crate::world::terrain::Biome;

use super::climate::ClimatePoint;

/// Un nodo del mapa multi-parametrico: el punto "ideal" de un bioma, con su
/// peso de influencia por eje.
#[derive(Clone, Copy, Debug)]
pub struct BiomeNode {
    /// Bioma asociado.
    pub biome: Biome,
    /// Punto de referencia en el espacio `[temperature, humidity,
    /// continentalness, erosion, peaks]`.
    pub target: [f32; 5],
    /// Escala por parametro: cuanto pesa cada eje en la distancia.
    pub weights: [f32; 5],
}

/// Resultado de seleccionar bioma en un punto: el principal y su vecino, con un
/// peso de mezcla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeBlend {
    /// Bioma mas cercano.
    pub primary: Biome,
    /// Segundo bioma mas cercano.
    pub secondary: Biome,
    /// 0 = solo `primary`, 1 = solo `secondary`. Cerca 0.5 en la frontera.
    pub mix: f32,
}

/// Peso comun a todos los biomas: temperatura y humedad mandan (bandas
/// climaticas), continentalidad matiza, erosion y relieve desempatan.
const BASE_WEIGHTS: [f32; 5] = [1.0, 1.0, 0.55, 0.60, 0.30];

/// Pesos de config 4: el bioma lo decide **el clima**, no el relieve. Asi una
/// montana dentro de un bosque sigue siendo bosque; la erosion y los picos solo
/// desempatan. Evita los parches de tundra/taiga por altura.
/// Biomas que deben ganar area (desierto, llanura, pantano, tundra).
const WEIGHTS_COMMON: [f32; 5] = [0.70, 0.70, 0.14, 0.14, 0.07];
/// Bosque y taiga: area media.
const WEIGHTS_MID: [f32; 5] = [1.10, 1.10, 0.22, 0.22, 0.11];
/// Sabana: el clima calido-seco es muy amplio; se estrecha para no comerse el
/// resto de biomas.
const WEIGHTS_DOMINANT: [f32; 5] = [1.20, 1.20, 0.24, 0.24, 0.12];

/// Nodos de config 4: objetivos recolocados sobre la distribucion real de
/// clima (poca humedad extrema, temperatura en U por la banda latitudinal).
/// Buscan que los siete biomas aparezcan con frecuencia parecida.
pub const DEFAULT_BIOME_NODES_V4: [BiomeNode; 7] = [
    BiomeNode {
        biome: Biome::Desert,
        target: [0.80, 0.34, 0.66, 0.82, 0.25],
        weights: WEIGHTS_COMMON,
    },
    BiomeNode {
        biome: Biome::Savanna,
        target: [0.78, 0.52, 0.72, 0.70, 0.35],
        weights: WEIGHTS_DOMINANT,
    },
    BiomeNode {
        biome: Biome::Plains,
        target: [0.55, 0.52, 0.72, 0.92, 0.22],
        weights: WEIGHTS_COMMON,
    },
    BiomeNode {
        biome: Biome::Forest,
        target: [0.50, 0.74, 0.84, 0.42, 0.50],
        weights: WEIGHTS_MID,
    },
    BiomeNode {
        biome: Biome::Swamp,
        target: [0.55, 0.86, 0.56, 1.00, 0.12],
        weights: WEIGHTS_COMMON,
    },
    BiomeNode {
        biome: Biome::Taiga,
        target: [0.22, 0.60, 0.86, 0.44, 0.62],
        weights: WEIGHTS_MID,
    },
    BiomeNode {
        biome: Biome::Tundra,
        target: [0.12, 0.42, 0.80, 0.28, 0.58],
        weights: WEIGHTS_COMMON,
    },
];

/// Nodos por defecto: un punto por bioma, repartidos por el espacio de
/// parametros. Son datos calibrables.
pub const DEFAULT_BIOME_NODES: [BiomeNode; 7] = [
    BiomeNode {
        biome: Biome::Desert,
        target: [0.82, 0.20, 0.66, 0.82, 0.25],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Savanna,
        target: [0.76, 0.38, 0.72, 0.70, 0.35],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Plains,
        target: [0.56, 0.46, 0.72, 0.92, 0.22],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Forest,
        target: [0.52, 0.76, 0.84, 0.42, 0.50],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Swamp,
        target: [0.60, 0.94, 0.56, 1.00, 0.12],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Taiga,
        target: [0.18, 0.54, 0.86, 0.44, 0.62],
        weights: BASE_WEIGHTS,
    },
    BiomeNode {
        biome: Biome::Tundra,
        target: [0.06, 0.32, 0.80, 0.28, 0.58],
        weights: BASE_WEIGHTS,
    },
];

/// Altura por bioma (config 4): pantanos bajos y planos, llanuras/sabanas mas
/// planas, desierto suavemente aplanado. Se mezcla con el bioma secundario para
/// que la transicion no tenga un escalon.
pub fn height_modifier(blend: BiomeBlend, height: f32, sea_level: f32) -> f32 {
    let primary = height_for_biome(blend.primary, height, sea_level);
    let secondary = height_for_biome(blend.secondary, height, sea_level);
    let t = (blend.mix * 2.0).clamp(0.0, 1.0);
    primary + (secondary - primary) * t
}

fn height_for_biome(biome: Biome, height: f32, sea_level: f32) -> f32 {
    match biome {
        // Pantano: pegado al nivel del mar y aplanado (charcas y orillas).
        Biome::Swamp => sea_level + 1.0 + (height - sea_level).clamp(-1.0, 14.0) * 0.55,
        // Llanura: relieve suave.
        Biome::Plains => sea_level + (height - sea_level) * 0.80,
        // Sabana: algo mas plana que el bosque.
        Biome::Savanna => sea_level + (height - sea_level) * 0.85,
        // Desierto: dunas bajas, sin grandes cuestas.
        Biome::Desert => sea_level + (height - sea_level) * 0.92,
        // Bosque y biomas frios conservan su relieve.
        Biome::Forest | Biome::Taiga | Biome::Tundra => height,
    }
}

/// Selector de biomas: distancia ponderada O(N) con N ~ 8-12 nodos. Barato y
/// sin tablas precalculadas.
pub struct BiomeSelector {
    nodes: Box<[BiomeNode]>,
}

impl BiomeSelector {
    /// Construye el selector con una lista explicita de nodos.
    pub fn new(nodes: impl Into<Box<[BiomeNode]>>) -> Self {
        Self {
            nodes: nodes.into(),
        }
    }

    /// Selector con los nodos por defecto (siete biomas del mundo).
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BIOME_NODES)
    }

    /// Selector de config 4: nodos recolocados al clima real y peso mucho mayor
    /// del clima frente al relieve (biomas amplios y coherentes).
    pub fn with_defaults_v4() -> Self {
        Self::new(DEFAULT_BIOME_NODES_V4)
    }

    /// Numero de nodos.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// ¿Sin nodos? (nunca con los defaults, pero pedido por clippy).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Devuelve el bioma principal, el secundario y el peso de mezcla para `p`.
    pub fn select(&self, p: &ClimatePoint) -> BiomeBlend {
        let v = p.to_array();
        let mut best_d = f32::INFINITY;
        let mut second_d = f32::INFINITY;
        let mut best = Biome::Plains;
        let mut second = Biome::Plains;

        for n in self.nodes.iter() {
            let mut d = 0.0f32;
            for ((vi, ti), wi) in v.iter().zip(n.target.iter()).zip(n.weights.iter()) {
                let e = vi - ti;
                d += wi * e * e;
            }
            if d < best_d {
                second_d = best_d;
                second = best;
                best_d = d;
                best = n.biome;
            } else if d < second_d {
                second_d = d;
                second = n.biome;
            }
        }

        let total = best_d + second_d;
        let mix = if total > 0.0 { best_d / total } else { 0.0 };
        BiomeBlend {
            primary: best,
            secondary: second,
            mix,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(t: f32, h: f32, c: f32, e: f32, p: f32) -> ClimatePoint {
        ClimatePoint {
            continentalness: c,
            erosion: e,
            peaks: p,
            temperature: t,
            humidity: h,
        }
    }

    #[test]
    fn el_modificador_de_altura_aplana_llanura_y_baja_pantano() {
        let sea = 64.0;
        let plains = BiomeBlend {
            primary: Biome::Plains,
            secondary: Biome::Plains,
            mix: 0.0,
        };
        let swamp = BiomeBlend {
            primary: Biome::Swamp,
            secondary: Biome::Swamp,
            mix: 0.0,
        };
        let forest = BiomeBlend {
            primary: Biome::Forest,
            secondary: Biome::Forest,
            mix: 0.0,
        };
        assert!(height_modifier(plains, 100.0, sea) < 100.0);
        assert!(height_modifier(swamp, 80.0, sea) < 80.0);
        assert_eq!(height_modifier(forest, 100.0, sea), 100.0);
        // La mezcla entre dos biomas queda entre sus dos alturas.
        let mixed = BiomeBlend {
            primary: Biome::Swamp,
            secondary: Biome::Forest,
            mix: 0.25,
        };
        let a = height_modifier(swamp, 80.0, sea);
        let b = height_modifier(forest, 80.0, sea);
        let m = height_modifier(mixed, 80.0, sea);
        assert!(m > a.min(b) && m < a.max(b));
    }

    #[test]
    fn casos_representativos() {
        let s = BiomeSelector::with_defaults();
        assert_eq!(
            s.select(&point(0.9, 0.1, 0.7, 0.8, 0.2)).primary,
            Biome::Desert
        );
        assert_eq!(
            s.select(&point(0.6, 0.95, 0.5, 1.0, 0.1)).primary,
            Biome::Swamp
        );
        assert_eq!(
            s.select(&point(0.05, 0.35, 0.8, 0.3, 0.6)).primary,
            Biome::Tundra
        );
        assert_eq!(
            s.select(&point(0.55, 0.75, 0.85, 0.4, 0.5)).primary,
            Biome::Forest
        );
    }

    #[test]
    fn es_determinista_y_continuo() {
        let s = BiomeSelector::with_defaults();
        let a = s.select(&point(0.5, 0.5, 0.6, 0.6, 0.4));
        let b = s.select(&point(0.5, 0.5, 0.6, 0.6, 0.4));
        assert_eq!(a, b);
        // Un paso minusculo en el clima no puede cambiar el bioma principal de
        // golpe (la distancia es continua).
        let c = s.select(&point(0.501, 0.5, 0.6, 0.6, 0.4));
        assert_eq!(a.primary, c.primary);
    }

    #[test]
    fn el_mix_esta_en_rango() {
        let s = BiomeSelector::with_defaults();
        for i in 0..40 {
            for j in 0..40 {
                let t = i as f32 / 39.0;
                let h = j as f32 / 39.0;
                let b = s.select(&point(t, h, 0.6, 0.5, 0.4));
                assert!(
                    (0.0..=0.5).contains(&b.mix),
                    "mix fuera de rango: {}",
                    b.mix
                );
            }
        }
    }

    #[test]
    fn cubre_los_siete_biomas() {
        let s = BiomeSelector::with_defaults();
        let mut seen = std::collections::HashSet::new();
        for ti in 0..=12 {
            for hi in 0..=12 {
                for ei in 0..=3 {
                    let t = ti as f32 / 12.0;
                    let h = hi as f32 / 12.0;
                    let e = ei as f32 / 3.0;
                    seen.insert(s.select(&point(t, h, 0.7, e, 0.4)).primary);
                }
            }
        }
        assert_eq!(seen.len(), 7, "faltan biomas: {seen:?}");
    }
}
