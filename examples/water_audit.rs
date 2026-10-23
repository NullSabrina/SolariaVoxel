use solaria_voxel::world::worldgen::larion::{LarionConfig, LarionGenerator, WaterKind};
use solaria_voxel::world::{Block, GeneratorKind, TerrainGenerator, WORLD_HEIGHT};

fn arg(args: &[String], i: usize, default: i64) -> i64 {
    args.get(i)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = arg(&args, 1, 13_371) as u32;
    let span = arg(&args, 2, 2048).clamp(64, 65_536) as i32;
    let step = arg(&args, 3, 32).clamp(1, 512) as i32;
    let version = arg(&args, 4, 4).clamp(1, 4) as u32;

    let g = LarionGenerator::with_version(seed, LarionConfig::default(), version);
    if version < 4 {
        println!("[water] aviso: WaterKind solo existe en config >=4; usa version 4.");
    }
    let sea = g.config().sea_level;
    let mut kinds = [0u64; 5];
    let mut total = 0u64;
    let mut inland = 0u64;
    let mut inland_wet = 0u64;
    let mut desert_wet = 0u64;
    let mut dry_with_water = 0u64;
    let mut river_below_bed = 0u64;
    let mut ocean_above_sea = 0u64;
    let mut non_finite = 0u64;

    for z in (-span..span).step_by(step as usize) {
        for x in (-span..span).step_by(step as usize) {
            let s = g.sample(x as f64, z as f64);
            total += 1;
            if !s.height.is_finite() || !s.surface_water.is_finite() {
                non_finite += 1;
                continue;
            }
            let idx = match s.water_kind {
                WaterKind::None => 0,
                WaterKind::Ocean => 1,
                WaterKind::River => 2,
                WaterKind::Lake => 3,
                WaterKind::Wetland => 4,
            };
            kinds[idx] += 1;
            if s.continental_raw > 0.10 {
                inland += 1;
                if s.water_kind != WaterKind::None {
                    inland_wet += 1;
                }
            }
            if matches!(s.biome, solaria_voxel::world::Biome::Desert)
                && s.water_kind != WaterKind::None
            {
                desert_wet += 1;
            }
            if s.water_kind == WaterKind::None && s.surface_water > 0.0 {
                dry_with_water += 1;
            }
            if s.water_kind == WaterKind::River && s.surface_water < s.height - 0.51 {
                river_below_bed += 1;
            }
            if s.water_kind == WaterKind::Ocean && s.height > sea + 1.0 {
                ocean_above_sea += 1;
            }
        }
    }

    println!("[water] seed={seed} version={version} span={span} step={step} samples={total}");
    println!(
        "[water] seco {:.1}% | oceano {:.1}% | rio {:.1}% | lago {:.1}% | humedal {:.1}%",
        100.0 * kinds[0] as f64 / total as f64,
        100.0 * kinds[1] as f64 / total as f64,
        100.0 * kinds[2] as f64 / total as f64,
        100.0 * kinds[3] as f64 / total as f64,
        100.0 * kinds[4] as f64 / total as f64,
    );
    println!(
        "[water] interior={inland} mojado={inland_wet} ({:.2}%) | desierto_mojado={desert_wet}",
        100.0 * inland_wet as f64 / inland.max(1) as f64
    );
    println!(
        "[water] anomalias: seco_con_cota={dry_with_water} rio_bajo_lecho={river_below_bed} oceano_alto={ocean_above_sea} no_finito={non_finite}"
    );

    if args.get(5).map(|s| s.as_str()) == Some("3d") {
        audit_3d(seed, version);
    }
}

/// Auditoria 3D: genera columnas reales en una rejilla pequena y comprueba que
/// el agua escrita coincide con la clasificacion (no agua sobre tierra seca ni
/// por encima de la cota esperada).
fn audit_3d(seed: u32, version: u32) {
    if version < 4 {
        println!("[water-3d] requiere config 4");
        return;
    }
    let g = LarionGenerator::with_version(seed, LarionConfig::default(), version);
    let t = TerrainGenerator::with_kind_and_config(seed, GeneratorKind::Larion, version);
    let span = 64i32;
    let step = 16i32;
    let mut columns = 0u32;
    let mut with_water = 0u32;
    let mut dry_surface = 0u32;
    let mut above_level = 0u32;
    for z in (-span..span).step_by(step as usize) {
        for x in (-span..span).step_by(step as usize) {
            let s = g.sample(x as f64, z as f64);
            let c = t.generate_column(x, z);
            columns += 1;
            let mut highest = None;
            for y in 0..WORLD_HEIGHT {
                if c.get(0, y, 0) == Block::Water {
                    highest = Some(y);
                }
            }
            if let Some(y) = highest {
                with_water += 1;
                let level = (y + 1) as f32;
                if s.water_kind == WaterKind::None && level >= s.height - 0.5 {
                    dry_surface += 1;
                }
                if s.water_kind != WaterKind::None && level > s.surface_water + 1.5 {
                    above_level += 1;
                }
            }
        }
    }
    println!(
        "[water-3d] columnas={columns} con_agua={with_water} seco_con_agua={dry_surface} agua_sobre_cota={above_level}"
    );
}
