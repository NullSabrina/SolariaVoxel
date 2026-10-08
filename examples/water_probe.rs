//! Sonda de **agua** (MEGA PROMPT 2, Fase A): reproduce escenarios y mide.
//!
//! Sin GPU ni mundo cargado: implementa [`FluidGrid`] en memoria y corre el
//! **mismo** `step_cell` + cola que `World::tick_water_with` (pop de a una celda,
//! re-encolar los 6 vecinos solo si cambio). Sirve para confirmar o descartar los
//! problemas de la seccion 2 del prompt con numeros antes de tocar la logica.
//!
//! Uso:
//! ```text
//! cargo run --release --example water_probe
//! ```

use std::collections::{HashMap, HashSet};
use std::io::BufWriter;

use solaria_voxel::world::water::{
    DirtyQueue, Fluid, FluidGrid, MAX_LEVEL, neighborhood, step_cell,
};

/// Rejilla en memoria: suelo plano solido y paredes opcionales.
struct ProbeGrid {
    cells: HashMap<[i32; 3], Fluid>,
    walls: HashSet<[i32; 3]>,
    floor_y: i32,
    radius: i32,
    y_max: i32,
}

impl ProbeGrid {
    fn new(floor_y: i32) -> Self {
        Self {
            cells: HashMap::new(),
            walls: HashSet::new(),
            floor_y,
            radius: 96,
            y_max: 64,
        }
    }

    fn set(&mut self, p: [i32; 3], f: Fluid) {
        if f == Fluid::None {
            self.cells.remove(&p);
        } else {
            self.cells.insert(p, f);
        }
    }

    fn wall(&mut self, p: [i32; 3]) {
        self.walls.insert(p);
    }

    fn total(&self) -> usize {
        self.cells.len()
    }

    fn level_at(&self, p: [i32; 3]) -> u8 {
        self.fluid(p).level()
    }

    /// Celdas de agua a la altura `y`.
    fn at_height(&self, y: i32) -> usize {
        self.cells.keys().filter(|p| p[1] == y).count()
    }

    /// Celdas de agua a la altura `y` **fuera** de la columna del tronco.
    fn lateral_at_height(&self, y: i32) -> usize {
        self.cells
            .keys()
            .filter(|p| p[1] == y && (p[0] != 0 || p[2] != 0))
            .count()
    }

    fn sum(&self) -> u32 {
        self.cells.values().map(|f| f.level() as u32).sum()
    }
}

impl FluidGrid for ProbeGrid {
    fn in_bounds(&self, p: [i32; 3]) -> bool {
        p[1] > self.floor_y
            && p[1] < self.y_max
            && p[0].abs() < self.radius
            && p[2].abs() < self.radius
    }
    fn is_solid(&self, p: [i32; 3]) -> bool {
        p[1] <= self.floor_y || self.walls.contains(&p)
    }
    fn fluid(&self, p: [i32; 3]) -> Fluid {
        *self.cells.get(&p).unwrap_or(&Fluid::None)
    }
    fn set_fluid(&mut self, p: [i32; 3], f: Fluid) {
        self.set(p, f);
    }
}

/// Corre la simulacion como `tick_water_with` hasta vaciar la cola (o `max_ticks`).
/// Devuelve `(ticks, celdas_procesadas, pendientes_finales)`.
fn settle(grid: &mut ProbeGrid, budget: usize, max_ticks: usize) -> (usize, usize, usize) {
    let mut q = DirtyQueue::new();
    let seed: Vec<[i32; 3]> = grid.cells.keys().copied().collect();
    for p in seed {
        q.push(p);
    }
    let mut ticks = 0;
    let mut processed = 0;
    while !q.is_empty() && ticks < max_ticks {
        let mut n = 0;
        while n < budget {
            let Some(p) = q.pop() else { break };
            n += 1;
            processed += 1;
            if step_cell(grid, p) {
                for nb in neighborhood(p) {
                    q.push(nb);
                }
            }
        }
        ticks += 1;
    }
    (ticks, processed, q.len())
}

/// Presupuesto de celdas por tick (el real: `FluidBudget::default().cells`).
const BUDGET: usize = 16_384;

fn main() {
    println!("water_probe (Fase A): modelo fuente->distancia, tick de a una celda\n");
    escenario_fuente_sobre_suelo();
    escenario_cascada();
    escenario_lago_cerrado();
    escenario_retroceso();
    escenario_dique();
    escenario_2x2();
}

/// 2.1: una fuente en y=10 sobre suelo plano. El bug esperado: inunda un volumen.
fn escenario_fuente_sobre_suelo() {
    let mut g = ProbeGrid::new(0);
    g.set([0, 10, 0], Fluid::Source);
    let (ticks, processed, pending) = settle(&mut g, BUDGET, 5_000);
    let axis: Vec<String> = (0..=MAX_LEVEL as i32 + 1)
        .map(|d| format!("{}", g.level_at([d, 1, 0])))
        .collect();
    println!("== Fuente en y=10 sobre suelo plano ==");
    println!(
        "  celdas totales={}  suma_niveles={}  ticks={}  procesadas={}  pendientes={}",
        g.total(),
        g.sum(),
        ticks,
        processed,
        pending
    );
    println!("  nivel en y=1 a lo largo de +X (d=0..9): {}", axis.join(","));
    let mut per_y = String::new();
    for y in 1..=10 {
        per_y.push_str(&format!(" y{y}:{}", g.at_height(y)));
    }
    println!("  celdas por altura:{per_y}");
    write_levels_png("screenshots/water_probe_fuente.png", &g, 1, 16);
    println!();
}

/// "Captura" data-driven (sin GPU): mapa cenital de niveles en la capa `y`,
/// en escala de grises (0 = sin agua, 255 = nivel 8). Muestra el charco y su
/// decaimiento.
fn write_levels_png(path: &str, g: &ProbeGrid, y: i32, half: i32) {
    let n = (2 * half + 1) as u32;
    let mut img = vec![0u8; (n * n * 4) as usize];
    for pz in -half..=half {
        for px in -half..=half {
            let v = (g.level_at([px, y, pz]) as f32 / MAX_LEVEL as f32 * 255.0) as u8;
            let i = (((pz + half) as u32 * n + (px + half) as u32) * 4) as usize;
            img[i] = v;
            img[i + 1] = v;
            img[i + 2] = v;
            img[i + 3] = 255;
        }
    }
    std::fs::create_dir_all("screenshots").ok();
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  no se pudo crear {path}: {e}");
            return;
        }
    };
    let mut enc = png::Encoder::new(BufWriter::new(file), n, n);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    match enc.write_header() {
        Ok(mut w) => {
            if let Err(e) = w.write_image_data(&img) {
                eprintln!("  PNG data: {e}");
            }
        }
        Err(e) => eprintln!("  PNG header: {e}"),
    }
}

/// 2.1: cascada de 6 bloques. El bug esperado: agua lateral a cada altura.
fn escenario_cascada() {
    let mut g = ProbeGrid::new(0);
    g.set([0, 6, 0], Fluid::Source);
    let (ticks, processed, pending) = settle(&mut g, BUDGET, 5_000);
    println!("== Cascada de 6 (fuente en y=6) ==");
    println!(
        "  celdas totales={}  ticks={}  procesadas={}  pendientes={}",
        g.total(),
        ticks,
        processed,
        pending
    );
    let mut per_y = String::new();
    for y in 1..=6 {
        per_y.push_str(&format!(
            " y{y}: tot={} lat={}",
            g.at_height(y),
            g.lateral_at_height(y)
        ));
    }
    println!("  por altura (tot=celdas, lat=fuera del tronco):{per_y}");
    println!("  nivel en y=1 a lo largo de +X: {}", {
        let a: Vec<String> = (0..=MAX_LEVEL as i32 + 1)
            .map(|d| format!("{}", g.level_at([d, 1, 0])))
            .collect();
        a.join(",")
    });
    println!();
}

/// Lago cerrado: cuenca 5x5 con paredes. Mide celdas y ticks al equilibrio.
fn escenario_lago_cerrado() {
    let mut g = ProbeGrid::new(0);
    // Paredes de un cuenco 5x5 (interior 3x3) de altura 3.
    for y in 1..=3 {
        for x in -2..=2 {
            g.wall([x, y, -2]);
            g.wall([x, y, 2]);
        }
        for z in -2..=2 {
            g.wall([-2, y, z]);
            g.wall([2, y, z]);
        }
    }
    g.set([0, 1, 0], Fluid::Source);
    let (ticks, processed, pending) = settle(&mut g, BUDGET, 5_000);
    println!("== Lago cerrado (cuenca 5x5, fuente en el centro) ==");
    println!(
        "  celdas totales={}  suma_niveles={}  ticks={}  procesadas={}  pendientes={}",
        g.total(),
        g.sum(),
        ticks,
        processed,
        pending
    );
    println!("  celdas por altura: y1:{} y2:{} y3:{}", g.at_height(1), g.at_height(2), g.at_height(3));
    println!();
}

/// Retroceso: se quita la fuente y el agua debe secarse en ticks acotados.
fn escenario_retroceso() {
    let mut g = ProbeGrid::new(0);
    g.set([0, 1, 0], Fluid::Source);
    let (ticks_full, _, _) = settle(&mut g, BUDGET, 5_000);
    let after_full = g.total();
    // Quitar la fuente.
    g.set([0, 1, 0], Fluid::None);
    let (ticks_dry, processed, pending) = settle(&mut g, BUDGET, 5_000);
    println!("== Retroceso (quitar la fuente) ==");
    println!(
        "  tras llenar: celdas={after_full} (ticks={ticks_full})"
    );
    println!(
        "  tras quitar la fuente: celdas={}  ticks={}  procesadas={}  pendientes={}",
        g.total(),
        ticks_dry,
        processed,
        pending
    );
    println!();
}

/// Dique que se rompe: un deposito retenido por una pared que se retira.
fn escenario_dique() {
    let mut g = ProbeGrid::new(0);
    // Deposito: pared en x=3, agua en x=0..2 (fuente en x=0).
    for y in 1..=4 {
        for z in -3..=3 {
            g.wall([3, y, z]);
        }
    }
    g.set([0, 1, 0], Fluid::Source);
    g.set([1, 1, 0], Fluid::Source);
    let (ticks_held, _, _) = settle(&mut g, BUDGET, 5_000);
    let held = g.total();
    // Romper el dique.
    for y in 1..=4 {
        for z in -3..=3 {
            g.walls.remove(&[3, y, z]);
        }
    }
    let (ticks_break, processed, pending) = settle(&mut g, BUDGET, 20_000);
    println!("== Dique que se rompe ==");
    println!("  retenido: celdas={held} (ticks={ticks_held})");
    println!(
        "  tras romper: celdas={}  ticks={}  procesadas={}  pendientes={}",
        g.total(),
        ticks_break,
        processed,
        pending
    );
    println!();
}

/// 2x2: 3 fuentes + 1 flujo a nivel 8 deben convertirse en fuente.
fn escenario_2x2() {
    let mut g = ProbeGrid::new(0);
    g.set([0, 1, 0], Fluid::Source);
    g.set([1, 1, 0], Fluid::Source);
    g.set([0, 1, 1], Fluid::Source);
    g.set([1, 1, 1], Fluid::Flow(MAX_LEVEL));
    let _ = settle(&mut g, BUDGET, 200);
    let all_source = [[0, 1, 0], [1, 1, 0], [0, 1, 1], [1, 1, 1]]
        .iter()
        .all(|&p| g.fluid(p).is_source());
    println!("== 2x2 (3 fuentes + 1 flujo) ==");
    println!(
        "  las 4 celdas son fuente: {}  (niveles: {},{},{},{})",
        all_source,
        g.level_at([0, 1, 0]),
        g.level_at([1, 1, 0]),
        g.level_at([0, 1, 1]),
        g.level_at([1, 1, 1])
    );
    println!();
}
