//! Cuevas **jerarquicas** (FASE 6): varios sistemas que se suman en vez de un
//! unico `|perlin| < umbral`.
//!
//! Cada sistema vive en su propia escala de frecuencia y se activa con una
//! condicion barata, de modo que el caso comun (sin cueva) **sale pronto** y no
//! paga los ruidos que no le tocan:
//!
//! | sistema  | forma                                             | coste |
//! |----------|---------------------------------------------------|-------|
//! | spaghetti| 2 campos de tubos delgados que se cruzan          | 3D    |
//! | regional | tubos anchos de baja frecuencia                   | 3D    |
//! | cheese   | camaras grandes (blobs) en regiones profundas     | 3D    |
//! | shaft    | pozos verticales (tubo casi constante en Y)       | 3D    |
//! | canyon   | canones largos en X, en banda media               | 3D    |
//! | entrada  | grietas/sinkholes que rompen la corteza (raras)   | 3D    |
//!
//! `pillar` es una **mascara de preservacion**: columnas solidas que sobreviven
//! dentro de las camaras (pilares y "puentes" naturales). La densidad se
//! **atenua** por profundidad (`surface - y`) y se **refuerza** bajo montanas
//! (`mountain_mask`): de noche, la corteza se respeta salvo en las entradas.
//!
//! Todo es determinista por semilla. El trabajo 2D (region de camaras, pozos,
//! canones, entradas) se calcula **una vez por columna** en [`CaveContext`]; el
//! bucle por voxel solo paga los campos 3D que se activan.

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Profundidad por debajo de la cual no se cava (bedrock).
pub const BEDROCK_CLEAR: i32 = 5;

/// Corteza que las cuevas **no** perforan bajo la superficie (salvo entradas).
pub const CAVE_CRUST: i32 = 2;

/// Profundidad a la que la densidad de cueva es maxima.
const FULL_DEPTH_Y: i32 = 10;

// Ancho de cada sistema, en unidades del campo (|n| < ancho => hueco). La
// atenuacion los estrecha cerca de la superficie.
const TUBE_A_WIDTH: f64 = 0.055;
const TUBE_B_WIDTH: f64 = 0.045;
const REGION_WIDTH: f64 = 0.085;
const SHAFT_WIDTH: f64 = 0.16;
const CANYON_WIDTH: f64 = 0.10;

/// Umbral del campo `cheese` (blobs de camara).
const CHEESE_THRESHOLD: f64 = 0.34;

/// La corteza que se preserva sobre las camaras (que no salgan cuevas enormes a
/// flor de piel en las cumbres).
const CHEESE_CRUST_KEEP: i32 = 16;

/// Cuanto refuerza una montana la densidad de cueva (0 bajo el mar, 1 en pico).
const MOUNTAIN_CAVE_BOOST: f64 = 0.60;

/// Profundidad (bajo la superficie) que puede abrir una entrada.
const ENTRANCE_DEPTH: i32 = 7;

/// Lado de la celda de `WormField` en X/Z (bloques).
pub const WORM_CELL: i32 = 64;
/// Worms por celda.
pub const WORMS_PER_CELL: usize = 2;
/// Maximo de worms base + conectores que se guardan en un contexto.
pub const MAX_WORMS: usize = 26;

/// Tramo de un worm: cubica de Bezier + radio de excavacion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Worm {
    a: [f32; 3],
    c1: [f32; 3],
    c2: [f32; 3],
    b: [f32; 3],
    radius: f32,
}

impl Worm {
    const ZERO: Self = Self {
        a: [0.0; 3],
        c1: [0.0; 3],
        c2: [0.0; 3],
        b: [0.0; 3],
        radius: 0.0,
    };

    fn aabb(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = self.a;
        let mut hi = self.a;
        for p in [self.c1, self.c2, self.b] {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        for i in 0..3 {
            lo[i] -= self.radius;
            hi[i] += self.radius;
        }
        (lo, hi)
    }

    /// Distancia minima aproximada (8 muestras) al eje del worm.
    fn distance2(&self, p: [f32; 3]) -> f32 {
        let mut best = f32::INFINITY;
        for s in 0..=8 {
            let t = s as f32 / 8.0;
            let u = 1.0 - t;
            let x = u * u * u * self.a[0]
                + 3.0 * u * u * t * self.c1[0]
                + 3.0 * u * t * t * self.c2[0]
                + t * t * t * self.b[0];
            let y = u * u * u * self.a[1]
                + 3.0 * u * u * t * self.c1[1]
                + 3.0 * u * t * t * self.c2[1]
                + t * t * t * self.b[1];
            let z = u * u * u * self.a[2]
                + 3.0 * u * u * t * self.c1[2]
                + 3.0 * u * t * t * self.c2[2]
                + t * t * t * self.b[2];
            let dx = p[0] - x;
            let dy = p[1] - y;
            let dz = p[2] - z;
            best = best.min(dx * dx + dy * dy + dz * dz);
        }
        best
    }
}

/// Red de worms determinista (P1.4). Dos worms por celda de 64 bloques,
/// conectados a un vecino con probabilidad 0.7 mediante un tubo recto.
#[derive(Clone, Copy, Debug)]
pub struct WormField {
    seed: u64,
}

impl WormField {
    pub fn new(seed: u32) -> Self {
        Self {
            seed: seed as u64 ^ 0xA24B_AED4_963E_E407,
        }
    }

    fn cell_seed(&self, cx: i32, cz: i32, k: usize) -> u64 {
        mix64(
            self.seed
                ^ (cx as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (cz as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
                ^ (k as u64).wrapping_mul(0x94D0_49BB_1331_11EB),
        )
    }

    fn worm(&self, cx: i32, cz: i32, k: usize) -> Worm {
        let mut rng = SplitMix64::new(self.cell_seed(cx, cz, k));
        let ox = (cx * WORM_CELL) as f32;
        let oz = (cz * WORM_CELL) as f32;
        let a = [
            ox + 8.0 + rng.next_f32() * 48.0,
            12.0 + rng.next_f32() * 18.0,
            oz + 8.0 + rng.next_f32() * 48.0,
        ];
        let b = [
            ox + 8.0 + rng.next_f32() * 48.0,
            42.0 + rng.next_f32() * 14.0,
            oz + 8.0 + rng.next_f32() * 48.0,
        ];
        let c1 = [
            a[0] + (rng.next_f32() * 2.0 - 1.0) * 12.0,
            a[1] + 6.0 + rng.next_f32() * 12.0,
            a[2] + (rng.next_f32() * 2.0 - 1.0) * 12.0,
        ];
        let c2 = [
            b[0] + (rng.next_f32() * 2.0 - 1.0) * 12.0,
            b[1] - 8.0 - rng.next_f32() * 10.0,
            b[2] + (rng.next_f32() * 2.0 - 1.0) * 12.0,
        ];
        Worm {
            a,
            c1,
            c2,
            b,
            radius: 1.4 + rng.next_f32() * 1.6,
        }
    }

    /// Worms que pueden afectar a `(x, z)`: 3x3 celdas + conectores del centro
    /// hacia sus cuatro vecinos ortogonales.
    pub fn context(&self, x: i32, z: i32) -> ([Worm; MAX_WORMS], u8) {
        let cx = x.div_euclid(WORM_CELL);
        let cz = z.div_euclid(WORM_CELL);
        let mut out = [Worm::ZERO; MAX_WORMS];
        let mut n = 0usize;
        for dz in -1..=1 {
            for dx in -1..=1 {
                for k in 0..WORMS_PER_CELL {
                    if n < MAX_WORMS {
                        out[n] = self.worm(cx + dx, cz + dz, k);
                        n += 1;
                    }
                }
            }
        }
        // Conectores: 0.7 de probabilidad por worm y vecino ortogonal.
        for (dir, dx, dz) in [(0, 1, 0), (1, -1, 0), (2, 0, 1), (3, 0, -1)] {
            for k in 0..WORMS_PER_CELL {
                let h = mix64(
                    self.cell_seed(cx, cz, k) ^ (dir as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93),
                );
                if (h & 0xFFFF) as f32 / 65536.0 >= 0.7 || n >= MAX_WORMS {
                    continue;
                }
                let a = self.worm(cx, cz, k);
                let b = self.worm(cx + dx, cz + dz, k);
                let start = a.b;
                let end = b.a;
                out[n] = Worm {
                    a: start,
                    c1: [
                        start[0] + (end[0] - start[0]) * 0.33,
                        start[1] + (end[1] - start[1]) * 0.33,
                        start[2] + (end[2] - start[2]) * 0.33,
                    ],
                    c2: [
                        start[0] + (end[0] - start[0]) * 0.66,
                        start[1] + (end[1] - start[1]) * 0.66,
                        start[2] + (end[2] - start[2]) * 0.66,
                    ],
                    b: end,
                    radius: 1.2,
                };
                n += 1;
            }
        }
        (out, n as u8)
    }
}

/// ¿Un worm del contexto excava `(x, y, z)`? AABB barata antes de la cubica.
fn worm_hollow(ctx: &CaveContext, x: i32, y: i32, z: i32) -> bool {
    if ctx.worm_count == 0 {
        return false;
    }
    let p = [x as f32, y as f32, z as f32];
    for w in &ctx.worms[..ctx.worm_count as usize] {
        let (lo, hi) = w.aabb();
        if p[0] < lo[0]
            || p[1] < lo[1]
            || p[2] < lo[2]
            || p[0] > hi[0]
            || p[1] > hi[1]
            || p[2] > hi[2]
        {
            continue;
        }
        let r2 = w.radius * w.radius;
        if w.distance2(p) <= r2 {
            return true;
        }
    }
    false
}

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }
}

/// Resultado de evaluar una celda.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Carve {
    None,
    Air,
    Water,
}

/// Datos 2D de una columna que deciden **que** sistemas pueden activarse aqui.
/// Se calcula una vez por `(x, z)` (4 ruidos 2D) y se reutiliza en cada voxel.
#[derive(Clone, Copy, Debug)]
pub struct CaveContext {
    /// Mascara de montana `0..1` (viene del worldgen).
    pub mountain: f32,
    /// `0..1`: regiones donde pueden abrirse camaras `cheese`.
    pub cheese_region: f64,
    /// `0..1`: probabilidad local de pozo vertical.
    pub shaft: f64,
    /// `0..1`: probabilidad local de canon.
    pub canyon: f64,
    /// `0..1`: probabilidad local de entrada (rompe corteza).
    pub entrance: f64,
    /// Worms (P1.4) que pueden afectar a esta columna. `worm_count == 0` en v1.
    pub worms: [Worm; MAX_WORMS],
    pub worm_count: u8,
}

/// El sistema de cuevas, determinista por semilla.
pub struct CaveSystem {
    tubes_a: Fbm<Perlin>,
    tubes_b: Fbm<Perlin>,
    regional: Fbm<Perlin>,
    cheese: Fbm<Perlin>,
    shaft_shape: Fbm<Perlin>,
    canyon_shape: Fbm<Perlin>,
    /// Mascara de preservacion (pilares/puentes dentro de las camaras).
    pillar: Fbm<Perlin>,
    // --- campos 2D (una muestra por columna) ---
    cheese_region: Fbm<Perlin>,
    shaft_region: Fbm<Perlin>,
    canyon_region: Perlin,
    entrance_region: Fbm<Perlin>,
    /// Red de worms (P1.4), solo en `generator_config >= 3`.
    worms: Option<WormField>,
}

impl CaveSystem {
    pub fn new(seed: u32) -> Self {
        Self::with_worms(seed, false)
    }

    /// Igual que [`CaveSystem::new`] pero activando la red de worms.
    pub fn new_with_worms(seed: u32) -> Self {
        Self::with_worms(seed, true)
    }

    fn with_worms(seed: u32, worms: bool) -> Self {
        let s = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        Self {
            // Tubos delgados: dos Fbm de frecuencia distinta se cruzan -> red de
            // galerias, no tubos paralelos.
            tubes_a: Fbm::<Perlin>::new(s(11))
                .set_octaves(3)
                .set_frequency(0.055)
                .set_persistence(0.5),
            tubes_b: Fbm::<Perlin>::new(s(13))
                .set_octaves(3)
                .set_frequency(0.075)
                .set_persistence(0.5),
            // Tubos regionales: mas anchos y de baja frecuencia.
            regional: Fbm::<Perlin>::new(s(17))
                .set_octaves(2)
                .set_frequency(0.014)
                .set_persistence(0.5),
            // Camaras: frecuencia muy baja -> blobs grandes.
            cheese: Fbm::<Perlin>::new(s(19))
                .set_octaves(3)
                .set_frequency(0.006)
                .set_persistence(0.5),
            // Pozos: varia lento en Y -> el tubo es casi vertical.
            shaft_shape: Fbm::<Perlin>::new(s(23))
                .set_octaves(2)
                .set_frequency(1.0)
                .set_persistence(0.5),
            // Canones: anisotropo (se evalua con escalas por eje distintas).
            canyon_shape: Fbm::<Perlin>::new(s(29))
                .set_octaves(2)
                .set_frequency(1.0)
                .set_persistence(0.5),
            pillar: Fbm::<Perlin>::new(s(37))
                .set_octaves(2)
                .set_frequency(0.09)
                .set_persistence(0.5),
            cheese_region: Fbm::<Perlin>::new(s(41))
                .set_octaves(2)
                .set_frequency(0.004)
                .set_persistence(0.5),
            shaft_region: Fbm::<Perlin>::new(s(43))
                .set_octaves(2)
                .set_frequency(0.02)
                .set_persistence(0.5),
            canyon_region: Perlin::new(s(47)),
            entrance_region: Fbm::<Perlin>::new(s(53))
                .set_octaves(2)
                .set_frequency(0.006)
                .set_persistence(0.5),
            worms: worms.then(|| WormField::new(seed)),
        }
    }

    /// Calcula el contexto 2D de la columna `(x, z)`. `mountain` es la mascara
    /// de montana del worldgen (`0..1`).
    pub fn context(&self, x: i32, z: i32, mountain: f32) -> CaveContext {
        let (fx, fz) = (x as f64, z as f64);
        let n01 = |n: f64| (n * 0.5 + 0.5).clamp(0.0, 1.0);
        let (worms, worm_count) = match self.worms {
            Some(w) => w.context(x, z),
            None => ([Worm::ZERO; MAX_WORMS], 0),
        };
        CaveContext {
            mountain,
            cheese_region: smoothstep(0.35, 0.70, n01(self.cheese_region.get([fx, fz]))),
            shaft: smoothstep(0.74, 0.92, n01(self.shaft_region.get([fx, fz]))),
            // Canon: cresta estrecha (1 - |n|) de un ruido anisotropo (largo en X).
            canyon: smoothstep(
                0.72,
                0.95,
                1.0 - self.canyon_region.get([fx * 0.0016, fz * 0.010]).abs(),
            ),
            entrance: smoothstep(0.86, 0.98, n01(self.entrance_region.get([fx, fz]))),
            worms,
            worm_count,
        }
    }

    /// Atenuacion por profundidad en `0..=1`: 0 en la corteza, 1 en
    /// `FULL_DEPTH_Y` y de nuevo 0 en la bedrock. Se multiplica por el refuerzo
    /// de montana.
    fn attenuation(y: i32, surface: i32, mountain: f64) -> f64 {
        let crust = surface - CAVE_CRUST;
        if y < BEDROCK_CLEAR || y >= crust {
            return 0.0;
        }
        let upper = ((crust - y) as f64 / (crust - FULL_DEPTH_Y).max(1) as f64).clamp(0.0, 1.0);
        let lower = ((y - BEDROCK_CLEAR) as f64 / (FULL_DEPTH_Y - BEDROCK_CLEAR).max(1) as f64)
            .clamp(0.0, 1.0);
        let base = (upper * lower).clamp(0.0, 1.0);
        (base * (1.0 + mountain * MOUNTAIN_CAVE_BOOST)).clamp(0.0, 1.75)
    }

    /// ¿La celda entra dentro de un sistema que rompe la corteza (entrada)?
    fn is_entrance(ctx: &CaveContext, y: i32, surface: i32) -> bool {
        let crust = surface - CAVE_CRUST;
        ctx.entrance > 0.0 && y >= crust && y < surface && y >= surface - ENTRANCE_DEPTH
    }

    /// Evalua una celda del terreno. `surface` es la altura del terreno;
    /// `aquifer` el nivel del acuifero (por debajo, la cueva nace con agua).
    pub fn carve(
        &self,
        ctx: &CaveContext,
        x: i32,
        y: i32,
        z: i32,
        surface: i32,
        aquifer: i32,
    ) -> Carve {
        if y < BEDROCK_CLEAR {
            return Carve::None;
        }
        let crust = surface - CAVE_CRUST;
        if y >= crust {
            // Solo una entrada rara perfora la corteza; el resto queda intacto.
            if Self::is_entrance(ctx, y, surface) {
                return if y < aquifer {
                    Carve::Water
                } else {
                    Carve::Air
                };
            }
            return Carve::None;
        }
        let at = Self::attenuation(y, surface, ctx.mountain as f64);
        if at <= 0.0 {
            return Carve::None;
        }
        let (fx, fy, fz) = (x as f64, y as f64, z as f64);

        // 0) Red de worms (P1.4): tubos globales de 64 bloques. Solo en v2
        // (`worm_count > 0`) y por debajo de la corteza.
        let mut hollow =
            y > BEDROCK_CLEAR + 2 && y < surface - CHEESE_CRUST_KEEP && worm_hollow(ctx, x, y, z);
        // 1) Tubos delgados (spaghetti), dos campos cruzados. El segundo solo se
        // evalua cerca de la banda cero del primero (que es donde se cruzan), para
        // no pagar un ruido 3D en cada celda del subsuelo.
        if !hollow {
            let ta = self.tubes_a.get([fx, fy, fz]);
            if ta.abs() < TUBE_A_WIDTH * at
                || (ta.abs() < 0.30 && self.tubes_b.get([fx, fy, fz]).abs() < TUBE_B_WIDTH * at)
            {
                hollow = true;
            }
        }
        // 2) Tubos regionales (mas anchos).
        if !hollow && self.regional.get([fx, fy, fz]).abs() < REGION_WIDTH * at {
            hollow = true;
        }
        // 3) Camaras `cheese`: profundas y solo en regiones aptas.
        if !hollow
            && y < surface - CHEESE_CRUST_KEEP
            && ctx.cheese_region > 0.0
            && self.cheese.get([fx, fy, fz]) * (0.5 + 0.5 * ctx.cheese_region) > CHEESE_THRESHOLD
        {
            hollow = true;
        }
        // 4) Pozos verticales.
        if !hollow
            && ctx.shaft > 0.0
            && self.shaft_shape.get([fx * 1.0, fy * 0.12, fz * 1.0]).abs()
                < SHAFT_WIDTH * ctx.shaft * at
        {
            hollow = true;
        }
        // 5) Canones (largos en X), en banda media.
        if !hollow
            && ctx.canyon > 0.0
            && y > FULL_DEPTH_Y + 4
            && y < surface - CHEESE_CRUST_KEEP
            && self
                .canyon_shape
                .get([fx * 0.010, fy * 0.06, fz * 0.0022])
                .abs()
                < CANYON_WIDTH * ctx.canyon
        {
            hollow = true;
        }
        if !hollow {
            return Carve::None;
        }
        // Mascara de preservacion: columnas/puentes solidos dentro de las camaras.
        // Solo se paga cuando algun sistema ha propuesto cavar (raro).
        if self.pillar.get([fx, fy, fz]) > 0.55 {
            return Carve::None;
        }
        if y < aquifer {
            Carve::Water
        } else {
            Carve::Air
        }
    }

    /// ¿Hay cueva (seca o inundada) en la celda? Comodo para tests y previews:
    /// construye el contexto de la columna al vuelo.
    pub fn is_cave(&self, x: i32, y: i32, z: i32, surface: i32, mountain: f32) -> bool {
        let ctx = self.context(x, z, mountain);
        !matches!(self.carve(&ctx, x, y, z, surface, i32::MIN), Carve::None)
    }
}

/// `smoothstep` monótono en `[lo, hi]` -> `0..1`.
fn smoothstep(lo: f64, hi: f64, v: f64) -> f64 {
    if hi <= lo {
        return if v < lo { 0.0 } else { 1.0 };
    }
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_misma_semilla_da_las_mismas_cuevas() {
        let a = CaveSystem::new(5);
        let b = CaveSystem::new(5);
        for y in 10..60 {
            assert_eq!(
                a.is_cave(3, y, 7, 70, 0.0),
                b.is_cave(3, y, 7, 70, 0.0),
                "y={y}"
            );
        }
    }

    #[test]
    fn la_atenuacion_es_cero_en_los_extremos_y_maxima_abajo() {
        let surface = 70;
        assert_eq!(CaveSystem::attenuation(4, surface, 0.0), 0.0, "bedrock");
        assert_eq!(CaveSystem::attenuation(69, surface, 0.0), 0.0, "corteza");
        assert!((CaveSystem::attenuation(FULL_DEPTH_Y, surface, 0.0) - 1.0).abs() < 1e-6);
        let a = CaveSystem::attenuation(60, surface, 0.0);
        let b = CaveSystem::attenuation(30, surface, 0.0);
        let c = CaveSystem::attenuation(15, surface, 0.0);
        assert!(a < b && b < c && c <= 1.0);
    }

    #[test]
    fn la_montana_refuerza_la_densidad_de_cueva() {
        let m = CaveSystem::attenuation(30, 90, 1.0);
        let flat = CaveSystem::attenuation(30, 90, 0.0);
        assert!(m > flat, "la montana deberia reforzar: {m} vs {flat}");
    }

    #[test]
    fn hay_cuevas_pero_no_en_toda_la_columna() {
        let c = CaveSystem::new(13_371);
        let mut carved = 0;
        let total = 60 * 60 * 60;
        for x in 0..60 {
            for z in 0..60 {
                let ctx = c.context(x, z, 0.0);
                for y in 6..66 {
                    if !matches!(c.carve(&ctx, x, y, z, 70, i32::MIN), Carve::None) {
                        carved += 1;
                    }
                }
            }
        }
        let frac = carved as f64 / total as f64;
        assert!(frac > 0.005, "apenas hay cuevas: {frac}");
        assert!(frac < 0.30, "demasiadas cuevas: {frac}");
    }

    #[test]
    fn no_se_cava_la_bedrock_y_la_corteza_solo_en_entradas() {
        let c = CaveSystem::new(99);
        for x in 0..40 {
            for z in 0..40 {
                let ctx = c.context(x, z, 0.0);
                for y in 0..BEDROCK_CLEAR {
                    assert_eq!(c.carve(&ctx, x, y, z, 70, 50), Carve::None, "bedrock y={y}");
                }
                for y in (70 - CAVE_CRUST)..70 {
                    let carved = c.carve(&ctx, x, y, z, 70, 50) != Carve::None;
                    if carved {
                        assert!(
                            CaveSystem::is_entrance(&ctx, y, 70),
                            "corteza cavada sin ser entrada: y={y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn las_cuevas_bajo_el_acuifero_se_llenan_de_agua() {
        let c = CaveSystem::new(7);
        let (mut agua, mut aire) = (0u32, 0u32);
        for x in 0..80 {
            for z in 0..80 {
                let ctx = c.context(x, z, 0.0);
                for y in 6..66 {
                    match c.carve(&ctx, x, y, z, 70, 40) {
                        Carve::Water => {
                            assert!(y < 40, "agua por encima del acuifero: y={y}");
                            agua += 1;
                        }
                        Carve::Air => {
                            assert!(y >= 40, "aire bajo el acuifero: y={y}");
                            aire += 1;
                        }
                        Carve::None => {}
                    }
                }
            }
        }
        assert!(agua > 0, "no se genero agua de acuifero");
        assert!(aire > 0, "no se genero aire de cueva");
    }

    #[test]
    fn hay_pozos_verticales_que_cruzan_varias_capas() {
        let c = CaveSystem::new(2_024);
        let mut run_max = 0;
        for x in 0..120 {
            for z in 0..120 {
                let ctx = c.context(x, z, 0.0);
                let mut run = 0;
                for y in 6..64 {
                    if matches!(c.carve(&ctx, x, y, z, 70, i32::MIN), Carve::Air) {
                        run += 1;
                        run_max = run_max.max(run);
                    } else {
                        run = 0;
                    }
                }
            }
        }
        assert!(run_max >= 8, "no hay pozos verticales largos: {run_max}");
    }

    #[test]
    fn worm_field_es_determinista() {
        let a = WormField::new(7);
        let b = WormField::new(7);
        for (x, z) in [(0, 0), (63, 63), (-100, 250), (4000, -2500)] {
            let ca = a.context(x, z);
            let cb = b.context(x, z);
            assert_eq!(ca.1, cb.1, "distinto numero de worms en ({x},{z})");
            assert_eq!(ca.0[..ca.1 as usize], cb.0[..cb.1 as usize]);
        }
    }

    #[test]
    fn worm_field_teje_conectores_entre_celdas() {
        let f = WormField::new(13_371);
        let mut con_conector = 0;
        for cz in 0..8 {
            for cx in 0..8 {
                let (_, n) = f.context(cx * WORM_CELL + 32, cz * WORM_CELL + 32);
                if n as usize > 3 * 3 * WORMS_PER_CELL {
                    con_conector += 1;
                }
            }
        }
        assert!(con_conector > 0, "no se genero ningun conector");
    }

    #[test]
    fn v2_no_cava_la_corteza_con_worms() {
        let c = CaveSystem::new_with_worms(99);
        for x in 0..32 {
            for z in 0..32 {
                let ctx = c.context(x, z, 0.0);
                for y in (70 - CAVE_CRUST)..70 {
                    if c.carve(&ctx, x, y, z, 70, 50) != Carve::None {
                        assert!(
                            CaveSystem::is_entrance(&ctx, y, 70),
                            "corteza cavada sin entrada: y={y}"
                        );
                    }
                }
            }
        }
    }
}
