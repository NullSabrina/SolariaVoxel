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
    fn casos_representativos() {
        let s = BiomeSelector::with_defaults();
        assert_eq!(s.select(&point(0.9, 0.1, 0.7, 0.8, 0.2)).primary, Biome::Desert);
        assert_eq!(s.select(&point(0.6, 0.95, 0.5, 1.0, 0.1)).primary, Biome::Swamp);
        assert_eq!(s.select(&point(0.05, 0.35, 0.8, 0.3, 0.6)).primary, Biome::Tundra);
        assert_eq!(s.select(&point(0.55, 0.75, 0.85, 0.4, 0.5)).primary, Biome::Forest);
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
                assert!((0.0..=0.5).contains(&b.mix), "mix fuera de rango: {}", b.mix);
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
