//! Simulacion de **agua** con la semantica de Minecraft (Java).
//!
//! `Block` es un `enum` sin campos y el chunk guarda **1 byte por voxel**, asi
//! que el *nivel* de agua no cabe en el bloque: lo guardamos aparte (nibble de
//! flujo en la columna) y lo simulamos aqui.
//!
//! ## Modelo (fuente -> distancia), NO conserva volumen
//!
//! * **Nivel 8 = fuente** (inagotable). El flujo vale `8 - distancia`, asi que
//!   una fuente alcanza **7 bloques** en horizontal y se agota (nivel 1).
//! * El nivel de una celda de flujo **se recalcula** desde sus vecinos
//!   (`max(nivel vecino) - 1`), no de un volumen compartido. Por eso, al quitar
//!   la fuente, el agua **retrocede** a su nivel por distancia y desaparece.
//! * **Fuente infinita**: una celda de flujo con **2+ vecinos fuente**
//!   ortogonales se vuelve fuente (el clasico 2x2).
//! * **Caida**: una celda es *falling* (nivel 8) si la de **arriba** tiene agua
//!   (cadena vertical bajo una fuente o una caida); al tocar suelo se reparte a
//!   `8 -> 7`. Una celda a nivel 8 (fuente o caida) **solo** se reparte en
//!   horizontal cuando descansa sobre solido: mientras su columna no este llena,
//!   solo baja (asi una cascada no ensancha a cada altura). El agua **prefiere
//!   bajar**; solo se extiende en horizontal si no puede caer.
//! * Una celda de flujo **sin fuente** (ni agua arriba) desaparece.
//!
//! A diferencia de Minecraft (tick cada 5 ticks = 0.25 s), aqui el tick es mas
//! rapido (ver `WATER_PERIOD` en `engine::app`), pero las reglas son las mismas.
//!
//! Fuera del alcance (documentado): presion hacia arriba (vasos comunicantes),
//! evaporacion y **lava** (bloques/texturas aparte). Una celda que toca una
//! columna **no cargada** no se recalcula: se congela (`is_loaded`), porque un
//! vecino desconocido **no** es lo mismo que un vecino seco (P1-6).

use std::collections::BTreeSet;

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
            cells: 16_384,
            ms: 6.0,
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
/// * `Flow(l)` — agua que fluye con nivel `1..=MAX_LEVEL` (`MAX_LEVEL` = caida).
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

    /// ¿La columna de `p` esta cargada? Por defecto equivale a `in_bounds`.
    ///
    /// Se usa para distinguir "no hay agua" de "no tenemos datos": una celda
    /// junto a una columna descargada no se recalcula (se congela) en vez de
    /// secarse (P1-6). Las rejillas de test lo sobreescriben para simular
    /// columnas descargadas; `World` usa el valor por defecto (`in_bounds`).
    fn is_loaded(&self, p: [i32; 3]) -> bool {
        self.in_bounds(p)
    }
}

/// ¿Algun vecino **horizontal** vive en una columna no cargada? En ese caso no
/// se puede recalcular el nivel sin inventar datos.
fn touches_unknown_column<G: FluidGrid + ?Sized>(grid: &G, p: [i32; 3]) -> bool {
    H_DIRS.iter().any(|d| {
        let n = [p[0] + d[0], p[1], p[2] + d[2]];
        !grid.is_loaded(n)
    })
}

/// Los 4 vecinos horizontales.
const H_DIRS: [[i32; 3]; 4] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];

/// ¿El agua que fluye en `p` toca **dos o mas fuentes** ortogonales? Entonces
/// pasa a `Fluid::Source`: es la regla clasica del 2x2 (apoyar agua junto a un
/// manantial la fija).
pub fn check_2x2_source<G: FluidGrid + ?Sized>(grid: &mut G, p: [i32; 3]) -> bool {
    if grid.fluid(p).is_source() {
        return false;
    }
    if count_horizontal_sources(grid, p) >= 2 {
        grid.set_fluid(p, Fluid::Source);
        true
    } else {
        false
    }
}

/// Cuantos vecinos horizontales son fuentes.
fn count_horizontal_sources<G: FluidGrid + ?Sized>(grid: &G, p: [i32; 3]) -> u32 {
    H_DIRS
        .iter()
        .filter(|d| {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            grid.in_bounds(n) && grid.fluid(n).is_source()
        })
        .count() as u32
}

/// Nivel que le corresponde a `p` segun sus vecinos (`getNewLiquid`).
///
/// Las fuentes nunca cambian por esta regla. Una celda de flujo:
/// * con **2+ vecinos fuente** -> se convierte en fuente;
/// * con **agua encima** -> *falling* a `MAX_LEVEL`;
/// * si no, `max(nivel vecino) - 1` (0 -> se elimina).
fn get_new_level<G: FluidGrid + ?Sized>(grid: &G, p: [i32; 3]) -> Fluid {
    if grid.fluid(p).is_source() {
        return Fluid::Source;
    }
    let mut max_n = 0u8;
    for d in H_DIRS {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if !grid.in_bounds(n) || grid.is_solid(n) {
            continue;
        }
        let l = grid.fluid(n).level();
        if l > max_n {
            max_n = l;
        }
    }
    if count_horizontal_sources(grid, p) >= 2 {
        return Fluid::Source;
    }
    // Agua (o fuente) justo encima -> el agua "cae" a nivel maximo.
    let above = [p[0], p[1] + 1, p[2]];
    if grid.in_bounds(above) && grid.fluid(above).level() > 0 {
        return Fluid::Flow(MAX_LEVEL);
    }
    if max_n <= 1 {
        Fluid::None
    } else {
        Fluid::from_level(max_n - FLOW_DECAY)
    }
}

/// Reparte el agua de `p` (de nivel `level`): primero **abajo** (a nivel 8,
/// *falling*); si no puede caer, a los vecinos horizontales con `nivel - 1`.
fn spread<G: FluidGrid + ?Sized>(grid: &mut G, p: [i32; 3], level: u8) -> bool {
    // 1. Caida: si el fondo puede recibir, el agua va hacia abajo y NO se
    //    extiende en horizontal (preferencia por bajar).
    let below = [p[0], p[1] - 1, p[2]];
    if grid.in_bounds(below) && !grid.is_solid(below) {
        let bf = grid.fluid(below);
        if !bf.is_source() && bf.level() < MAX_LEVEL {
            grid.set_fluid(below, Fluid::Flow(MAX_LEVEL));
            return true;
        }
        // El fondo ya esta **lleno** (agua a nivel 8). Una celda a nivel 8 (fuente
        // o caida) sigue alimentando su columna vertical y NO se reparte en
        // horizontal: solo lo hace al tocar **suelo** (rama de abajo). Sin esto,
        // cada altura de la cascada generaba un charco (bug 2.1).
        if level == MAX_LEVEL {
            return false;
        }
    }

    // 2. Horizontal: cada vecino sube a `nivel - 1` si esta mas bajo.
    let target = level.saturating_sub(FLOW_DECAY);
    if target == 0 {
        return false;
    }
    let mut changed = false;
    for d in H_DIRS {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        // No se escribe agua hacia una columna sin cargar (se pierde al descargar).
        if !grid.in_bounds(n) || !grid.is_loaded(n) || grid.is_solid(n) {
            continue;
        }
        let nf = grid.fluid(n);
        if nf.is_source() || nf.level() >= target {
            continue;
        }
        grid.set_fluid(n, Fluid::Flow(target));
        changed = true;
    }
    changed
}

/// Procesa **una** celda con agua. Devuelve `true` si cambio algo.
pub fn step_cell<G: FluidGrid + ?Sized>(grid: &mut G, p: [i32; 3]) -> bool {
    let cur = grid.fluid(p);
    if !cur.is_water() {
        return false;
    }
    // P1-6: sin datos de una columna vecina el agua **no** se seca; se congela
    // hasta que la columna se cargue (entonces `wake_column_water` la encola).
    if touches_unknown_column(grid, p) {
        return false;
    }
    let mut changed = false;

    // 1. Recalcular el nivel por distancia a la fuente.
    let new = get_new_level(grid, p);
    if new != cur {
        grid.set_fluid(p, new);
        changed = true;
        if new == Fluid::None {
            return true; // los vecinos se re-encolaran y retrocederan
        }
    }

    // 2. Repartir (bajar o extenderse en horizontal).
    if new.is_water() && spread(grid, p, new.level()) {
        changed = true;
    }
    changed
}

/// Cola de celdas pendientes con **orden determinista** y deduplicacion.
///
/// La clave es `(x, z, y)`: se procesa primero la columna mas al oeste y, en
/// empate, la mas al sur. Antes era un `VecDeque` en orden de insercion y, como
/// las columnas llegan de workers de terreno en orden arbitrario, dos ejecuciones
/// del mismo mundo podian simular el agua en distinto orden. `BTreeSet` cuesta
/// `O(log n)` por operacion (n ~ miles) pero elimina esa dependencia.
#[derive(Default)]
pub struct DirtyQueue {
    queue: BTreeSet<(i32, i32, i32)>,
}

impl DirtyQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Encola una celda si no estaba ya pendiente.
    pub fn push(&mut self, p: [i32; 3]) {
        self.queue.insert((p[0], p[2], p[1]));
    }

    /// Saca la siguiente celda pendiente (menor `(x, z, y)`).
    pub fn pop(&mut self) -> Option<[i32; 3]> {
        let (x, z, y) = self.queue.pop_first()?;
        Some([x, y, z])
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

    /// Rejilla con la mitad izquierda "no cargada": sirve para probar que el
    /// borde no se trata como seco.
    struct EdgeGrid {
        cells: HashMap<[i32; 3], Fluid>,
    }

    impl FluidGrid for EdgeGrid {
        fn in_bounds(&self, p: [i32; 3]) -> bool {
            (1..16).contains(&p[1]) && p[0].abs() < 16 && p[2].abs() < 16
        }
        fn is_loaded(&self, p: [i32; 3]) -> bool {
            p[0] >= 0
        }
        fn is_solid(&self, p: [i32; 3]) -> bool {
            p[1] <= 0
        }
        fn fluid(&self, p: [i32; 3]) -> Fluid {
            *self.cells.get(&p).unwrap_or(&Fluid::None)
        }
        fn set_fluid(&mut self, p: [i32; 3], f: Fluid) {
            if f == Fluid::None {
                self.cells.remove(&p);
            } else {
                self.cells.insert(p, f);
            }
        }
    }

    #[test]
    fn el_agua_en_el_borde_no_se_seca_si_el_vecino_no_esta_cargado() {
        // P1-6: sin datos de la columna vecina, la celda se congela; antes
        // `in_bounds == false` se interpretaba como "seco" y el agua desaparecia.
        let mut g = EdgeGrid {
            cells: HashMap::new(),
        };
        g.set_fluid([0, 1, 0], Fluid::Flow(5));
        assert!(!step_cell(&mut g, [0, 1, 0]), "la celda debe congelarse");
        assert_eq!(
            g.fluid([0, 1, 0]),
            Fluid::Flow(5),
            "el agua del borde no debe secarse"
        );
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
    fn una_fuente_cae_hasta_el_suelo_y_se_queda() {
        let mut g = TestGrid::new(0);
        g.set([0, 5, 0], Fluid::Source);
        for _ in 0..20 {
            tick_all(&mut g);
        }
        assert!(g.fluid([0, 1, 0]).is_water(), "deberia posarse en el suelo");
        assert_eq!(g.fluid([0, 5, 0]), Fluid::Source, "la fuente no se agota");
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
    fn el_alcance_horizontal_es_siete_en_un_canal() {
        let mut g = TestGrid::new(0);
        // Paredes laterales para formar un canal de 1 de ancho en +X.
        for x in 0..=MAX_LEVEL as i32 + 1 {
            g.wall([x, 1, -1]);
            g.wall([x, 1, 1]);
        }
        g.wall([MAX_LEVEL as i32 + 1, 1, 0]);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..80 {
            tick_all(&mut g);
        }
        for d in 1..MAX_LEVEL {
            assert_eq!(g.fluid([d as i32, 1, 0]).level(), MAX_LEVEL - d, "d={d}");
        }
        assert_eq!(g.fluid([MAX_LEVEL as i32, 1, 0]), Fluid::None);
    }

    #[test]
    fn una_fuente_alta_no_inunda_un_volumen() {
        // 2.1: una fuente en y=10 sobre suelo plano debe formar un charco de
        // radio 7 (niveles 8..1) y un tubo de nivel 8 debajo, NO un bloque lleno.
        let mut g = TestGrid::new(0);
        g.set([0, 10, 0], Fluid::Source);
        for _ in 0..200 {
            tick_all(&mut g);
        }
        assert!(
            g.cells.len() < 1000,
            "una fuente inundo {} celdas",
            g.cells.len()
        );
        // En el suelo, el nivel decae con la distancia.
        for d in 1..MAX_LEVEL {
            assert_eq!(g.fluid([d as i32, 1, 0]).level(), MAX_LEVEL - d, "d={d}");
        }
        assert_eq!(g.fluid([MAX_LEVEL as i32, 1, 0]), Fluid::None);
        // La columna de caida es de nivel 8 (no se ensancha).
        for y in 2..=9 {
            assert_eq!(g.fluid([0, y, 0]).level(), MAX_LEVEL, "caida y={y}");
        }
    }

    #[test]
    fn una_cascada_no_se_extiende_en_lateral() {
        // 2.1(c): cascada de 6; solo hay agua lateral en el suelo (y=1).
        let mut g = TestGrid::new(0);
        g.set([0, 6, 0], Fluid::Source);
        for _ in 0..200 {
            tick_all(&mut g);
        }
        for y in 2..=6 {
            let lateral = g
                .cells
                .keys()
                .filter(|p| p[1] == y && (p[0] != 0 || p[2] != 0))
                .count();
            assert_eq!(lateral, 0, "agua lateral en y={y}");
        }
        assert!(
            g.fluid([3, 1, 0]).is_water(),
            "deberia haber charco en el suelo"
        );
    }

    #[test]
    fn test_water_spreads_7_blocks() {
        // P0.2 (Fase B): una fuente en una llanura llega 7 bloques en horizontal
        // con nivel decreciente (8 en la fuente, 1 en la ultima celda).
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..80 {
            tick_all(&mut g);
        }
        for d in 1..MAX_LEVEL {
            assert_eq!(
                g.fluid([d as i32, 1, 0]).level(),
                MAX_LEVEL - d,
                "nivel a {d} bloques"
            );
        }
        assert_eq!(g.fluid([7, 1, 0]).level(), 1);
        assert_eq!(g.fluid([8, 1, 0]), Fluid::None, "no debe pasar de 7");
    }

    #[test]
    fn test_water_falls() {
        // P0.2 (Fase B): una fuente sobre un acantilado cae hasta el fondo.
        let mut g = TestGrid::new(0);
        g.set([0, 8, 0], Fluid::Source);
        for _ in 0..40 {
            tick_all(&mut g);
        }
        for y in 1..=8 {
            assert!(g.fluid([0, y, 0]).is_water(), "falta agua en y={y}");
        }
    }

    #[test]
    fn test_break_block_triggers_flow() {
        // P0.2 (Fase B): un muro contiene la fuente; al romperlo, el siguiente
        // tick riega la celda que antes estaba bloqueada.
        let mut g = TestGrid::new(0);
        g.wall([1, 1, 0]);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..10 {
            tick_all(&mut g);
        }
        assert_eq!(g.fluid([1, 1, 0]), Fluid::None, "el muro debe contener");

        // "Romper" el bloque: deja de ser solido.
        g.walls.remove(&[1, 1, 0]);
        step_cell(&mut g, [0, 1, 0]);
        assert!(
            g.fluid([1, 1, 0]).is_water(),
            "el agua debe entrar al romper el bloque"
        );
    }

    #[test]
    fn un_flujo_sin_fuente_desaparece() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Flow(5));
        for _ in 0..5 {
            tick_all(&mut g);
        }
        assert_eq!(g.fluid([0, 1, 0]), Fluid::None, "un flujo huerfano se seca");
    }

    #[test]
    fn el_agua_no_conserva_volumen() {
        // Una sola fuente crea agua a su alrededor: el volumen aumenta.
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        let antes = g.sum();
        for _ in 0..80 {
            tick_all(&mut g);
        }
        assert!(
            g.sum() > antes,
            "una fuente deberia crear agua (no conserva)"
        );
    }

    #[test]
    fn quitar_la_fuente_drena_la_charca() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
        for _ in 0..80 {
            tick_all(&mut g);
        }
        assert!(g.fluid([3, 1, 0]).is_water(), "el charco deberia existir");
        // Quitamos la fuente: el agua retrocede y desaparece.
        g.set([0, 1, 0], Fluid::None);
        for _ in 0..200 {
            tick_all(&mut g);
        }
        assert!(
            g.cells.is_empty(),
            "el agua deberia haberse drenado: {:?}",
            g.cells
        );
    }

    #[test]
    fn el_agua_cae_en_columna_vertical() {
        let mut g = TestGrid::new(0);
        g.set([0, 6, 0], Fluid::Source);
        for _ in 0..30 {
            tick_all(&mut g);
        }
        for y in 1..=6 {
            assert!(g.fluid([0, y, 0]).is_water(), "falta agua en y={y}");
        }
    }

    #[test]
    fn el_agua_no_se_sale_por_el_fondo_del_mundo() {
        let mut g = TestGrid::new(0);
        g.set([0, 1, 0], Fluid::Source);
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

    #[test]
    fn test_dirty_queue_orden_por_xz() {
        // El orden no depende de la insercion: (x, z, y) ascendente.
        let mut q = DirtyQueue::new();
        for p in [[3, 0, 1], [1, 5, 2], [1, 0, 4], [1, 2, 2]] {
            q.push(p);
        }
        q.push([3, 0, 1]); // duplicado: no debe repetirse
        assert_eq!(q.len(), 4);
        // Claves (x, z, y): (1,2,2) < (1,2,5) < (1,4,0) < (3,1,0).
        assert_eq!(q.pop(), Some([1, 2, 2]));
        assert_eq!(q.pop(), Some([1, 5, 2]));
        assert_eq!(q.pop(), Some([1, 0, 4]));
        assert_eq!(q.pop(), Some([3, 0, 1]));
        assert!(q.is_empty());
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
}
