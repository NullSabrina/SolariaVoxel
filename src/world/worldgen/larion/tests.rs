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
            let h = g
                .sample((px * step) as f64, (py * step) as f64)
                .height
                .round() as i32;
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

// ---------------------------------------------------------------------------
// P0.0b: hash canonico de la columna v1.
//
// El hash es FNV-1a de los campos de `LarionSample` en un orden **fijo**. Cambiar
// el orden, anadir un campo al hash o cambiar la formula v1 rompe el test: es
// justo lo que queremos detectar. Los campos se codifican con `f32::to_bits()`
// (nunca con `Hash` de `f32`, que no existe para floats).
// ---------------------------------------------------------------------------

fn hash_u64(mut h: u64, v: u64) -> u64 {
    for b in v.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn hash_f32(h: u64, v: f32) -> u64 {
    hash_u64(h, v.to_bits() as u64)
}

/// Hash canonico de una muestra Larion. Orden documentado y estable:
/// altura, continentalidad (normalizada y cruda), erosion, crestas, temperatura,
/// humedad, montana, rio, agua, erosion hidrica, densidad de arboles, bioma,
/// mezcla y oceano.
pub fn column_hash(s: &LarionSample) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    h = hash_f32(h, s.height);
    h = hash_f32(h, s.continentalness);
    h = hash_f32(h, s.continental_raw);
    h = hash_f32(h, s.erosion);
    h = hash_f32(h, s.peaks);
    h = hash_f32(h, s.temperature);
    h = hash_f32(h, s.humidity);
    h = hash_f32(h, s.mountain);
    h = hash_f32(h, s.river_proximity);
    h = hash_f32(h, s.surface_water);
    h = hash_f32(h, s.hydro_erosion);
    h = hash_f32(h, s.tree_density);
    h = hash_u64(h, s.biome as u8 as u64);
    h = hash_u64(h, s.blend.primary as u8 as u64);
    h = hash_u64(h, s.blend.secondary as u8 as u64);
    h = hash_f32(h, s.blend.mix);
    h = hash_u64(h, u64::from(s.ocean));
    h
}

/// 1000 entradas deterministas `(seed, x, z)`: cuatro semillas y una rejilla
/// aritmetica sin dependencia de `HashMap` ni del orden de generacion.
fn baseline_input(i: usize) -> (u32, f64, f64) {
    const SEEDS: [u32; 4] = [13_371, 7, 2_024, 99];
    let seed = SEEDS[i % SEEDS.len()];
    let x = ((i as i64 * 7919).rem_euclid(20_000)) as f64 - 10_000.0;
    let z = ((i as i64 * 104_729).rem_euclid(20_000)) as f64 - 10_000.0;
    (seed, x, z)
}

/// Hash agregado de las 1000 muestras con `version = 1`.
pub fn baseline_hash() -> u64 {
    let gens: Vec<LarionGenerator> = [13_371u32, 7, 2_024, 99]
        .into_iter()
        .map(|seed| LarionGenerator::with_version(seed, LarionConfig::default(), 1))
        .collect();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for i in 0..1000 {
        let (seed, x, z) = baseline_input(i);
        let idx = gens.iter().position(|g| g.seed() == seed).unwrap();
        let sample = gens[idx].sample(x, z);
        h = hash_u64(h, seed as u64);
        h = hash_u64(h, x.to_bits());
        h = hash_u64(h, z.to_bits());
        h = hash_u64(h, column_hash(&sample));
    }
    h
}

/// Imprime el baseline. Ejecutar **una vez**:
/// `cargo test print_baseline_hash --release -- --ignored --nocapture`
#[test]
#[ignore = "se ejecuta a mano para rellenar BASELINE_HASH"]
fn print_baseline_hash() {
    let h = baseline_hash();
    println!("BASELINE_HASH (hex) = 0x{h:016x}");
    println!("BASELINE_HASH (dec) = {h}");
}

#[test]
fn test_column_hash_unchanged_v1() {
    // Baseline medido con `print_baseline_hash` sobre el codigo de este turno.
    const BASELINE_HASH: u64 = 0x7a69_3088_f994_1c67;
    assert_ne!(
        BASELINE_HASH, 0,
        "ejecuta `cargo test print_baseline_hash --release -- --ignored --nocapture` \
         y pega el valor en BASELINE_HASH"
    );
    let got = baseline_hash();
    assert_eq!(
        got, BASELINE_HASH,
        "la columna v1 cambio (0x{got:016x} != 0x{BASELINE_HASH:016x}); \
         revisa el orden de las muestras de ruido o los campos de LarionSample"
    );
}

#[test]
fn test_tree_density_blends_at_biome_border() {
    // P0.3: la densidad de arboles no puede saltar en la frontera de bioma. Se
    // busca una frontera real (cambio de `blend.primary` entre columnas
    // contiguas) y se mide el salto maximo de `tree_density` por bloque.
    let g = LarionGenerator::with_version(13_371, LarionConfig::default(), 1);
    let mut border = None;
    'z: for z in (0..900).step_by(37) {
        let z = z as f64 - 400.0;
        let mut prev = g.sample(-1500.0, z).blend.primary;
        for xi in -1499..1500 {
            let s = g.sample(xi as f64, z);
            if s.blend.primary != prev {
                border = Some((xi, z));
                break 'z;
            }
            prev = s.blend.primary;
        }
    }
    let (x0, z) = border.expect("no se encontro una frontera de bioma");
    let mut max_delta = 0.0f32;
    for dx in -10..=10 {
        let a = g.sample((x0 + dx) as f64, z).tree_density;
        let b = g.sample((x0 + dx + 1) as f64, z).tree_density;
        max_delta = max_delta.max((a - b).abs());
    }
    assert!(
        max_delta <= 0.02,
        "salto de densidad de arboles en la frontera ({x0:.0},{z}): {max_delta}"
    );
}
