//! Estado del **inventario** y logica de **clicks/arrastre** (MEGA PROMPT 3,
//! Fase D). Puro: sin GPU ni winit; `engine::app` traduce eventos y `render`
//! dibuja. Toda la semantica de stacks (coger, soltar, mitad, uno, reparto por
//! arrastre, shift-click, doble click, 1-9, Q) vive aqui y tiene tests.
//!
//! Invariante: ninguna operacion **salvo** "soltar" (Q) o crear en creativo
//! cambia el numero total de unidades de cada bloque (ranuras + cursor).

use crate::world::ItemStack;
use crate::world::block::Block;

/// Ranuras de la hotbar.
pub const HOTBAR_SLOTS: usize = 9;
/// Ranuras del inventario principal (9x3).
pub const MAIN_SLOTS: usize = 27;
/// Ventana de doble click, en segundos.
pub const DOUBLE_CLICK_SECS: f32 = 0.25;

/// Zona de ranuras.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Hotbar,
    Main,
}

/// Referencia a una ranura.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotRef {
    pub zone: Zone,
    pub index: usize,
}

impl SlotRef {
    pub fn hotbar(index: usize) -> Self {
        Self {
            zone: Zone::Hotbar,
            index,
        }
    }
    pub fn main(index: usize) -> Self {
        Self {
            zone: Zone::Main,
            index,
        }
    }
}

/// Boton del raton relevante para el inventario.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
}

#[derive(Clone, Debug)]
struct Drag {
    button: Button,
    /// Ranura donde empezo el arrastre (`None` = fuera del panel).
    start: Option<SlotRef>,
    /// Ranuras recorridas durante el arrastre.
    visited: Vec<SlotRef>,
}

/// Estado completo del inventario (hotbar + inventario + cursor + arrastre).
#[derive(Clone, Debug)]
pub struct InventoryState {
    hotbar: [Option<ItemStack>; HOTBAR_SLOTS],
    main: [Option<ItemStack>; MAIN_SLOTS],
    cursor: Option<ItemStack>,
    drag: Option<Drag>,
    last_click: Option<(SlotRef, f32)>,
    time: f32,
}

impl Default for InventoryState {
    fn default() -> Self {
        Self {
            hotbar: [None; HOTBAR_SLOTS],
            main: [None; MAIN_SLOTS],
            cursor: None,
            drag: None,
            last_click: None,
            time: 0.0,
        }
    }
}

impl InventoryState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Avanza el reloj interno (para la ventana de doble click).
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
    }

    fn slots(&self, zone: Zone) -> &[Option<ItemStack>] {
        match zone {
            Zone::Hotbar => &self.hotbar,
            Zone::Main => &self.main,
        }
    }
    fn slots_mut(&mut self, zone: Zone) -> &mut [Option<ItemStack>] {
        match zone {
            Zone::Hotbar => &mut self.hotbar,
            Zone::Main => &mut self.main,
        }
    }

    /// Pila de una ranura (o `None`).
    pub fn get(&self, r: SlotRef) -> Option<ItemStack> {
        self.slots(r.zone).get(r.index).copied().flatten()
    }

    /// Fija una ranura (una pila vacia se guarda como `None`).
    pub fn set(&mut self, r: SlotRef, stack: Option<ItemStack>) {
        if let Some(slot) = self.slots_mut(r.zone).get_mut(r.index) {
            *slot = stack.filter(|s| !s.is_empty());
        }
    }

    /// Pila que lleva el cursor (o `None`).
    pub fn cursor(&self) -> Option<ItemStack> {
        self.cursor
    }

    /// Pone una **copia infinita** (creativo) en el cursor.
    pub fn set_cursor(&mut self, stack: Option<ItemStack>) {
        self.cursor = stack.filter(|s| !s.is_empty());
    }

    /// La hotbar como `(id, cantidad)` (ranuras vacias = `(0, 0)`) para guardar.
    pub fn hotbar_save(&self) -> Vec<(u8, u8)> {
        self.hotbar
            .iter()
            .map(|s| s.map_or((0, 0), |s| (s.block.id(), s.count)))
            .collect()
    }

    /// Restaura la hotbar desde `(id, cantidad)`.
    pub fn set_hotbar_from_save(&mut self, saved: &[(u8, u8)]) {
        self.hotbar = std::array::from_fn(|i| {
            saved.get(i).and_then(|&(id, count)| {
                let stack = ItemStack::new(Block::from_u8(id), count);
                (!stack.is_empty()).then_some(stack)
            })
        });
    }

    /// Bloque de la ranura `i` de la hotbar (aire si vacia).
    pub fn hotbar_block(&self, i: usize) -> Block {
        self.hotbar
            .get(i)
            .copied()
            .flatten()
            .map_or(Block::Air, |s| s.block)
    }

    /// Total de unidades de un bloque en ranuras + cursor (para los tests).
    pub fn total(&self, block: Block) -> u32 {
        self.hotbar
            .iter()
            .chain(self.main.iter())
            .chain(std::iter::once(&self.cursor))
            .flatten()
            .filter(|s| s.block == block)
            .map(|s| s.count as u32)
            .sum()
    }

    // --- Eventos -----------------------------------------------------------

    /// Pulsa un boton. `slot = None` = click fuera del panel.
    pub fn press(&mut self, slot: Option<SlotRef>, button: Button, shift: bool) {
        if shift {
            if let Some(r) = slot {
                self.quick_move(r);
            }
            return;
        }
        self.drag = Some(Drag {
            button,
            start: slot,
            visited: slot.into_iter().collect(),
        });
    }

    /// Durante un arrastre, entra en una ranura.
    pub fn drag_enter(&mut self, r: SlotRef) {
        if let Some(drag) = self.drag.as_mut()
            && !drag.visited.contains(&r)
        {
            drag.visited.push(r);
        }
    }

    /// Suelta el boton: aplica el click o el reparto.
    pub fn release(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag.start {
            // Click fuera del panel: el stack del cursor se suelta (se descarta).
            None => self.cursor = None,
            Some(start) if drag.visited.len() <= 1 => self.click_slot(start, drag.button),
            Some(_) => self.drag_distribute(&drag),
        }
    }

    fn click_slot(&mut self, r: SlotRef, button: Button) {
        // Doble click izquierdo: recoge todo el mismo bloque.
        if button == Button::Left
            && let Some((last, t)) = self.last_click
            && last == r
            && (self.time - t) < DOUBLE_CLICK_SECS
        {
            self.collect_all(r);
            self.last_click = None;
            return;
        }
        if button == Button::Left {
            self.last_click = Some((r, self.time));
        }

        let slot = self.get(r);
        match (button, self.cursor, slot) {
            // Coger el stack completo.
            (Button::Left, None, Some(s)) => {
                self.cursor = Some(s);
                self.set(r, None);
            }
            // Soltar el stack completo.
            (Button::Left, Some(c), None) => {
                self.set(r, Some(c));
                self.cursor = None;
            }
            // Combinar hasta el maximo.
            (Button::Left, Some(c), Some(s)) if c.block == s.block => {
                let total = s.count as u32 + c.count as u32;
                let placed = total.min(ItemStack::MAX as u32) as u8;
                self.set(r, Some(ItemStack::new(s.block, placed)));
                let rest = total - placed as u32;
                self.cursor = (rest > 0).then(|| ItemStack::new(c.block, rest as u8));
            }
            // Intercambiar.
            (Button::Left, Some(c), Some(s)) => {
                self.set(r, Some(c));
                self.cursor = Some(s);
            }
            // Coger la mitad (redondeo hacia arriba).
            (Button::Right, None, Some(s)) => {
                let half = s.count.div_ceil(2);
                self.cursor = Some(ItemStack::new(s.block, half));
                let rest = s.count - half;
                self.set(r, (rest > 0).then(|| ItemStack::new(s.block, rest)));
            }
            // Dejar uno.
            (Button::Right, Some(c), None) => {
                self.set(r, Some(ItemStack::new(c.block, 1)));
                self.cursor = (c.count > 1).then(|| ItemStack::new(c.block, c.count - 1));
            }
            // Dejar uno encima de una pila compatible.
            (Button::Right, Some(c), Some(s)) if c.block == s.block && s.count < ItemStack::MAX => {
                self.set(r, Some(ItemStack::new(s.block, s.count + 1)));
                self.cursor = (c.count > 1).then(|| ItemStack::new(c.block, c.count - 1));
            }
            _ => {}
        }
    }

    /// Reparte el stack del cursor entre las ranuras recorridas.
    fn drag_distribute(&mut self, drag: &Drag) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let block = cursor.block;
        // Ranuras que pueden recibir (vacias o del mismo bloque, sin llenar).
        let targets: Vec<SlotRef> = drag
            .visited
            .iter()
            .copied()
            .filter(|&r| match self.get(r) {
                None => true,
                Some(s) => s.block == block && s.count < ItemStack::MAX,
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let mut left = cursor.count as u32;
        match drag.button {
            Button::Left => {
                let n = targets.len() as u32;
                let per = left / n;
                let mut rem = left % n;
                for r in &targets {
                    let want = per + u32::from(rem > 0);
                    rem = rem.saturating_sub(1);
                    left -= self.give(*r, block, want as u8);
                }
            }
            Button::Right => {
                for r in &targets {
                    if left == 0 {
                        break;
                    }
                    left -= self.give(*r, block, 1);
                }
            }
        }
        self.cursor = (left > 0).then(|| ItemStack::new(block, left as u8));
    }

    /// Anade `want` unidades de `block` a la ranura `r` (respetando MAX) y
    /// devuelve cuantas se colocaron de verdad.
    fn give(&mut self, r: SlotRef, block: Block, want: u8) -> u32 {
        let cur = self.get(r);
        let have = cur.map_or(0, |s| s.count as u32);
        let space = ItemStack::MAX as u32 - have;
        let placed = (want as u32).min(space);
        if placed > 0 {
            self.set(r, Some(ItemStack::new(block, (have + placed) as u8)));
        }
        placed
    }

    /// Shift-click: mueve el stack a la **otra** zona (combina o busca hueco).
    fn quick_move(&mut self, r: SlotRef) {
        let Some(stack) = self.get(r) else {
            return;
        };
        let other = match r.zone {
            Zone::Hotbar => Zone::Main,
            Zone::Main => Zone::Hotbar,
        };
        let n = self.slots(other).len();
        // 1. Combinar en la **primera** pila compatible; el resto se queda.
        for i in 0..n {
            let t = SlotRef {
                zone: other,
                index: i,
            };
            if let Some(s) = self.get(t)
                && s.block == stack.block
                && s.count < ItemStack::MAX
            {
                let moved = (stack.count as u32).min((ItemStack::MAX - s.count) as u32) as u8;
                self.set(t, Some(ItemStack::new(s.block, s.count + moved)));
                let rest = stack.count - moved;
                self.set(r, (rest > 0).then(|| ItemStack::new(stack.block, rest)));
                return;
            }
        }
        // 2. Un hueco vacio.
        for i in 0..n {
            let t = SlotRef {
                zone: other,
                index: i,
            };
            if self.get(t).is_none() {
                self.set(t, Some(stack));
                self.set(r, None);
                return;
            }
        }
    }

    /// Doble click: recoge del inventario todo el bloque del cursor (o de la
    /// ranura) hasta llenar el cursor.
    fn collect_all(&mut self, r: SlotRef) {
        let block = match self.cursor {
            Some(c) => c.block,
            None => match self.get(r) {
                Some(s) => s.block,
                None => return,
            },
        };
        let mut have = self.cursor.map_or(0, |c| c.count as u32);
        for zone in [Zone::Hotbar, Zone::Main] {
            let n = self.slots(zone).len();
            for i in 0..n {
                if have >= ItemStack::MAX as u32 {
                    break;
                }
                let sref = SlotRef { zone, index: i };
                if let Some(s) = self.get(sref)
                    && s.block == block
                {
                    let space = ItemStack::MAX as u32 - have;
                    let moved = (s.count as u32).min(space);
                    have += moved;
                    let rest = s.count as u32 - moved;
                    self.set(sref, (rest > 0).then(|| ItemStack::new(block, rest as u8)));
                }
            }
        }
        self.cursor = (have > 0).then(|| ItemStack::new(block, have as u8));
    }

    /// Tecla 1-9 sobre una ranura: intercambia con esa ranura de la hotbar.
    pub fn number_key(&mut self, r: SlotRef, hotbar_index: usize) {
        if hotbar_index >= HOTBAR_SLOTS {
            return;
        }
        let h = SlotRef::hotbar(hotbar_index);
        if r == h {
            return;
        }
        let a = self.get(r);
        let b = self.get(h);
        self.set(r, b);
        self.set(h, a);
    }

    /// Tecla Q: suelta una unidad (o el stack entero con `all`).
    pub fn drop_key(&mut self, r: SlotRef, all: bool) {
        let Some(s) = self.get(r) else {
            return;
        };
        if all || s.count <= 1 {
            self.set(r, None);
        } else {
            self.set(r, Some(ItemStack::new(s.block, s.count - 1)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(block: Block, count: u8) -> ItemStack {
        ItemStack::new(block, count)
    }

    /// PRNG determinista (xorshift) para el fuzz.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn arrastre_izquierdo_reparte_uniforme_sin_perder_unidades() {
        let mut inv = InventoryState::new();
        inv.set_cursor(Some(stack(Block::Stone, 10)));
        inv.press(Some(SlotRef::main(0)), Button::Left, false);
        inv.drag_enter(SlotRef::main(1));
        inv.drag_enter(SlotRef::main(2));
        inv.release();
        // 10 entre 3 = 4,3,3 (reparto uniforme).
        assert_eq!(inv.get(SlotRef::main(0)).unwrap().count, 4);
        assert_eq!(inv.get(SlotRef::main(1)).unwrap().count, 3);
        assert_eq!(inv.get(SlotRef::main(2)).unwrap().count, 3);
        assert!(inv.cursor().is_none());
        assert_eq!(inv.total(Block::Stone), 10);
    }

    #[test]
    fn arrastre_derecho_deja_uno_por_ranura() {
        let mut inv = InventoryState::new();
        inv.set_cursor(Some(stack(Block::Dirt, 5)));
        inv.press(Some(SlotRef::hotbar(0)), Button::Right, false);
        inv.drag_enter(SlotRef::hotbar(1));
        inv.drag_enter(SlotRef::hotbar(2));
        inv.release();
        assert_eq!(inv.get(SlotRef::hotbar(0)).unwrap().count, 1);
        assert_eq!(inv.get(SlotRef::hotbar(1)).unwrap().count, 1);
        assert_eq!(inv.get(SlotRef::hotbar(2)).unwrap().count, 1);
        assert_eq!(inv.cursor().unwrap().count, 2);
    }

    #[test]
    fn shift_click_cruza_hotbar_e_inventario() {
        let mut inv = InventoryState::new();
        inv.set(SlotRef::hotbar(3), Some(stack(Block::Stone, 20)));
        inv.press(Some(SlotRef::hotbar(3)), Button::Left, true);
        inv.release();
        assert!(inv.get(SlotRef::hotbar(3)).is_none());
        assert_eq!(inv.get(SlotRef::main(0)).unwrap().count, 20);
    }

    #[test]
    fn doble_click_recoge_todo_el_mismo_bloque() {
        let mut inv = InventoryState::new();
        inv.set(SlotRef::main(0), Some(stack(Block::Stone, 10)));
        inv.set(SlotRef::main(5), Some(stack(Block::Stone, 7)));
        inv.set(SlotRef::hotbar(1), Some(stack(Block::Stone, 3)));
        inv.set(SlotRef::main(2), Some(stack(Block::Dirt, 9)));
        // Primer click coge un stack; segundo (dentro de la ventana) recoge todo.
        inv.press(Some(SlotRef::main(0)), Button::Left, false);
        inv.release();
        inv.press(Some(SlotRef::main(0)), Button::Left, false);
        inv.release();
        assert_eq!(inv.cursor().unwrap().block, Block::Stone);
        assert_eq!(inv.cursor().unwrap().count, 20);
        assert_eq!(inv.total(Block::Dirt), 9, "el otro bloque no se toca");
    }

    #[test]
    fn stack_nunca_supera_64() {
        let mut inv = InventoryState::new();
        inv.set(SlotRef::main(0), Some(stack(Block::Stone, 60)));
        inv.set_cursor(Some(stack(Block::Stone, 10)));
        inv.press(Some(SlotRef::main(0)), Button::Left, false);
        inv.release();
        assert_eq!(inv.get(SlotRef::main(0)).unwrap().count, 64);
        assert_eq!(inv.cursor().unwrap().count, 6, "el resto queda en el cursor");
    }

    #[test]
    fn number_key_intercambia_con_la_hotbar() {
        let mut inv = InventoryState::new();
        inv.set(SlotRef::main(0), Some(stack(Block::Stone, 5)));
        inv.set(SlotRef::hotbar(2), Some(stack(Block::Dirt, 9)));
        inv.number_key(SlotRef::main(0), 2);
        assert_eq!(inv.get(SlotRef::hotbar(2)).unwrap().block, Block::Stone);
        assert_eq!(inv.get(SlotRef::main(0)).unwrap().block, Block::Dirt);
    }

    #[test]
    fn q_suelta_una_unidad_o_el_stack() {
        let mut inv = InventoryState::new();
        inv.set(SlotRef::main(0), Some(stack(Block::Stone, 5)));
        inv.drop_key(SlotRef::main(0), false);
        assert_eq!(inv.get(SlotRef::main(0)).unwrap().count, 4);
        inv.drop_key(SlotRef::main(0), true);
        assert!(inv.get(SlotRef::main(0)).is_none());
    }

    #[test]
    fn invariante_unidades_por_bloque_en_secuencias_aleatorias() {
        // Fuzz determinista: cualquier secuencia de clicks/arrastres/shift/1-9
        // conserva el total de unidades de cada bloque (sin soltar ni crear).
        let blocks = [Block::Stone, Block::Dirt, Block::Sand, Block::Wood];
        let mut inv = InventoryState::new();
        // Estado inicial: pilas repartidas.
        let mut rng = Rng(0x1234_5678_9abc_def0);
        for i in 0..HOTBAR_SLOTS {
            if rng.below(2) == 0 {
                inv.set(SlotRef::hotbar(i), Some(stack(blocks[rng.below(4)], 1 + rng.below(64) as u8)));
            }
        }
        for i in 0..MAIN_SLOTS {
            if rng.below(3) == 0 {
                inv.set(SlotRef::main(i), Some(stack(blocks[rng.below(4)], 1 + rng.below(64) as u8)));
            }
        }
        inv.set_cursor(Some(stack(blocks[0], 30)));

        let before: Vec<u32> = blocks.iter().map(|&b| inv.total(b)).collect();
        for _ in 0..2000 {
            inv.tick(0.05);
            let zone = if rng.below(2) == 0 { Zone::Hotbar } else { Zone::Main };
            let n = if zone == Zone::Hotbar { HOTBAR_SLOTS } else { MAIN_SLOTS };
            let slot = SlotRef { zone, index: rng.below(n) };
            let button = if rng.below(2) == 0 { Button::Left } else { Button::Right };
            match rng.below(4) {
                0 => {
                    inv.press(Some(slot), button, false);
                    inv.release();
                }
                1 => {
                    inv.press(Some(slot), Button::Left, false);
                    for _ in 0..rng.below(4) {
                        inv.drag_enter(SlotRef {
                            zone,
                            index: rng.below(n),
                        });
                    }
                    inv.release();
                }
                2 => {
                    inv.press(Some(slot), button, true);
                    inv.release();
                }
                _ => inv.number_key(slot, rng.below(HOTBAR_SLOTS)),
            }
            // Tras cada paso, ningun stack supera 64.
            for r in (0..HOTBAR_SLOTS).map(SlotRef::hotbar).chain((0..MAIN_SLOTS).map(SlotRef::main)) {
                if let Some(s) = inv.get(r) {
                    assert!(s.count <= ItemStack::MAX, "stack > 64 en {r:?}");
                }
            }
        }
        let after: Vec<u32> = blocks.iter().map(|&b| inv.total(b)).collect();
        assert_eq!(before, after, "se perdieron o duplicaron unidades");
    }
}
