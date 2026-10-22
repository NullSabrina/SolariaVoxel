//! Metrica de **conectividad de cuevas** (P1.4).
//!
//! Genera una region con `CaveSystem::new_with_worms` (v2), marca las celdas
//! cavadas y hace un flood-fill 6-conectado. Reporta:
//! * fraccion de celdas cavadas en la componente mayor (objetivo >= 70%),
//! * fraccion de celdas en componentes aisladas (< 32 celdas, objetivo < 10%).
//!
//! Uso:
//! ```text
//! cargo run --release --example cave_connectivity
//! cargo run --release --example cave_connectivity -- 128 64
//! ```

use std::collections::VecDeque;

use solaria_voxel::world::caves::{Carve, CaveSystem};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let span: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(96);
    let y_span: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(56);

    let x0 = -span / 2;
    let z0 = -span / 2;
    let y0 = 8;
    let w = span as usize;
    let d = span as usize;
    let h = y_span as usize;
    let total = w * d * h;

    let with_worms = args.get(3).map(|s| s != "v1").unwrap_or(true);
    let mode = if with_worms { "v2+worms" } else { "v1" };
    let caves = if with_worms {
        CaveSystem::new_with_worms(13_371)
    } else {
        CaveSystem::new(13_371)
    };
    let mut carved = vec![false; total];
    let mut carved_count = 0usize;
    for zi in 0..d {
        for xi in 0..w {
            let x = x0 + xi as i32;
            let z = z0 + zi as i32;
            let ctx = caves.context(x, z, 0.0);
            for yi in 0..h {
                let y = y0 + yi as i32;
                if !matches!(caves.carve(&ctx, x, y, z, 70, i32::MIN), Carve::None) {
                    let i = (zi * w + xi) * h + yi;
                    carved[i] = true;
                    carved_count += 1;
                }
            }
        }
    }

    let idx = |xi: usize, zi: usize, yi: usize| (zi * w + xi) * h + yi;
    let mut seen = vec![false; total];
    let mut components: Vec<usize> = Vec::new();
    let mut queue = VecDeque::new();
    for zi in 0..d {
        for xi in 0..w {
            for yi in 0..h {
                let start = idx(xi, zi, yi);
                if !carved[start] || seen[start] {
                    continue;
                }
                seen[start] = true;
                queue.push_back((xi, zi, yi));
                let mut size = 0usize;
                while let Some((cx, cz, cy)) = queue.pop_front() {
                    size += 1;
                    let neigh = [
                        (cx.wrapping_sub(1), cz, cy),
                        (cx + 1, cz, cy),
                        (cx, cz.wrapping_sub(1), cy),
                        (cx, cz + 1, cy),
                        (cx, cz, cy.wrapping_sub(1)),
                        (cx, cz, cy + 1),
                    ];
                    for (nx, nz, ny) in neigh {
                        if nx >= w || nz >= d || ny >= h {
                            continue;
                        }
                        let ni = idx(nx, nz, ny);
                        if carved[ni] && !seen[ni] {
                            seen[ni] = true;
                            queue.push_back((nx, nz, ny));
                        }
                    }
                }
                components.push(size);
            }
        }
    }

    components.sort_unstable_by(|a, b| b.cmp(a));
    let largest = components.first().copied().unwrap_or(0);
    let isolated: usize = components.iter().filter(|&&s| s < 32).sum();
    let largest_frac = largest as f64 / carved_count.max(1) as f64;
    let isolated_frac = isolated as f64 / carved_count.max(1) as f64;

    println!(
        "[caves] {mode} region {w}x{d}x{h}: {carved_count} celdas cavadas ({:.2}%), {} componentes",
        carved_count as f64 / total as f64 * 100.0,
        components.len()
    );
    println!(
        "[caves] componente mayor: {largest} celdas ({:.1}%), aisladas <32: {isolated} ({:.1}%)",
        largest_frac * 100.0,
        isolated_frac * 100.0
    );
    println!(
        "[caves] top componentes: {:?}",
        &components[..components.len().min(10)]
    );

    if largest_frac < 0.70 || isolated_frac > 0.10 {
        eprintln!("[caves] FALLO: se exige >=70% en la mayor y <10% aisladas");
        std::process::exit(1);
    }
}
