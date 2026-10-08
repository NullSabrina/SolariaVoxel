//! **Arboles procedimentales** (MEGA PROMPT 1, Fase D).
//!
//! Una unica decision pura por coordenada **global**: [`TreePlacer::plan`] mira
//! el bioma, la altura del suelo y la pendiente de `(wx, wz)` y devuelve un
//! [`TreePlan`] (especie, tronco, semilla) o `None`. La copa **no** es una
//! plantilla: es un elipsoide (o cono, para la picea) con el radio perturbado por
//! ruido 3D de baja frecuencia y recorte por distancia al tronco, asi que cada
//! arbol es distinto y ninguno depende del chunk que lo dibuje.
//!
//! Colocacion sin cortes (camino A del prompt): cada chunk consulta las
//! coordenadas de un margen [`MARGIN`] a su alrededor y **dibuja solo la parte
//! de la copa que cae en su volumen**. Un arbol que cruza una frontera se genera
//! entero en ambos chunks, sin cola de pendientes ni estado compartido.
//!
//! Todo hash usa `(wx, wz)` **globales** (nunca locales al chunk): el mismo arbol
//! es identico sin importar el orden de generacion.

use noise::{NoiseFn, Perlin};

use crate::world::block::Block;
use crate::world::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};
use crate::world::terrain::{Biome, SEA_LEVEL};

/// Radio horizontal maximo de una copa (y margen de consulta entre chunks).
/// Todas las formas respetan este limite; un test lo comprueba.
pub const MARGIN: i32 = 4;

/// Altura maxima por encima del suelo a la que llega una copa (cota superior).
const MAX_CANOPY_UP: i32 = 10;

/// Escala del ruido 3D que perturba el radio de la copa.
const CANOPY_SCALE: f64 = 0.09;
/// Escala del ruido regional que elige la especie dentro de un bioma.
const SPECIES_SCALE: f64 = 0.006;
/// Escala del ruido de agrupacion (bosques con claros).
const CLUSTER_SCALE: f64 = 0.018;
/// Umbral minimo de agrupacion de cualquier bioma (filtro barato previo).
const MIN_CLUSTER: f32 = -0.20;
/// Densidad maxima entre todos los biomas (cota del filtro barato previo).
const MAX_DENSITY: f32 = 0.07;

/// Especie de arbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Species {
    /// Roble/haya: copa ovoide irregular, tronco corto y grueso.
    Oak,
    /// Abeto/picea: tronco recto y copa conica en capas.
    Spruce,
    /// Abedul: tronco claro, copa alta y estrecha.
    Birch,
    /// Acacia: tronco inclinado y copa plana (sabana).
    Acacia,
}

impl Species {
    /// Nombre legible (diagnostico).
    pub fn name(self) -> &'static str {
        match self {
            Species::Oak => "oak",
            Species::Spruce => "spruce",
            Species::Birch => "birch",
            Species::Acacia => "acacia",
        }
    }

    /// Altura del tronco (bloques), determinista por la semilla del arbol.
    fn trunk_height(self, seed: u32) -> i32 {
        let r = (seed >> 3) % 100;
        match self {
            Species::Oak => 4 + (r % 3) as i32,
            Species::Spruce => 6 + (r % 4) as i32,
            Species::Birch => 5 + (r % 3) as i32,
            Species::Acacia => 4 + (r % 2) as i32,
        }
    }
}

/// Plan de un arbol: todo lo necesario para que cualquier chunk dibuje su parte.
#[derive(Clone, Copy, Debug)]
pub struct TreePlan {
    pub species: Species,
    /// Coordenada global del tronco.
    pub wx: i32,
    pub wz: i32,
    /// Primera celda de aire sobre el suelo (base del tronco).
    pub ground: i32,
    /// Altura del tronco en bloques.
    pub trunk: i32,
    /// Semilla del arbol (forma y tronco).
    pub seed: u32,
}

/// Decisor y dibujante de arboles, determinista por semilla.
pub struct TreePlacer {
    cluster: Perlin,
    species: Perlin,
    canopy: Perlin,
}

impl TreePlacer {
    pub fn new(seed: u32) -> Self {
        let s = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        Self {
            cluster: Perlin::new(s(61)),
            species: Perlin::new(s(71)),
            canopy: Perlin::new(s(73)),
        }
    }

    /// Filtro **barato** (agrupacion + densidad global): `false` garantiza que
    /// `plan` tambien devolveria `None`. Sirve para no muestrear el terreno en el
    /// anillo de margen salvo donde de verdad puede haber arbol.
    pub fn maybe(&self, wx: i32, wz: i32) -> bool {
        if self.cluster_value(wx, wz) < MIN_CLUSTER {
            return false;
        }
        hash01(wx, wz) < MAX_DENSITY
    }

    fn cluster_value(&self, wx: i32, wz: i32) -> f32 {
        self.cluster
            .get([wx as f64 * CLUSTER_SCALE, wz as f64 * CLUSTER_SCALE]) as f32
    }

    /// Especie para un bioma, con variacion regional (abedul dentro de bosque y
    /// llanura). `None` en biomas sin arboles.
    fn species_for(&self, biome: Biome, wx: i32, wz: i32) -> Option<Species> {
        let n = self
            .species
            .get([wx as f64 * SPECIES_SCALE, wz as f64 * SPECIES_SCALE]);
        Some(match biome {
            Biome::Forest => {
                if n > 0.15 {
                    Species::Birch
                } else {
                    Species::Oak
                }
            }
            Biome::Taiga => Species::Spruce,
            Biome::Swamp => Species::Oak,
            Biome::Plains => {
                if n > 0.30 {
                    Species::Birch
                } else {
                    Species::Oak
                }
            }
            Biome::Savanna => Species::Acacia,
            Biome::Desert | Biome::Tundra => return None,
        })
    }

    /// Umbral de agrupacion del bioma (claros): por debajo, no hay arbol.
    fn cluster_threshold(biome: Biome) -> f32 {
        match biome {
            Biome::Forest => -0.05,
            Biome::Taiga => -0.10,
            Biome::Swamp => -0.20,
            Biome::Plains => 0.15,
            Biome::Savanna => 0.10,
            Biome::Desert | Biome::Tundra => 1.0,
        }
    }

    /// Decide el arbol de `(wx, wz)`. `ground` es la primera celda de aire (base
    /// del tronco); `slope`, la pendiente de la superficie (diferencia maxima con
    /// las 4 vecinas). Puro: solo depende de sus argumentos.
    pub fn plan(
        &self,
        wx: i32,
        wz: i32,
        biome: Biome,
        ground: i32,
        slope: i32,
    ) -> Option<TreePlan> {
        if slope > 1 {
            return None; // no en laderas fuertes
        }
        if ground <= SEA_LEVEL || ground > 175 {
            return None; // ni en agua ni en cumbres
        }
        // Rechazo barato (agrupacion + densidad global) antes de ruidos de especie.
        let cluster = self.cluster_value(wx, wz);
        if cluster < MIN_CLUSTER || hash01(wx, wz) >= MAX_DENSITY {
            return None;
        }
        let species = self.species_for(biome, wx, wz)?;
        if cluster < Self::cluster_threshold(biome) {
            return None; // claro
        }
        let density = biome.tree_density();
        if density <= 0.0 || hash01(wx, wz) >= density {
            return None;
        }
        let seed = hash_u32(wx, wz);
        let trunk = species.trunk_height(seed);
        Some(TreePlan {
            species,
            wx,
            wz,
            ground,
            trunk,
            seed,
        })
    }

    /// Dibuja en `column` (chunk en `world_x, world_z`) la parte del arbol que
    /// cae dentro. Solo escribe sobre **aire** (nunca pisa terreno, agua ni otro
    /// arbol).
    pub fn draw(&self, column: &mut Column, plan: &TreePlan, world_x: i32, world_z: i32) {
        let ox = plan.wx - world_x;
        let oz = plan.wz - world_z;
        for dy in 0..=(plan.trunk + MAX_CANOPY_UP) {
            for dz in -MARGIN..=MARGIN {
                for dx in -MARGIN..=MARGIN {
                    let Some(block) = self.block_at(plan, dx, dy, dz) else {
                        continue;
                    };
                    let wx = ox + dx;
                    let wz = oz + dz;
                    let wy = plan.ground + dy;
                    if wx < 0
                        || wz < 0
                        || wx >= CHUNK_SIZE as i32
                        || wz >= CHUNK_SIZE as i32
                        || wy < 0
                        || wy >= WORLD_HEIGHT as i32
                    {
                        continue;
                    }
                    let (wx, wy, wz) = (wx as usize, wy as usize, wz as usize);
                    let existing = column.get(wx, wy, wz);
                    // El tronco tiene **prioridad** sobre las hojas (asi un tronco
                    // inclinado no queda truncado por la copa de un vecino); las
                    // hojas solo se escriben sobre aire.
                    let place = match block {
                        Block::Wood => existing == Block::Air || existing == Block::Leaves,
                        _ => existing == Block::Air,
                    };
                    if place {
                        column.set(wx, wy, wz, block);
                    }
                }
            }
        }
    }

    /// Bloque del arbol en el desplazamiento `(dx, dy, dz)` respecto al tronco
    /// (`dy` relativo al suelo; `0` = primera celda del tronco). `None` = aire.
    fn block_at(&self, p: &TreePlan, dx: i32, dy: i32, dz: i32) -> Option<Block> {
        if self.is_trunk(p, dx, dy, dz) {
            return Some(Block::Wood);
        }
        if self.in_canopy(p, dx, dy, dz) {
            Some(Block::Leaves)
        } else {
            None
        }
    }

    /// ¿La celda es tronco? Recto en todas las especies (el tronco inclinado de
    /// la acacia se aplazo: ver `DECISIONS.md`).
    fn is_trunk(&self, p: &TreePlan, dx: i32, dy: i32, dz: i32) -> bool {
        dy >= 0 && dy < p.trunk && dx == 0 && dz == 0
    }

    /// ¿La celda cae dentro de la copa?
    fn in_canopy(&self, p: &TreePlan, dx: i32, dy: i32, dz: i32) -> bool {
        let n = self.canopy.get([
            (p.wx + dx) as f64 * CANOPY_SCALE,
            (p.ground + dy) as f64 * CANOPY_SCALE,
            (p.wz + dz) as f64 * CANOPY_SCALE,
        ]) as f32;
        match p.species {
            Species::Spruce => spruce_canopy(p, dx, dy, dz, n),
            _ => ellipsoid_canopy(p, dx, dy, dz, n),
        }
    }
}

/// Copa ovoide (roble, abedul) o plana (acacia): elipsoide con el radio
/// perturbado por `n` y recorte por distancia al centro.
fn ellipsoid_canopy(p: &TreePlan, dx: i32, dy: i32, dz: i32, n: f32) -> bool {
    // Centro y radios por especie (todos <= MARGIN).
    let (cy, rx, ry, rz) = match p.species {
        Species::Oak => (p.trunk - 1, 2.6f32, 2.0f32, 2.6f32),
        Species::Birch => (p.trunk - 1, 1.7, 2.6, 1.7),
        Species::Acacia => (p.trunk, 3.0, 1.0, 3.0), // copa plana (paraguas)
        Species::Spruce => unreachable!("la picea usa el cono"),
    };
    let grow = 1.0 + 0.28 * n;
    let fx = dx as f32 / rx;
    let fy = (dy - cy) as f32 / ry;
    let fz = dz as f32 / rz;
    fx * fx + fy * fy + fz * fz <= grow
}

/// Copa conica en capas de la picea: mas ancha abajo, estrecha arriba.
fn spruce_canopy(p: &TreePlan, dx: i32, dy: i32, dz: i32, n: f32) -> bool {
    if dy < 1 || dy > p.trunk + 1 {
        return false;
    }
    let t = (dy - 1) as f32 / p.trunk.max(1) as f32; // 0 abajo, 1 arriba
    let r = (3.0 * (1.0 - t) + 0.5) + 0.5 * n;
    if r <= 0.0 {
        return false;
    }
    (dx * dx + dz * dz) as f32 <= r * r
}

/// Hash determinista de `(x, z)` en `[0, 1)`.
fn hash01(x: i32, z: i32) -> f32 {
    (hash_u32(x, z) % 100_000) as f32 / 100_000.0
}

/// Hash entero determinista de `(x, z)`.
fn hash_u32(x: i32, z: i32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((z as u32).wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_decidor_es_determinista_y_global() {
        let a = TreePlacer::new(7);
        let b = TreePlacer::new(7);
        for (x, z) in [(0, 0), (321, -98), (-4000, 2500)] {
            assert_eq!(
                a.plan(x, z, Biome::Forest, 70, 0).map(|p| (p.species, p.trunk)),
                b.plan(x, z, Biome::Forest, 70, 0).map(|p| (p.species, p.trunk))
            );
        }
    }

    #[test]
    fn no_hay_arboles_en_laderas_ni_en_biomas_secos() {
        let p = TreePlacer::new(99);
        for x in 0..40 {
            for z in 0..40 {
                assert!(p.plan(x, z, Biome::Forest, 70, 3).is_none(), "ladera");
                assert!(p.plan(x, z, Biome::Desert, 70, 0).is_none(), "desierto");
                assert!(p.plan(x, z, Biome::Tundra, 70, 0).is_none(), "tundra");
            }
        }
    }

    #[test]
    fn aparecen_las_cuatro_especies() {
        let p = TreePlacer::new(13_371);
        let mut seen = [false; 4];
        let mut mark = |s: Species| match s {
            Species::Oak => seen[0] = true,
            Species::Spruce => seen[1] = true,
            Species::Birch => seen[2] = true,
            Species::Acacia => seen[3] = true,
        };
        for i in 0..4000 {
            let x = i * 37 - 40_000;
            let z = i * 53 - 20_000;
            for biome in [Biome::Forest, Biome::Taiga, Biome::Savanna] {
                if let Some(plan) = p.plan(x, z, biome, 70, 0) {
                    mark(plan.species);
                }
            }
        }
        assert!(seen.iter().all(|&v| v), "faltan especies: {seen:?}");
    }

    #[test]
    fn ningun_arbol_sale_del_margen() {
        // La copa y el tronco caben en `MARGIN` (nada aparece mas alla).
        let p = TreePlacer::new(5);
        let mut checked = 0u32;
        for i in 0..3000 {
            let x = i * 41 - 30_000;
            let z = i * 67 - 15_000;
            for biome in [Biome::Forest, Biome::Taiga, Biome::Plains, Biome::Savanna] {
                let Some(plan) = p.plan(x, z, biome, 70, 0) else {
                    continue;
                };
                for dy in 0..=(plan.trunk + MAX_CANOPY_UP) {
                    for dz in -8..=8 {
                        for dx in -8..=8 {
                            if p.block_at(&plan, dx, dy, dz).is_some() {
                                assert!(
                                    dx.abs() <= MARGIN && dz.abs() <= MARGIN,
                                    "bloque fuera del margen en ({dx},{dy},{dz})"
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 0, "no se comprobo ningun bloque");
    }

    #[test]
    fn un_arbol_que_cruza_la_frontera_se_genera_entero_en_ambos_chunks() {
        // Fase D (camino A): cada chunk dibuja su parte; el arbol queda entero.
        let p = TreePlacer::new(13_371);
        for i in 0..40_000 {
            let wx = 15 + (i % 4);
            let wz = 5 + ((i / 4) % 6);
            for biome in [Biome::Forest, Biome::Taiga, Biome::Plains, Biome::Savanna] {
                let Some(plan) = p.plan(wx, wz, biome, 70, 0) else {
                    continue;
                };
                let mut a = Column::empty();
                let mut b = Column::empty();
                p.draw(&mut a, &plan, 0, 0);
                p.draw(&mut b, &plan, 16, 0);
                let (mut in_a, mut in_b, mut total) = (0u32, 0u32, 0u32);
                for dy in 0..=(plan.trunk + MAX_CANOPY_UP) {
                    for dz in -MARGIN..=MARGIN {
                        for dx in -MARGIN..=MARGIN {
                            let Some(block) = p.block_at(&plan, dx, dy, dz) else {
                                continue;
                            };
                            let wx = plan.wx + dx;
                            let wz = plan.wz + dz;
                            let wy = plan.ground + dy;
                            // El arbol cabe en z dentro del chunk (wz 5..11).
                            assert!((0..CHUNK_SIZE as i32).contains(&wz));
                            let got = if wx < 16 {
                                in_a += 1;
                                a.get(wx as usize, wy as usize, wz as usize)
                            } else {
                                in_b += 1;
                                b.get((wx - 16) as usize, wy as usize, wz as usize)
                            };
                            assert_eq!(got, block, "bloque perdido en ({wx},{wy},{wz})");
                            total += 1;
                        }
                    }
                }
                assert!(total > 0);
                if in_a > 0 && in_b > 0 {
                    return; // encontrado un arbol que cruza la frontera
                }
            }
        }
        panic!("no se encontro un arbol que cruce la frontera");
    }

    #[test]
    fn las_hojas_no_pisan_solidos_ni_agua() {
        // Fase E (3.4): el dibujo solo escribe sobre aire; nunca sustituye agua
        // ni solidos.
        let p = TreePlacer::new(13_371);
        let mut found = None;
        'find: for i in 0..100_000 {
            for biome in [Biome::Forest, Biome::Taiga, Biome::Plains, Biome::Savanna] {
                if let Some(pl) = p.plan(i * 7 - 30_000, i * 3 - 10_000, biome, 70, 0) {
                    found = Some(pl);
                    break 'find;
                }
            }
        }
        let plan = found.expect("deberia haber algun arbol");
        // Columna llena de agua en toda la zona del arbol.
        let mut column = Column::empty();
        for y in 60..100 {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    column.set(x, y, z, Block::Water);
                }
            }
        }
        p.draw(&mut column, &plan, plan.wx - 8, plan.wz - 8);
        for y in 60..100 {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    assert_eq!(
                        column.get(x, y, z),
                        Block::Water,
                        "el arbol piso una celda no-aire en ({x},{y},{z})"
                    );
                }
            }
        }
    }
}
