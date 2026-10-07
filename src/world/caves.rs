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
}

impl CaveSystem {
    pub fn new(seed: u32) -> Self {
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
        }
    }

    /// Calcula el contexto 2D de la columna `(x, z)`. `mountain` es la mascara
    /// de montana del worldgen (`0..1`).
    pub fn context(&self, x: i32, z: i32, mountain: f32) -> CaveContext {
        let (fx, fz) = (x as f64, z as f64);
        let n01 = |n: f64| (n * 0.5 + 0.5).clamp(0.0, 1.0);
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

        let mut hollow = false;
        // 1) Tubos delgados (spaghetti), dos campos cruzados. El segundo solo se
        // evalua cerca de la banda cero del primero (que es donde se cruzan), para
        // no pagar un ruido 3D en cada celda del subsuelo.
        let ta = self.tubes_a.get([fx, fy, fz]);
        if ta.abs() < TUBE_A_WIDTH * at
            || (ta.abs() < 0.30 && self.tubes_b.get([fx, fy, fz]).abs() < TUBE_B_WIDTH * at)
        {
            hollow = true;
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
}
