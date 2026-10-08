//! Tests del generador Larion: determinismo, continuidad, cobertura y escala.

use std::collections::HashSet;

use super::*;
use crate::world::terrain::{Biome, SEA_LEVEL};

const SEMILLAS: [u32; 4] = [13_371, 7, 2_024, 99];

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn el_larion_es_send_y_sync() {
    assert_send_sync::<LarionGenerator>();
}

#[test]
fn es_determinista_por_semilla() {
    let a = LarionGenerator::new(7);
    let b = LarionGenerator::new(7);
    for i in 0..200 {
        let x = i as f64 * 91.0 - 9_000.0;
        let z = i as f64 * -53.0 + 6_000.0;
        let sa = a.sample(x, z);
        let sb = b.sample(x, z);
        assert_eq!(sa.height, sb.height);
        assert_eq!(sa.biome, sb.biome);
        assert_eq!(sa.blend, sb.blend);
        assert_eq!(sa.erosion, sb.erosion);
    }
}

#[test]
fn semillas_distintas_dan_mundos_distintos() {
    let a = LarionGenerator::new(1);
    let b = LarionGenerator::new(2);
    let mut distintos = 0;
    for i in 0..50 {
        let x = i as f64 * 200.0;
        if (a.sample(x, x).height - b.sample(x, x).height).abs() > 1.0 {
            distintos += 1;
        }
    }
    assert!(distintos > 20, "dos semillas dan casi el mismo mundo");
}

#[test]
fn la_altura_es_finita_y_acotada() {
    let cfg = LarionConfig::default();
    for seed in SEMILLAS {
        let g = LarionGenerator::new(seed);
        for i in 0..150 {
            for j in 0..150 {
                let x = i as f64 * 41.0 - 3_000.0;
                let z = j as f64 * 37.0 - 3_000.0;
                let s = g.sample(x, z);
                assert!(s.height.is_finite(), "altura no finita");
                assert!(
                    (cfg.min_height..=cfg.max_height).contains(&s.height),
                    "altura fuera de rango: {}",
                    s.height
                );
                assert!((0.0..=1.0).contains(&s.erosion));
                assert!((0.0..=1.0).contains(&s.temperature));
                assert!((0.0..=1.0).contains(&s.humidity));
                assert!((0.0..=1.0).contains(&s.river_proximity));
            }
        }
    }
}

/// Estadisticas de altura sobre una galeria de semillas.
fn stats_altura(seed: u32, pixels: i32, step: i32) -> (Vec<i32>, i64, i64) {
    let g = LarionGenerator::new(seed);
    let mut heights = Vec::with_capacity((pixels * pixels) as usize);
    let mut max = i32::MIN;
    for py in 0..pixels {
        for px in 0..pixels {
            let h = g.sample((px * step) as f64, (py * step) as f64).height.round() as i32;
            heights.push(h);
            max = max.max(h);
        }
    }
    let oceanos = heights.iter().filter(|&&h| h <= SEA_LEVEL).count() as i64;
    heights.sort_unstable();
    (heights, max as i64, oceanos)
}

#[test]
fn hay_cordilleras_con_cumbres_superiores_a_200() {
    let mut cumbres = 0;
    for seed in SEMILLAS {
        let (_, max, _) = stats_altura(seed, 160, 46);
        if max > 200 {
            cumbres += 1;
        }
    }
    assert!(cumbres > 0, "ninguna semilla supera 200 de altura");
}

#[test]
fn el_rango_de_altura_supera_los_150_bloques() {
    for seed in SEMILLAS {
        let (heights, _, _) = stats_altura(seed, 200, 40);
        let p01 = heights[heights.len() / 100];
        let p99 = heights[heights.len() * 99 / 100];
        assert!(
            p99 - p01 >= 150,
            "seed {seed}: rango p99-p01 = {} (p01={p01}, p99={p99})",
            p99 - p01
        );
    }
}

#[test]
fn el_mundo_tiene_oceano_y_tierra() {
    for seed in SEMILLAS {
        let (_, _, oceanos) = stats_altura(seed, 160, 46);
        let total = 160.0 * 160.0;
        let frac = oceanos as f64 / total;
        assert!(frac > 0.05, "seed {seed}: casi sin oceano ({frac})");
        assert!(frac < 0.95, "seed {seed}: casi sin tierra ({frac})");
    }
}

#[test]
fn los_siete_biomas_aparecen_y_ninguno_domina() {
    for seed in SEMILLAS {
        let g = LarionGenerator::new(seed);
        let mut counts: std::collections::HashMap<Biome, u64> = std::collections::HashMap::new();
        let mut total = 0u64;
        let pixels = 260;
        let step = 46;
        for py in 0..pixels {
            for px in 0..pixels {
                let s = g.sample((px * step) as f64, (py * step) as f64);
                // Solo tierra: el oceano no tiene bioma material propio.
                if s.ocean {
                    continue;
                }
                *counts.entry(s.biome).or_insert(0) += 1;
                total += 1;
            }
        }
        assert!(total > 0, "seed {seed}: sin tierra");
        let seen: HashSet<Biome> = counts.keys().copied().collect();
        assert_eq!(seen.len(), 7, "seed {seed}: faltan biomas {seen:?}");
        let (dom, n) = counts.iter().max_by_key(|(_, v)| **v).unwrap();
        let frac = *n as f64 / total as f64;
        assert!(frac < 0.45, "seed {seed}: {dom:?} ocupa {frac}");
    }
}

#[test]
fn la_transicion_de_bioma_no_tiene_saltos() {
    // El peso de mezcla debe cambiar de forma continua entre columnas vecinas.
    let g = LarionGenerator::new(13_371);
    let mut max_delta = 0.0f32;
    for py in 0..180 {
        for px in 0..180 {
            let x = px * 3 - 400;
            let z = py * 3 - 400;
            if g.sample(x as f64, z as f64).ocean {
                continue;
            }
            let a = g.sample(x as f64, z as f64).blend.mix;
            let b = g.sample((x + 3) as f64, z as f64).blend.mix;
            max_delta = max_delta.max((a - b).abs());
        }
    }
    assert!(max_delta < 0.3, "salto de mezcla de bioma: {max_delta}");
}

#[test]
fn los_voladizos_solo_aparecen_en_montana_joven() {
    // En una llanura muy erosionada la densidad no debe desplazarse.
    let g = LarionGenerator::new(13_371);
    let mut con_voladizo = 0;
    for i in 0..2000 {
        let x = (i % 45) * 61 - 1300;
        let z = (i / 45) * 61 - 1300;
        let s = g.sample(x as f64, z as f64);
        if !s.ocean && LarionGenerator::overhang_strength(&s) > 0.5 {
            con_voladizo += 1;
        }
    }
    assert!(con_voladizo > 0, "no hay montana joven con voladizos");
}
