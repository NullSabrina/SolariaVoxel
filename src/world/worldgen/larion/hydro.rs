//! **Erosion hidrica y red de drenaje** (P0.1) sobre una ventana global.
//!
//! La generacion es pura por columna y corre en varios workers, asi que no se
//! puede hacer un flujo global por columna. En su lugar se construye una
//! **ventana coarse** determinista alrededor del jugador:
//!
//! 1. `h0`: altura geografica (sin clima, bioma ni rios) en una rejilla coarse.
//! 2. `priority_flood`: rellena depresiones para que toda celda drene.
//! 3. D8 con desempate por hash: direccion de flujo.
//! 4. Acumulacion topologica del area de drenaje.
//! 5. *Stream power* `K·A^m·S^n` + deposicion en pendientes bajas.
//! 6. Suavizado, mascara de rio, distancia al eje y ancho por `sqrt(A)`.
//!
//! Las columnas solo **muestrean** la ventana con interpolacion bilineal. La
//! cache es thread-safe y construye fuera del lock.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

use super::LarionGenerator;
use super::config::HydroConfig;
use crate::world::worldgen::math;

/// Bloques por celda de la ventana.
pub const CELL: i32 = 4;
/// Lado de la ventana interior, en bloques.
pub const WINDOW: i32 = 2048;
/// Margen alrededor de la ventana para que el flujo tenga cuenca.
pub const HALO: i32 = 128;
/// Ventanas cacheadas antes de evacuar por FIFO.
pub const MAX_WINDOWS: usize = 2;

/// Paso de muestreo de la geografia base. La rejilla de flujo sigue siendo
/// `CELL`; el heightmap grande se muestrea mas espaciado y se interpola para que
/// construir una ventana no bloquee un worker durante segundos.
const GEO_STEP: i32 = 16;

const MAX_DISTANCE: f32 = 128.0;
const SQRT_2: f32 = std::f32::consts::SQRT_2;
const INF: f32 = 1.0e9;

/// Muestra interpolada de la ventana en un punto del mundo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HydroSample {
    /// Incision (negativa) + deposicion (positiva), en bloques.
    pub dh: f32,
    /// Area de drenaje normalizada `[0, 1]`.
    pub flow: f32,
    /// Ancho del cauce en bloques.
    pub width: f32,
    /// Distancia al eje del cauce en bloques.
    pub dist: f32,
    /// Nivel de agua (0 = sin agua).
    pub water: f32,
    /// Altura de la caida en bloques (0 = no hay cascada).
    pub waterfall: f32,
}

impl HydroSample {
    /// Perfil en V del cauce: profundidad maxima en el eje, 0 en el borde.
    pub fn v_depth(&self, cfg: &HydroConfig) -> f32 {
        let depth = math::lerp(cfg.river_min_depth, cfg.river_max_depth, self.flow.sqrt());
        let t = (self.dist / self.width.max(1.0)).clamp(0.0, 1.0);
        depth * (1.0 - t * t).powi(2)
    }

    /// Proximidad al cauce `[0, 1]` (1 en el eje, 0 fuera del ancho).
    pub fn proximity(&self) -> f32 {
        let t = (self.dist / self.width.max(1.0)).clamp(0.0, 1.0);
        (1.0 - t).powi(2)
    }
}

/// Ventana de drenaje ya construida. Inmutable y `Send + Sync`.
pub struct HydroWindow {
    key: (i32, i32),
    grid_origin: (i32, i32),
    size: usize,
    dh: Vec<f32>,
    flow: Vec<f32>,
    width: Vec<f32>,
    dist: Vec<f32>,
    water: Vec<f32>,
    waterfall: Vec<f32>,
}

impl HydroWindow {
    /// Clave de la ventana que contiene `(x, z)`.
    pub fn key_for(x: i32, z: i32) -> (i32, i32) {
        (x.div_euclid(WINDOW), z.div_euclid(WINDOW))
    }

    /// Construye la ventana. Es pura respecto de `generator`: mismo mundo, mismos
    /// bytes. El coste se amortiza entre miles de columnas.
    pub fn build(generator: &LarionGenerator, key: (i32, i32)) -> Self {
        let origin = (key.0 * WINDOW, key.1 * WINDOW);
        let grid_origin = (origin.0 - HALO, origin.1 - HALO);
        let size = ((WINDOW + 2 * HALO) / CELL) as usize;
        let n = size * size;
        const { assert!((WINDOW + 2 * HALO) % CELL == 0) };

        // 1. Heightmap base. Se muestrea a `GEO_STEP` (sin detalle fino) y se
        //    interpola a la rejilla de flujo. El detalle de 2 bloques no cambia
        //    la red de drenaje y dispararia el coste.
        let csize = ((WINDOW + 2 * HALO) / GEO_STEP) as usize;
        let mut coarse = vec![0.0f32; csize * csize];
        for cj in 0..csize {
            for ci in 0..csize {
                let wx = grid_origin.0 + ci as i32 * GEO_STEP + GEO_STEP / 2;
                let wz = grid_origin.1 + cj as i32 * GEO_STEP + GEO_STEP / 2;
                coarse[cj * csize + ci] = generator.geography_height(wx as f64, wz as f64);
            }
        }
        let mut h0 = vec![0.0f32; n];
        for j in 0..size {
            for i in 0..size {
                let u = (i as f32 + 0.5) * CELL as f32 / GEO_STEP as f32 - 0.5;
                let v = (j as f32 + 0.5) * CELL as f32 / GEO_STEP as f32 - 0.5;
                h0[j * size + i] = bilinear(&coarse, csize, u, v);
            }
        }

        // 2. Priority-flood determinista.
        let filled = priority_flood(&h0, size);

        // 3-4. D8 + acumulacion topologica.
        let key_vals: Vec<f32> = (0..n)
            .map(|i| filled[i] + 1.0e-5 * (hash01_index(i) - 0.5))
            .collect();
        let down = downhill(&key_vals, size);
        let cfg = generator.config().hydro;
        // Cascadas: caida entre la celda y su vecino D8, suavizada aguas abajo.
        let waterfall = waterfall_field(&filled, &down, cfg.waterfall_min);
        let flow = accumulate(&down);
        let max_flow = flow.iter().copied().fold(0.0f32, f32::max).max(1.0);
        let flow01: Vec<f32> = flow
            .iter()
            .map(|f| (f / max_flow).clamp(0.0, 1.0))
            .collect();

        // 5. Stream power + deposicion.
        let mut dh = vec![0.0f32; n];
        for i in 0..n {
            let a = flow01[i];
            let Some(d) = down[i] else {
                continue;
            };
            let d = d as usize;
            let (dx, dz) = neighbor_delta(i, d, size);
            let dist_cells = if dx != 0 && dz != 0 { SQRT_2 } else { 1.0 };
            // Pendiente en bloques por bloque (la celda de la rejilla mide CELL).
            let slope =
                ((key_vals[i] - key_vals[d]).max(0.0) / (dist_cells * CELL as f32)).max(0.0);
            let incision = (cfg.k * a.powf(cfg.m) * slope.powf(cfg.n)).min(cfg.max_incision);
            let deposition = if slope < cfg.slope_dep {
                (cfg.deposition * (1.0 - slope / cfg.slope_dep) * a).min(cfg.max_deposition)
            } else {
                0.0
            };
            // Incision negativa + deposicion positiva.
            dh[i] = deposition - incision;
        }
        smooth_3x3(&mut dh, size);
        smooth_3x3(&mut dh, size);

        // 6. Cauce: mascara por area, distancia al eje y ancho.
        let river: Vec<bool> = flow01.iter().map(|&f| f > cfg.river_min_flow).collect();
        let dist = distance_transform(&river, size);
        let width: Vec<f32> = flow01
            .iter()
            .map(|&a| math::lerp(cfg.river_min_width, cfg.river_max_width, a.sqrt()))
            .collect();
        let mut water = vec![0.0f32; n];
        for i in 0..n {
            let lake = filled[i] > h0[i] + 0.25;
            // Todo el **cauce** (no solo la linea de flujo D8) se llena hasta
            // la superficie del priority-flood. Antes solo la mascara
            // `flow > river_min_flow` recibia agua: el V excavado era ancho
            // pero el agua quedaba en una lama de 1-2 bloques en el fondo (el
            // "rio" estrecho de la captura). Minecraft llena el canal completo.
            let channel = dist[i] <= width[i];
            if lake || channel {
                water[i] = filled[i];
            }
        }

        Self {
            key,
            grid_origin,
            size,
            dh,
            flow: flow01,
            width,
            dist,
            water,
            waterfall,
        }
    }

    /// Clave de la ventana.
    pub fn key(&self) -> (i32, i32) {
        self.key
    }

    /// Lado de la rejilla (celdas).
    pub fn size(&self) -> usize {
        self.size
    }

    /// Muestrea con interpolacion bilineal. Coordenadas de mundo en bloques.
    pub fn sample(&self, x: f32, z: f32) -> HydroSample {
        let u = (x - self.grid_origin.0 as f32) / CELL as f32 - 0.5;
        let v = (z - self.grid_origin.1 as f32) / CELL as f32 - 0.5;
        HydroSample {
            dh: bilinear(&self.dh, self.size, u, v),
            flow: bilinear(&self.flow, self.size, u, v).clamp(0.0, 1.0),
            width: bilinear(&self.width, self.size, u, v).max(1.0),
            dist: bilinear(&self.dist, self.size, u, v).max(0.0),
            water: bilinear(&self.water, self.size, u, v),
            waterfall: bilinear(&self.waterfall, self.size, u, v).max(0.0),
        }
    }

    #[cfg(test)]
    fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut step = |v: f32| {
            h ^= v.to_bits() as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for i in 0..self.size * self.size {
            step(self.dh[i]);
            step(self.flow[i]);
            step(self.width[i]);
            step(self.dist[i]);
            step(self.water[i]);
            step(self.waterfall[i]);
        }
        h
    }
}

/// Cache thread-safe de ventanas. Construye fuera del lock y evacua FIFO.
pub struct HydroCache {
    map: RwLock<HashMap<(i32, i32), Arc<HydroWindow>>>,
    order: Mutex<VecDeque<(i32, i32)>>,
    builds: std::sync::atomic::AtomicUsize,
}

impl Default for HydroCache {
    fn default() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            builds: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl HydroCache {
    /// Devuelve la ventana que contiene `(x, z)`, construyendola si hace falta.
    /// Numero de ventanas construidas (diagnostico/benchmarks).
    pub fn builds(&self) -> usize {
        self.builds.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn get(&self, generator: &LarionGenerator, x: i32, z: i32) -> Arc<HydroWindow> {
        let key = HydroWindow::key_for(x, z);
        if let Some(window) = self.map.read().unwrap().get(&key) {
            return Arc::clone(window);
        }
        // Construir fuera del lock: otros workers pueden seguir con su ventana.
        let built = Arc::new(HydroWindow::build(generator, key));
        self.builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut map = self.map.write().unwrap();
        if let Some(existing) = map.get(&key) {
            return Arc::clone(existing);
        }
        map.insert(key, Arc::clone(&built));
        let mut order = self.order.lock().unwrap();
        order.push_back(key);
        while order.len() > MAX_WINDOWS {
            if let Some(old) = order.pop_front() {
                if old != key {
                    map.remove(&old);
                }
            }
        }
        built
    }
}

/// Rellena depresiones: el resultado es un heightmap que drena a los bordes.
fn priority_flood(h: &[f32], size: usize) -> Vec<f32> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let n = size * size;
    let mut filled = h.to_vec();
    let mut seen = vec![false; n];
    let mut heap: BinaryHeap<Reverse<(i32, u32)>> = BinaryHeap::new();

    for j in 0..size {
        for i in 0..size {
            if i == 0 || j == 0 || i + 1 == size || j + 1 == size {
                let idx = j * size + i;
                if !seen[idx] {
                    seen[idx] = true;
                    heap.push(Reverse((quant(filled[idx]), idx as u32)));
                }
            }
        }
    }
    while let Some(Reverse((_, idx))) = heap.pop() {
        let idx = idx as usize;
        let (i, j) = (idx % size, idx / size);
        for (di, dj) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let ni = i as i32 + di;
            let nj = j as i32 + dj;
            if ni < 0 || nj < 0 || ni >= size as i32 || nj >= size as i32 {
                continue;
            }
            let nidx = nj as usize * size + ni as usize;
            if !seen[nidx] {
                filled[nidx] = filled[idx].max(h[nidx]);
                seen[nidx] = true;
                heap.push(Reverse((quant(filled[nidx]), nidx as u32)));
            }
        }
    }
    filled
}

/// Direccion de flujo D8: el vecino con menor `key` (mas pendiente). Empates por
/// hash global, nunca por orden de `HashMap`.
fn downhill(key: &[f32], size: usize) -> Vec<Option<u32>> {
    const D8: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    let mut down = vec![None; size * size];
    for j in 0..size {
        for i in 0..size {
            let idx = j * size + i;
            let mut best = idx;
            let mut best_key = key[idx];
            let mut best_tie = hash01_index(idx);
            for (di, dj) in D8 {
                let ni = i as i32 + di;
                let nj = j as i32 + dj;
                if ni < 0 || nj < 0 || ni >= size as i32 || nj >= size as i32 {
                    continue;
                }
                let nidx = nj as usize * size + ni as usize;
                let tie = hash01_index(nidx);
                if key[nidx] < best_key || (key[nidx] == best_key && tie < best_tie) {
                    best = nidx;
                    best_key = key[nidx];
                    best_tie = tie;
                }
            }
            if best != idx {
                down[idx] = Some(best as u32);
            }
        }
    }
    down
}

/// Altura de cascada por celda: caida vertical entre `filled[i]` y su vecino D8,
/// suavizada **aguas abajo** con dos pasadas para que la transicion no sea un
/// escalon de un solo bloque.
///
/// Solo se marca una celda si la caida supera `min_drop`; una celda plana
/// (incluida una salida sin `down`) queda a 0. La operacion es determinista:
/// recorre indices en orden y no depende de `HashMap`.
fn waterfall_field(filled: &[f32], down: &[Option<u32>], min_drop: f32) -> Vec<f32> {
    let n = filled.len();
    let mut field = vec![0.0f32; n];
    for i in 0..n {
        let Some(d) = down[i] else { continue };
        let d = d as usize;
        if d >= n {
            continue;
        }
        let drop = (filled[i] - filled[d]).max(0.0);
        if drop.is_finite() && drop > min_drop {
            field[i] = drop;
        }
    }
    // Kernel 1D a lo largo de `down`: la mitad de la cascada pasa a la celda
    // siguiente, dos veces (transicion de ~2 celdas de rejilla).
    for _ in 0..2 {
        let src = field.clone();
        for (i, value) in src.iter().copied().enumerate() {
            if value <= 0.0 {
                continue;
            }
            let Some(d) = down[i] else { continue };
            let d = d as usize;
            if d < n {
                field[d] = field[d].max(value * 0.5);
            }
        }
    }
    for v in &mut field {
        if !v.is_finite() || *v < 0.0 {
            *v = 0.0;
        }
    }
    field
}

/// Acumulacion topologica: cada celda aporta 1 y todo su upstream.
fn accumulate(down: &[Option<u32>]) -> Vec<f32> {
    let n = down.len();
    let mut indegree = vec![0u32; n];
    for d in down.iter().flatten() {
        indegree[*d as usize] += 1;
    }
    let mut queue: VecDeque<u32> = (0..n as u32)
        .filter(|&i| indegree[i as usize] == 0)
        .collect();
    let mut flow = vec![1.0f32; n];
    while let Some(i) = queue.pop_front() {
        if let Some(d) = down[i as usize] {
            let d = d as usize;
            flow[d] += flow[i as usize];
            indegree[d] -= 1;
            if indegree[d] == 0 {
                queue.push_back(d as u32);
            }
        }
    }
    flow
}

/// Distancia (en bloques) a la celda de rio mas cercana, con chamfer 3x3.
fn distance_transform(river: &[bool], size: usize) -> Vec<f32> {
    let n = size * size;
    let mut dist = vec![INF; n];
    for i in 0..n {
        if river[i] {
            dist[i] = 0.0;
        }
    }
    let relax = |i: usize, j: usize, di: i32, dj: i32, w: f32, dist: &mut Vec<f32>| {
        let ni = i as i32 + di;
        let nj = j as i32 + dj;
        if ni < 0 || nj < 0 || ni >= size as i32 || nj >= size as i32 {
            return;
        }
        let nidx = nj as usize * size + ni as usize;
        let cand = dist[nidx] + w;
        if cand < dist[j * size + i] {
            dist[j * size + i] = cand;
        }
    };
    for j in 0..size {
        for i in 0..size {
            for (di, dj, w) in [
                (-1, 0, 1.0),
                (0, -1, 1.0),
                (-1, -1, SQRT_2),
                (1, -1, SQRT_2),
            ] {
                relax(i, j, di, dj, w, &mut dist);
            }
        }
    }
    for j in (0..size).rev() {
        for i in (0..size).rev() {
            for (di, dj, w) in [(1, 0, 1.0), (0, 1, 1.0), (1, 1, SQRT_2), (-1, 1, SQRT_2)] {
                relax(i, j, di, dj, w, &mut dist);
            }
        }
    }
    for d in &mut dist {
        *d = (*d * CELL as f32).min(MAX_DISTANCE);
    }
    dist
}

/// Suavizado 3x3 (media con los vecinos validos).
fn smooth_3x3(field: &mut [f32], size: usize) {
    let src = field.to_vec();
    for j in 0..size {
        for i in 0..size {
            let mut sum = 0.0f32;
            let mut count = 0.0f32;
            for dj in -1i32..=1 {
                for di in -1i32..=1 {
                    let ni = i as i32 + di;
                    let nj = j as i32 + dj;
                    if ni < 0 || nj < 0 || ni >= size as i32 || nj >= size as i32 {
                        continue;
                    }
                    sum += src[nj as usize * size + ni as usize];
                    count += 1.0;
                }
            }
            field[j * size + i] = sum / count;
        }
    }
}

/// Bilineal con clamp en los bordes.
fn bilinear(field: &[f32], size: usize, u: f32, v: f32) -> f32 {
    let u = u.clamp(0.0, size as f32 - 1.0);
    let v = v.clamp(0.0, size as f32 - 1.0);
    let i0 = u.floor() as usize;
    let j0 = v.floor() as usize;
    let i1 = (i0 + 1).min(size - 1);
    let j1 = (j0 + 1).min(size - 1);
    let tx = u - i0 as f32;
    let ty = v - j0 as f32;
    let a = field[j0 * size + i0];
    let b = field[j0 * size + i1];
    let c = field[j1 * size + i0];
    let d = field[j1 * size + i1];
    a * (1.0 - tx) * (1.0 - ty) + b * tx * (1.0 - ty) + c * (1.0 - tx) * ty + d * tx * ty
}

/// Altura cuantizada a 1/64: orden total determinista para el heap.
fn quant(h: f32) -> i32 {
    (h * 64.0).round() as i32
}

/// Hash global de una celda de la rejilla (no depende de HashMap).
fn hash01_index(i: usize) -> f32 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

/// Diferencia `(dx, dz)` entre dos indices (para la distancia euclidea de D8).
fn neighbor_delta(i: usize, n: usize, size: usize) -> (i32, i32) {
    let (xi, xj) = (i % size, i / size);
    let (ni, nj) = (n % size, n / size);
    (
        (ni as i32 - xi as i32).signum(),
        (nj as i32 - xj as i32).signum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_flood_rellena_una_depresion() {
        // 5x5 con borde a 10 y un pozo central a 0: debe llenarse a 10.
        let mut h = vec![10.0f32; 25];
        h[12] = 0.0;
        let filled = priority_flood(&h, 5);
        assert!((filled[12] - 10.0).abs() < 1e-4, "pozo sin rellenar");
    }

    #[test]
    fn test_distance_transform_es_correcta() {
        let mut mask = vec![false; 25];
        mask[12] = true; // centro de 5x5
        let d = distance_transform(&mask, 5);
        // Esquina a (0,0): sqrt(2)*2 celdas * CELL.
        let expected = (2.0 * SQRT_2) * CELL as f32;
        assert!((d[0] - expected).abs() < 1e-3, "d={}", d[0]);
        assert_eq!(d[12], 0.0);
    }

    #[test]
    fn test_window_deterministic_1_vs_4_workers() {
        let generator = LarionGenerator::with_version(
            13_371,
            crate::world::worldgen::larion::LarionConfig::default(),
            2,
        );
        let expected = HydroWindow::build(&generator, (0, 0)).hash();
        let mut hashes = Vec::new();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..4 {
                handles.push(scope.spawn(|| HydroWindow::build(&generator, (0, 0)).hash()));
            }
            for handle in handles {
                hashes.push(handle.join().expect("worker de test"));
            }
        });
        assert!(
            hashes.iter().all(|&h| h == expected),
            "la ventana no es determinista entre workers: {hashes:?} vs {expected:016x}"
        );
    }

    #[test]
    fn test_waterfall_only_on_steep_river() {
        // Rama con cascada 0 -> 1 -> 2 (caida de 5) y rama plana 3 -> 4 -> 5.
        // El suavizado no debe inventar cascada en la rama plana (celda 3).
        let filled = [10.0f32, 5.0, 5.0, 20.0, 20.0, 20.0];
        let down = [Some(1), Some(2), None, Some(4), Some(5), None];
        let field = waterfall_field(&filled, &down, 1.0);
        assert!(
            field[0] > 0.0,
            "la celda con slope > WATERFALL_MIN debe marcar cascada"
        );
        assert!(
            (field[0] - 5.0).abs() < 1e-6,
            "la altura debe ser filled[i] - filled[down[i]]: {}",
            field[0]
        );
        assert_eq!(field[3], 0.0, "una celda plana no es cascada");
        assert_eq!(field[5], 0.0, "la salida de la cuenca no es cascada");
        // El suavizado crea una transicion aguas abajo, no un escalon.
        assert!(field[1] > 0.0 && field[1] < field[0]);
    }

    #[test]
    fn test_waterfall_field_es_determinista() {
        let filled = [10.0f32, 5.0, 1.0, 5.0, 5.0];
        let down = [Some(1), Some(2), Some(3), Some(2), None];
        let a = waterfall_field(&filled, &down, 1.0);
        let b = waterfall_field(&filled, &down, 1.0);
        assert_eq!(a, b, "el campo de cascadas debe ser determinista");
        assert!(a.iter().all(|v| v.is_finite() && *v >= 0.0));
    }

    #[test]
    fn test_hydro_sample_es_finito_y_acotado() {
        let generator = LarionGenerator::with_version(
            7,
            crate::world::worldgen::larion::LarionConfig::default(),
            2,
        );
        let window = HydroWindow::build(&generator, (0, 0));
        for (x, z) in [(0.0, 0.0), (123.5, -77.25), (-2000.0, 3000.0)] {
            let s = window.sample(x, z);
            assert!(s.dh.is_finite());
            assert!(s.flow.is_finite() && (0.0..=1.0).contains(&s.flow));
            assert!(s.width >= 1.0);
            assert!(s.dist >= 0.0);
            assert!(s.water.is_finite());
            assert!(s.waterfall.is_finite() && s.waterfall >= 0.0);
        }
    }
}
