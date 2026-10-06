//! Simulacion de **agua** con niveles: automata celular (estilo Minecraft).
//!
//! `Block` es un `enum` sin campos y el chunk guarda **1 byte por voxel**, asi
//! que el *nivel* de agua no cabe en el bloque. Lo guardamos **aparte** (en
//! `World`, como mapa de desbordes) y lo simulamos aqui.
//!
//! Un tick procesa cada celda con agua:
//! 1. **Caida**: si la celda de abajo no es solida y no esta llena, el agua baja.
//! 2. **Propagacion horizontal**: si no puede caer, se reparte a los 4 vecinos
//!    horizontales hasta `nivel - FLOW_DECAY`. Una fuente (nivel maximo) llena a
//!    sus vecinos hasta `MAX_LEVEL - FLOW_DECAY`; esos, a los suyos, uno menos, y
//!    asi: una fuente forma un charco de radio `MAX_LEVEL - 1`, no inunda el
//!    mundo.
//! 3. **Igualacion**: dos celdas vecinas tienden al mismo nivel (superficies
//!    planas) porque cada una empuja su exceso hacia la mas baja.
//! 4. **Fuentes**: `Fluid::Source` nunca se agota (es el oceano). Un bloque
//!    `Water` colocado por el jugador tambien es fuente.
//! 5. **Conservacion**: en modo finito (sin fuentes) el volumen total no cambia.
//!    Cada transferencia **resta** a una celda y **suma** a otra, y una celda que
//!    llega a 0 se convierte en aire.
//!
//! Fuera del alcance (documentado): flujo **hacia arriba** por presion (vasos
//! comunicantes), cascada diagonal, evaporacion y **lava** (necesita bloques y
//! texturas nuevas, que aporta la IA de diseno). Los bordes de chunk no
//! cargados simplemente se saltan (el agua no los cruza todavia).

use std::collections::VecDeque;

/// Nivel de agua maximo (una fuente esta siempre a tope).
pub const MAX_LEVEL: u8 = 8;

/// Cuanto pierde el agua por cada bloque horizontal que se aleja de una fuente.
pub const FLOW_DECAY: u8 = 1;

/// Presupuesto de la simulacion de fluidos por tick. Doble cota: numero de
/// celdas y milisegundos, para que un cambio grande (romper un dique) no bloquee
/// el frame aunque queden pocas celdas por procesar. Configurable por entorno
/// (`SOLARIA_FLUID_BUDGET_CELLS`, `SOLARIA_FLUID_BUDGET_MS`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidBudget {
    /// Celdas maximas a procesar por tick.
    pub cells: usize,
    /// Tiempo maximo de simulacion por tick (ms).
    pub ms: f32,
}

impl Default for FluidBudget {
    fn default() -> Self {
        Self {
            cells: 8192,
            ms: 4.0,
        }
    }
}

impl FluidBudget {
    /// Lee el presupuesto del entorno, o el valor por defecto si no hay nada.
    pub fn from_env() -> Self {
        let mut budget = Self::default();
        if let Ok(s) = std::env::var("SOLARIA_FLUID_BUDGET_CELLS")
            && let Ok(v) = s.parse::<usize>()
        {
            budget.cells = v;
        }
        if let Ok(s) = std::env::var("SOLARIA_FLUID_BUDGET_MS")
            && let Ok(v) = s.parse::<f32>()
            && v > 0.0
        {
            budget.ms = v;
        }
        budget
    }
}

/// Estado del agua en una celda.
///
/// * `None` — no hay agua.
/// * `Source` — fuente inagotable (oceano o agua colocada por el jugador).
/// * `Flow(l)` — agua que fluye con nivel `1..=MAX_LEVEL`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fluid {
    None,
    Source,
    Flow(u8),
}

impl Fluid {
    /// Nivel numerico equivalente (0 para `None`).
    #[inline]
    pub fn level(self) -> u8 {
        match self {
            Fluid::None => 0,
            Fluid::Source => MAX_LEVEL,
            Fluid::Flow(l) => l.min(MAX_LEVEL),
        }
    }

    /// ¿Hay agua en la celda?
    #[inline]
    pub fn is_water(self) -> bool {
        !matches!(self, Fluid::None)
    }

    /// ¿Es una fuente inagotable?
    #[inline]
    pub fn is_source(self) -> bool {
        matches!(self, Fluid::Source)
    }

    /// Construye un `Flow` a partir de un nivel (`0` -> `None`).
    #[inline]
    pub fn from_level(level: u8) -> Self {
        if level == 0 {
            Fluid::None
        } else {
            Fluid::Flow(level.min(MAX_LEVEL))
        }
    }
}

/// Rejilla sobre la que corre la simulacion: `World` la implementa para el
/// mundo real y los tests usan una version en memoria.
///
/// `in_bounds` significa "celda **existente y cargada**": fuera de ella no se
/// escribe agua (es asi como se saltan los bordes de chunk sin bloquear).
pub trait FluidGrid {
    fn in_bounds(&self, p: [i32; 3]) -> bool;
    fn is_solid(&self, p: [i32; 3]) -> bool;
    fn fluid(&self, p: [i32; 3]) -> Fluid;
    fn set_fluid(&mut self, p: [i32; 3], f: Fluid);
}

/// Los 4 vecinos horizontales.
const H_DIRS: [[i32; 3]; 4] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];

/// ¿El agua que fluye en `p` toca **dos o mas fuentes** ortogonales? Entonces
/// pasa a `Fluid::Source`: es la regla clasica del 2x2 (apoyar agua junto a un
/// manantial la fija). Exigir 2 fuentes evita que un charco normal se convierta
/// en manantial infinito (y preserva la conservacion en modo finito).
pub fn check_2x2_source<G: FluidGrid + ?Sized>(grid: &mut G, p: [i32; 3]) -> bool {
    if !matches!(grid.fluid(p), Fluid::Flow(_)) {
        return false;
    }
    let fuentes = H_DIRS
        .iter()
        .filter(|d| {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            grid.in_bounds(n) && grid.fluid(n).is_source()
        })
        .count();
    if fuentes >= 2 {
        grid.set_fluid(p, Fluid::Source);
        true
    } else {
        false
    }
}

/// ¿El agua en `p` esta **en equilibrio** (estatica)? Ocurre cuando el fondo
/// esta bloqueado o lleno y los 4 vecinos horizontales tienen su mismo nivel.
/// Entonces el tick no debe tocarla: los oceanos generados salen gratis.
fn at_equilibrium<G: FluidGrid + ?Sized>(grid: &G, p: [i32; 3]) -> bool {
    let level = grid.fluid(p).level();
    let below = [p[0], p[1] - 1, p[2]];
    let below_ok =
        !grid.in_bounds(below) || grid.is_solid(below) || grid.fluid(below).level() >= MAX_LEVEL;
    if !below_ok {
        return false;
    }
    H_DIRS.iter().all(|d| {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        !grid.in_bounds(n) || grid.is_solid(n) || grid.fluid(n).level() == level
    })
}

/// Procesa **una** celda con agua. Devuelve `true` si cambio algo.
pub fn step_cell<G: FluidGrid + ?Sized>(grid: &mut G, p: [i32; 3]) -> bool {
    if !grid.fluid(p).is_water() {
        return false;
    }
    // 0. Fuentes 2x2: el flujo con 2+ fuentes contiguas se fija.
    if check_2x2_source(grid, p) {
        return true;
    }
    // 0b. Equilibrio: agua estatica (mismo nivel que los vecinos, fondo firme).
    //     Salimos ANTES de calcular nada: es el caso de los oceanos.
    if at_equilibrium(grid, p) {
        return false;
    }

    let f = grid.fluid(p);
    let is_source = f.is_source();
    let mut level = f.level();
    let mut changed = false;

    // 1. Caida: el agua prefiere bajar.
    let below = [p[0], p[1] - 1, p[2]];
    let mut below_blocked = true;
    if grid.in_bounds(below) && !grid.is_solid(below) {
        let bf = grid.fluid(below).level();
        if bf < MAX_LEVEL {
            let t = (MAX_LEVEL - bf).min(level);
            if t > 0 {
                grid.set_fluid(below, Fluid::from_level(bf + t));
                if !is_source {
                    level -= t;
                    grid.set_fluid(p, Fluid::from_level(level));
                }
                changed = true;
            }
            below_blocked = bf + t >= MAX_LEVEL;
            if !is_source && level == 0 {
                return true;
            }
        }
    }

    // 2. Propagacion horizontal: una fuente siempre; el agua que fluye, solo si
    //    no puede bajar mas (abajo solido o lleno).
    if is_source || below_blocked {
        for d in H_DIRS {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if !grid.in_bounds(n) || grid.is_solid(n) {
                continue;
            }
            let nf = grid.fluid(n).level();
            let target = if is_source {
                MAX_LEVEL - FLOW_DECAY
            } else {
                level.saturating_sub(FLOW_DECAY)
            };
            if nf >= target {
                continue;
            }
            // Mitad de la diferencia, al menos 1, sin pasar del objetivo.
            let diff = level.saturating_sub(nf);
            let mut give = (diff / 2).max(1).min(target - nf);
            if is_source {
                grid.set_fluid(n, Fluid::from_level(nf + give));
                changed = true;
            } else {
                give = give.min(level);
                if give == 0 {
                    continue;
                }
                grid.set_fluid(n, Fluid::from_level(nf + give));
                level -= give;
                grid.set_fluid(p, Fluid::from_level(level));
                changed = true;
                if level == 0 {
                    break;
                }
            }
        }
    }

    changed
}

/// Cola de celdas pendientes con deduplicacion (sin allocations en el hot path
/// mas alla del propio buffer reutilizado).
#[derive(Default)]
pub struct DirtyQueue {
    queue: VecDeque<[i32; 3]>,
    seen: std::collections::HashSet<[i32; 3]>,
}

impl DirtyQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Encola una celda si no estaba ya pendiente.
    pub fn push(&mut self, p: [i32; 3]) {
        if self.seen.insert(p) {
            self.queue.push_back(p);
        }
    }

    /// Saca la siguiente celda pendiente (la marca como ya vista).
    pub fn pop(&mut self) -> Option<[i32; 3]> {
        let p = self.queue.pop_front()?;
        self.seen.remove(&p);
        Some(p)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.queue.clear();
        self.seen.clear();
    }
}

/// Una coordenada y sus 6 vecinos (para re-encolar al cambiar una celda).
pub fn neighborhood(p: [i32; 3]) -> [[i32; 3]; 7] {
    [
        p,
        [p[0] + 1, p[1], p[2]],
        [p[0] - 1, p[1], p[2]],
        [p[0], p[1] + 1, p[2]],
        [p[0], p[1] - 1, p[2]],
        [p[0], p[1], p[2] + 1],
        [p[0], p[1], p[2] - 1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// Rejilla de test: suelo plano en `y <= floor_y` y paredes opcionales.
    struct TestGrid {
        cells: HashMap<[i32; 3], Fluid>,
        walls: HashSet<[i32; 3]>,
        floor_y: i32,
    }

    impl TestGrid {
        fn new(floor_y: i32) -> Self {
            Self {
                cells: HashMap::new(),
                walls: HashSet::new(),
                floor_y,
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

        fn sum(&self) -> u32 {
            self.cells.values().map(|f| f.level() as u32).sum()
        }
    }

    impl FluidGrid for TestGrid {
        fn in_bounds(&self, p: [i32; 3]) -> bool {
            p[1] > self.floor_y && p[1] < 64 && p[0].abs() < 64 && p[2].abs() < 64
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

    /// Un "tick" de test: procesa todas las celdas con agua (mas de una pasada
    /// para que el agua que acaba de llegar reaccione).
    fn tick_all(g: &mut TestGrid) {
        for _ in 0..2 {
            let keys: Vec<[i32; 3]> = g.cells.keys().copied().collect();
            for p in keys {
                step_cell(g, p);
            }
        }
    }

    #[test]
    fn un_bloque_cae_hasta_el_suelo() {
        let mut g = TestGrid::new(0);
        g.set([0, 5, 0], Fluid::Flow(MAX_LEVEL));
        for _ in 0..20 {
            tick_all(&mut g);
        }
        assert!(g.fluid([0, 1, 0]).is_water(), "deberia posarse en el suelo");
        assert_eq!(
            g.fluid([0, 5, 0]),
            Fluid::None,
            "la celda de arriba se vacia"
        );
    }

    #[test]
    fn una_fuente_se_extiende_a_radio_max_level_menos_1() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..80 {
            tick_all(&mut g);
        }
        // En el eje +X, la distancia d tiene nivel MAX_LEVEL - d (charco en rombo).
        for d in 1..MAX_LEVEL {
            assert_eq!(
                g.fluid([d as i32, 1, 0]).level(),
                MAX_LEVEL - d,
                "nivel inesperado a distancia {d}"
            );
        }
        // A distancia MAX_LEVEL ya no llega agua.
        assert_eq!(g.fluid([MAX_LEVEL as i32, 1, 0]), Fluid::None);
        assert_eq!(g.fluid([0, 1, 0]), Fluid::Source, "la fuente no se agota");
    }

    #[test]
    fn dos_niveles_vecinos_se_igualan() {
        // Canal cerrado por paredes para que solo puedan intercambiar entre si.
        let mut g = TestGrid::new(0);
        g.wall([-1, 1, 0]);
        g.wall([2, 1, 0]);
        g.wall([0, 1, -1]);
        g.wall([0, 1, 1]);
        g.wall([1, 1, -1]);
        g.wall([1, 1, 1]);
        g.set([0, 1, 0], Fluid::Flow(6));
        g.set([1, 1, 0], Fluid::Flow(2));
        let total = g.sum();
        for _ in 0..20 {
            tick_all(&mut g);
        }
        assert_eq!(g.sum(), total, "el volumen se conserva");
        let a = g.fluid([0, 1, 0]).level() as i32;
        let b = g.fluid([1, 1, 0]).level() as i32;
        assert!((a - b).abs() <= 1, "no se igualaron: {a} vs {b}");
    }

    #[test]
    fn el_volumen_se_conserva_en_modo_finito() {
        // Caja cerrada con paredes; reparto inicial irregular, sin fuentes.
        let mut g = TestGrid::new(0);
        for x in -3..=3 {
            for z in -3..=3 {
                for y in 1..=4 {
                    let border = x == -3 || x == 3 || z == -3 || z == 3 || y == 4;
                    if border {
                        g.wall([x, y, z]);
                    }
                }
            }
        }
        // Estado inicial deterministico.
        let mut seed = 12345u32;
        for x in -2..=2 {
            for z in -2..=2 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let lvl = ((seed >> 16) % 6) as u8; // 0..=5
                if lvl > 0 {
                    g.set([x, 1, z], Fluid::Flow(lvl));
                }
            }
        }
        let total = g.sum();
        for _ in 0..1000 {
            tick_all(&mut g);
        }
        assert_eq!(g.sum(), total, "se perdio o gano agua");
        assert!(
            g.cells.values().all(|f| f.level() <= MAX_LEVEL),
            "hay un nivel fuera de rango"
        );
    }

    #[test]
    fn la_fuente_no_se_agota_al_propagarse() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..200 {
            tick_all(&mut g);
        }
        assert_eq!(g.fluid([0, 1, 0]), Fluid::Source);
        // Sigue habiendo agua al lado despues de "drenar" la charca.
        assert!(g.fluid([1, 1, 0]).is_water());
    }

    #[test]
    fn la_cola_deduplica_y_vacia() {
        let mut q = DirtyQueue::new();
        q.push([1, 2, 3]);
        q.push([1, 2, 3]);
        q.push([4, 5, 6]);
        assert_eq!(q.len(), 2);
        assert_eq!(q.pop(), Some([1, 2, 3]));
        assert_eq!(q.pop(), Some([4, 5, 6]));
        assert!(q.is_empty());
        q.push([0, 0, 0]);
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn el_agua_no_se_sale_por_el_fondo_del_mundo() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Flow(MAX_LEVEL));
        for _ in 0..10 {
            tick_all(&mut g);
        }
        // No debe existir agua por debajo del suelo.
        assert!(g.cells.keys().all(|&[_, y, _]| y > 0));
    }

    #[test]
    fn tres_fuentes_y_un_flujo_forman_un_manantial_2x2() {
        // El prompt: 3 bloques de agua + 1 nuevo forman una fuente 2x2.
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        g.set([1, 1, 0], Fluid::Source);
        g.set([0, 1, 1], Fluid::Source);
        g.set([1, 1, 1], Fluid::Flow(MAX_LEVEL));
        assert!(
            check_2x2_source(&mut g, [1, 1, 1]),
            "el bloque nuevo deberia completar el manantial"
        );
        for c in [[0, 1, 0], [1, 1, 0], [0, 1, 1], [1, 1, 1]] {
            assert_eq!(g.fluid(c), Fluid::Source, "celda {c:?} no es fuente");
        }
    }

    #[test]
    fn el_agua_en_equilibrio_no_cambia() {
        // Mar tranquilo: 3x3 a nivel de fuente con el fondo solido.
        let mut g = TestGrid::new(0);
        for dx in -1..=1i32 {
            for dz in -1..=1i32 {
                g.set([dx, 1, dz], Fluid::Source);
            }
        }
        let antes = g.cells.clone();
        assert!(
            !step_cell(&mut g, [0, 1, 0]),
            "una celda en equilibrio no deberia cambiar"
        );
        assert_eq!(g.cells, antes, "el estado no debe modificarse");
    }

    #[test]
    fn el_agua_en_equilibrio_no_se_reencola() {
        // Reproduce la logica del tick: solo se re-encola si `step_cell` cambio.
        let mut g = TestGrid::new(0);
        for dx in -1..=1i32 {
            for dz in -1..=1i32 {
                g.set([dx, 1, dz], Fluid::Source);
            }
        }
        let mut q = DirtyQueue::new();
        q.push([0, 1, 0]);
        let p = q.pop().unwrap();
        if step_cell(&mut g, p) {
            for n in neighborhood(p) {
                q.push(n);
            }
        }
        assert!(q.is_empty(), "el agua en equilibrio no debe reencolarse");
    }
}
