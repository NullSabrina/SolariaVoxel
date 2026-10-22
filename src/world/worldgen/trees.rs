//! **Arboles procedimentales**.
//!
//! Hay dos caminos, aislados a proposito:
//!
//! * **Legacy (v1)**: el decisor historico por `hash01` + copa elipsoidal. Se
//!   conserva byte a byte para no cambiar los mundos existentes.
//! * **Organico (v2, P0.3)**: colocacion Poisson determinista + esqueleto de
//!   ramas por *Space Colonization*, voxelizado en troncos de 1-2 bloques.
//!
//! La colocacion organica es **global**: una celda de `PCELL` bloques genera
//! `K` candidatos con `SplitMix64` sembrado por `(semilla, celda)`, y un
//! candidato se acepta solo si ningun candidato vecino con prioridad menor cae
//! dentro de su radio. Esto es Poisson por *dart throwing* con prioridad global:
//! no depende del orden de generacion, del chunk ni de un `Vec` de activos, asi
//! que dos workers obtienen el mismo bosque. El radio crece con la densidad:
//! bosque denso -> ~2.5 bloques, llanura dispersa -> ~7.
//!
//! Todo hash/PRNG deriva de `(wx, wz, seed)`; no se usa `rand`.

use noise::{NoiseFn, Perlin};

use crate::world::block::Block;
use crate::world::chunk::{CHUNK_SIZE, Column, WORLD_HEIGHT};
use crate::world::terrain::{Biome, SEA_LEVEL};

/// Radio horizontal maximo de una copa y margen de consulta entre chunks.
pub const MARGIN: i32 = 4;

/// Margen del camino organico (copas SCA + hojas).
pub const ORGANIC_MARGIN: i32 = 5;

/// Altura maxima por encima del suelo a la que llega una copa (cota superior).
const MAX_CANOPY_UP: i32 = 20;

/// Escala del ruido 3D que perturba la copa legacy.
const CANOPY_SCALE: f64 = 0.09;
/// Escala del ruido regional que elige la especie dentro de un bioma.
const SPECIES_SCALE: f64 = 0.006;
/// Escala del ruido de agrupacion legacy (bosques con claros).
const CLUSTER_SCALE: f64 = 0.018;
/// Umbral minimo de agrupacion legacy de cualquier bioma.
const MIN_CLUSTER: f32 = -0.20;
/// Densidad maxima legacy entre todos los biomas.
const MAX_DENSITY: f32 = 0.07;

/// Lado de la celda Poisson (bloques).
pub const PCELL: i32 = 4;
/// Candidatos por celda Poisson.
pub const K: u32 = 3;
/// Radio minimo (bosque denso), en bloques.
pub const R_MIN: f32 = 2.5;
/// Radio maximo (llanura dispersa), en bloques.
pub const R_MAX: f32 = 7.0;

/// Capacidad del esqueleto voxelizado.
const MAX_BRANCHES: usize = 64;
const MAX_LEAVES: usize = 160;
const MAX_NODES: usize = 44;
const MAX_ATTRACTORS: usize = 44;

/// Especie de arbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Species {
    /// Roble/haya: copa ovoide irregular, tronco corto y grueso.
    Oak,
    /// Abeto/picea: tronco recto y copa conica en capas.
    Spruce,
    /// Abedul: tronco claro, copa alta y estrecha.
    Birch,
    /// Acacia: copa plana (sabana).
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

    /// Parametros de forma (data-driven en una tabla const).
    fn profile(self) -> SpeciesProfile {
        match self {
            Species::Oak => SpeciesProfile {
                crown_radius: 3.2,
                crown_height: 4.2,
                branch_length: 1.55,
                branch_angle: 38.0,
                trunk_radius: 2,
                attractors: 28,
            },
            Species::Spruce => SpeciesProfile {
                crown_radius: 2.5,
                crown_height: 6.2,
                branch_length: 1.40,
                branch_angle: 58.0,
                trunk_radius: 1,
                attractors: 32,
            },
            Species::Birch => SpeciesProfile {
                crown_radius: 2.1,
                crown_height: 5.0,
                branch_length: 1.35,
                branch_angle: 32.0,
                trunk_radius: 1,
                attractors: 24,
            },
            Species::Acacia => SpeciesProfile {
                crown_radius: 3.7,
                crown_height: 2.4,
                branch_length: 1.75,
                branch_angle: 66.0,
                trunk_radius: 1,
                attractors: 26,
            },
        }
    }
}

/// Parametros de forma de una especie (P0.3).
#[derive(Clone, Copy, Debug)]
pub struct SpeciesProfile {
    pub crown_radius: f32,
    pub crown_height: f32,
    pub branch_length: f32,
    /// Angulo de apertura de ramas (grados); modula el radio de influencia.
    pub branch_angle: f32,
    /// 1 = tronco de 1 bloque, 2 = tronco de 2 bloques.
    pub trunk_radius: u8,
    pub attractors: usize,
}

/// Tramo recto del esqueleto, relativo a la base del tronco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Branch {
    pub a: [i16; 3],
    pub b: [i16; 3],
    pub thickness: u8,
}

impl Branch {
    const EMPTY: Self = Self {
        a: [0, 0, 0],
        b: [0, 0, 0],
        thickness: 1,
    };
}

/// Esqueleto + hojas ya voxelizados, `Copy` para no asignar en el camino caliente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeShape {
    branches: [Branch; MAX_BRANCHES],
    branch_count: u16,
    leaves: [[i16; 3]; MAX_LEAVES],
    leaf_count: u16,
}

impl TreeShape {
    fn empty() -> Self {
        Self {
            branches: [Branch::EMPTY; MAX_BRANCHES],
            branch_count: 0,
            leaves: [[0, 0, 0]; MAX_LEAVES],
            leaf_count: 0,
        }
    }

    /// Tramos del esqueleto.
    pub fn branches(&self) -> &[Branch] {
        &self.branches[..self.branch_count as usize]
    }

    /// Voxeles de hojas (posiciones exactas).
    pub fn leaves(&self) -> &[[i16; 3]] {
        &self.leaves[..self.leaf_count as usize]
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
    /// Densidad de la columna que lo acepto (`0` en el camino legacy).
    pub density: f32,
    /// Esqueleto organico. `None` = camino legacy.
    pub shape: Option<TreeShape>,
}

/// Decisor y dibujante de arboles, determinista por semilla.
pub struct TreePlacer {
    cluster: Perlin,
    species: Perlin,
    canopy: Perlin,
    seed: u32,
}

impl TreePlacer {
    pub fn new(seed: u32) -> Self {
        let s = |k: u32| seed.wrapping_mul(0x9E37_79B9).wrapping_add(k);
        Self {
            cluster: Perlin::new(s(61)),
            species: Perlin::new(s(71)),
            canopy: Perlin::new(s(73)),
            seed,
        }
    }

    // -----------------------------------------------------------------------
    // Camino legacy (v1): no tocar. Conserva los mundos existentes.
    // -----------------------------------------------------------------------

    /// Filtro barato legacy (agrupacion + densidad global).
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

    /// Especie para un bioma, con variacion regional.
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

    /// Decide el arbol legacy de `(wx, wz)`.
    pub fn plan(
        &self,
        wx: i32,
        wz: i32,
        biome: Biome,
        ground: i32,
        slope: i32,
    ) -> Option<TreePlan> {
        if slope > 1 {
            return None;
        }
        if ground <= SEA_LEVEL || ground > 175 {
            return None;
        }
        let cluster = self.cluster_value(wx, wz);
        if cluster < MIN_CLUSTER || hash01(wx, wz) >= MAX_DENSITY {
            return None;
        }
        let species = self.species_for(biome, wx, wz)?;
        if cluster < Self::cluster_threshold(biome) {
            return None;
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
            density: 0.0,
            shape: None,
        })
    }

    // -----------------------------------------------------------------------
    // Camino organico (v2): Poisson + Space Colonization.
    // -----------------------------------------------------------------------

    /// Filtro barato del camino organico. Usa el radio **minimo** (el mas
    /// permisivo): si devuelve `false`, `plan_organic` tambien devolvera `None`
    /// para cualquier densidad.
    pub fn maybe_organic(&self, wx: i32, wz: i32) -> bool {
        self.poisson_accepted(wx, wz, 1.0)
    }

    /// Decide el arbol organico de `(wx, wz)`. `density` viene de
    /// [`crate::world::worldgen::larion::LarionSample::tree_density`] y ya debe
    /// incluir la atenuacion por pendiente.
    #[allow(clippy::too_many_arguments)]
    pub fn plan_organic(
        &self,
        wx: i32,
        wz: i32,
        biome: Biome,
        ground: i32,
        slope: i32,
        density: f32,
    ) -> Option<TreePlan> {
        if slope > 1 || ground <= SEA_LEVEL || ground > 175 {
            return None;
        }
        if !density.is_finite() || density <= 0.01 {
            return None;
        }
        if !self.poisson_accepted(wx, wz, density) {
            return None;
        }
        let species = self.species_for(biome, wx, wz)?;
        let seed = hash_u32(wx, wz) ^ self.seed.rotate_left(11);
        let trunk = species.trunk_height(seed);
        let shape = self.build_shape(wx, wz, species, seed);
        Some(TreePlan {
            species,
            wx,
            wz,
            ground,
            trunk,
            seed,
            density,
            shape: Some(shape),
        })
    }

    /// Radio de aceptacion para una densidad `[0, 1]`.
    pub fn radius_for_density(density: f32) -> f32 {
        let t = density.clamp(0.0, 1.0);
        R_MAX + (R_MIN - R_MAX) * t
    }

    /// ¿El punto es un candidato Poisson aceptado? Dart throwing con prioridad
    /// global: un candidato pierde si un vecino con prioridad menor esta dentro
    /// de `r`. El resultado no depende del orden de consulta.
    fn poisson_accepted(&self, wx: i32, wz: i32, density: f32) -> bool {
        let cx = wx.div_euclid(PCELL);
        let cz = wz.div_euclid(PCELL);
        let me = self.cell_candidate(cx, cz);
        if me.wx != wx || me.wz != wz {
            return false;
        }
        let r = Self::radius_for_density(density);
        let reach = ((r / PCELL as f32).ceil() as i32).max(1) + 1;
        let r2 = r * r;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let other = self.cell_candidate(cx + dx, cz + dz);
                let ddx = (other.wx - wx) as f32;
                let ddz = (other.wz - wz) as f32;
                if ddx * ddx + ddz * ddz >= r2 {
                    continue;
                }
                if other.priority < me.priority
                    || (other.priority == me.priority && (other.wx, other.wz) < (wx, wz))
                {
                    return false;
                }
            }
        }
        true
    }

    /// Mejor candidato (menor prioridad) de una celda Poisson.
    fn cell_candidate(&self, cx: i32, cz: i32) -> Candidate {
        let mut rng = SplitMix64::new(cell_seed(self.seed, cx, cz));
        let mut best: Option<Candidate> = None;
        for _ in 0..K {
            let jx = (rng.next_f32() * PCELL as f32) as i32;
            let jz = (rng.next_f32() * PCELL as f32) as i32;
            let c = Candidate {
                wx: cx * PCELL + jx.clamp(0, PCELL - 1),
                wz: cz * PCELL + jz.clamp(0, PCELL - 1),
                priority: rng.next_u64(),
            };
            if best.is_none_or(|b| c.priority < b.priority) {
                best = Some(c);
            }
        }
        best.expect("K >= 1")
    }

    /// Construye el esqueleto por *Space Colonization*: atractores en la copa,
    /// crecimiento iterativo de ramas hacia ellos, voxelizacion en tramos de
    /// 1-2 bloques y hojas en las puntas.
    fn build_shape(&self, wx: i32, wz: i32, species: Species, seed: u32) -> TreeShape {
        let prof = species.profile();
        let trunk = species.trunk_height(seed);
        let seed64 = (seed as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((wx as i64 as u64) << 32)
            .wrapping_add(wz as i64 as u64);
        let mut rng = SplitMix64::new(seed64);

        // Atractores dentro de un elipsoide/copa.
        let crown_base = (trunk - 1) as f32;
        let center_y = crown_base + prof.crown_height * 0.48;
        let mut attractors = [[0.0f32; 3]; MAX_ATTRACTORS];
        let want = prof.attractors.min(MAX_ATTRACTORS);
        let mut na = 0usize;
        for _ in 0..(want * 12) {
            let u = rng.next_f32() * 2.0 - 1.0;
            let v = rng.next_f32() * 2.0 - 1.0;
            let w = rng.next_f32() * 2.0 - 1.0;
            if u * u + v * v + w * w > 1.0 {
                continue;
            }
            let x = u * prof.crown_radius;
            let y = center_y + v * prof.crown_height * 0.5;
            let z = w * prof.crown_radius;
            if y < crown_base + 0.5 {
                continue;
            }
            attractors[na] = [x, y, z];
            na += 1;
            if na == want {
                break;
            }
        }

        let mut shape = TreeShape::empty();
        let mut nodes = [Node::ZERO; MAX_NODES];
        let mut nn = 0usize;
        nodes[nn] = Node::ZERO;
        nn += 1;

        // Tronco garantizado: nodos verticales de 1 bloque.
        let trunk_top = (trunk - 2).max(1);
        let mut last = 0u16;
        for y in 1..=trunk_top {
            let node = Node {
                p: [0.0, y as f32, 0.0],
                parent: last,
                depth: y as u8,
            };
            nodes[nn] = node;
            shape.push_branch(Branch {
                a: nodes[last as usize].p_i16(),
                b: node.p_i16(),
                thickness: prof.trunk_radius.clamp(1, 2),
            });
            last = nn as u16;
            nn += 1;
            if nn >= MAX_NODES {
                break;
            }
        }

        // Space colonization.
        let influence =
            (prof.branch_length * 2.4).max(prof.crown_radius * 1.15) + prof.branch_angle * 0.01;
        let mut reached = [false; MAX_ATTRACTORS];
        for _ in 0..26 {
            let mut sums = [[0.0f32; 3]; MAX_NODES];
            let mut counts = [0u16; MAX_NODES];
            for (ai, a) in attractors[..na].iter().enumerate() {
                if reached[ai] {
                    continue;
                }
                let mut best = usize::MAX;
                let mut best_d2 = influence * influence;
                for (ni, node) in nodes[..nn].iter().enumerate() {
                    let dx = a[0] - node.p[0];
                    let dy = a[1] - node.p[1];
                    let dz = a[2] - node.p[2];
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best = ni;
                    }
                }
                if best == usize::MAX {
                    continue;
                }
                let dx = a[0] - nodes[best].p[0];
                let dy = a[1] - nodes[best].p[1];
                let dz = a[2] - nodes[best].p[2];
                let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                sums[best][0] += dx / len;
                sums[best][1] += dy / len;
                sums[best][2] += dz / len;
                counts[best] += 1;
            }

            let snapshot = nn;
            let mut grew = false;
            for ni in 0..snapshot {
                if counts[ni] == 0 || nn >= MAX_NODES {
                    continue;
                }
                let s = sums[ni];
                let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-6);
                let step = prof.branch_length;
                let mut p = [
                    nodes[ni].p[0] + s[0] / len * step,
                    nodes[ni].p[1] + s[1] / len * step,
                    nodes[ni].p[2] + s[2] / len * step,
                ];
                // Nada puede salirse de `MARGIN` (los bloques del tronco son
                // relativos y las hojas anaden 1 bloque mas).
                let lim = (ORGANIC_MARGIN - 1) as f32;
                p[0] = p[0].clamp(-lim, lim);
                p[2] = p[2].clamp(-lim, lim);
                let max_y = (trunk + MAX_CANOPY_UP - 1) as f32;
                p[1] = p[1].clamp(crown_base * 0.5, max_y);
                let depth = nodes[ni].depth.saturating_add(1);
                let thickness = if depth <= 2 { 2 } else { 1 };
                let node = Node {
                    p,
                    parent: ni as u16,
                    depth,
                };
                nodes[nn] = node;
                shape.push_branch(Branch {
                    a: nodes[ni].p_i16(),
                    b: node.p_i16(),
                    thickness,
                });
                nn += 1;
                grew = true;
            }
            if !grew {
                break;
            }

            // Atractores alcanzados por cualquier nodo.
            let mut all = true;
            for (ai, a) in attractors[..na].iter().enumerate() {
                if reached[ai] {
                    continue;
                }
                let mut hit = false;
                for node in &nodes[..nn] {
                    let dx = a[0] - node.p[0];
                    let dy = a[1] - node.p[1];
                    let dz = a[2] - node.p[2];
                    if dx * dx + dy * dy + dz * dz <= (prof.branch_length * 1.1).powi(2) {
                        hit = true;
                        break;
                    }
                }
                reached[ai] = hit;
                if !hit {
                    all = false;
                }
            }
            if all {
                break;
            }
        }

        // Hojas: una bola pequena en cada punta (nodo sin hijos).
        for ni in 0..nn {
            let has_child = nodes[..nn]
                .iter()
                .any(|n| n.parent as usize == ni && ni != 0);
            if has_child || nodes[ni].depth < 2 {
                continue;
            }
            let p = nodes[ni].p_i16();
            for dy in 0..=1i16 {
                for dz in -1i16..=1i16 {
                    for dx in -1i16..=1i16 {
                        if dx.abs() + dz.abs() > 1 && dy == 1 {
                            continue;
                        }
                        shape.push_leaf([p[0] + dx, p[1] + dy, p[2] + dz]);
                    }
                }
            }
            if shape.leaf_count as usize >= MAX_LEAVES {
                break;
            }
        }
        if shape.leaf_count == 0 {
            let top = nodes[nn - 1].p_i16();
            shape.push_leaf(top);
            shape.push_leaf([top[0], top[1] + 1, top[2]]);
        }
        shape
    }

    /// Expone el esqueleto para benchmarks (`space_colonization`).
    pub fn shape_for_bench(&self, wx: i32, wz: i32, species: Species, seed: u32) -> TreeShape {
        self.build_shape(wx, wz, species, seed)
    }

    // -----------------------------------------------------------------------
    // Dibujo (comun a ambos caminos).
    // -----------------------------------------------------------------------

    /// Dibuja en `column` (chunk en `world_x, world_z`) la parte del arbol que
    /// cae dentro. Solo escribe sobre **aire** (nunca pisa terreno, agua ni otro
    /// arbol); el tronco tiene prioridad sobre las hojas.
    pub fn draw(&self, column: &mut Column, plan: &TreePlan, world_x: i32, world_z: i32) {
        self.for_each_block(plan, |dx, dy, dz, block| {
            let wx = plan.wx - world_x + dx;
            let wz = plan.wz - world_z + dz;
            let wy = plan.ground + dy;
            if wx < 0
                || wz < 0
                || wx >= CHUNK_SIZE as i32
                || wz >= CHUNK_SIZE as i32
                || wy < 0
                || wy >= WORLD_HEIGHT as i32
            {
                return;
            }
            let (wx, wy, wz) = (wx as usize, wy as usize, wz as usize);
            let existing = column.get(wx, wy, wz);
            let place = match block {
                Block::Wood => existing == Block::Air || existing == Block::Leaves,
                _ => existing == Block::Air,
            };
            if place {
                column.set(wx, wy, wz, block);
            }
        });
    }

    /// Bloque del arbol en el desplazamiento `(dx, dy, dz)` respecto al tronco
    /// (`dy` relativo al suelo). `None` = aire. Solo tests.
    #[cfg(test)]
    fn block_at(&self, p: &TreePlan, dx: i32, dy: i32, dz: i32) -> Option<Block> {
        let mut found = None;
        self.for_each_block(p, |bx, by, bz, block| {
            if found.is_none() && bx == dx && by == dy && bz == dz {
                found = Some(block);
            }
        });
        found
    }

    /// Enumera los voxeles del arbol en coordenadas relativas al tronco/suelo.
    fn for_each_block(&self, plan: &TreePlan, mut f: impl FnMut(i32, i32, i32, Block)) {
        if let Some(shape) = plan.shape {
            for br in shape.branches() {
                raster_branch(br, &mut f);
            }
            for leaf in shape.leaves() {
                f(
                    leaf[0] as i32,
                    leaf[1] as i32,
                    leaf[2] as i32,
                    Block::Leaves,
                );
            }
        } else {
            for dy in 0..=(plan.trunk + MAX_CANOPY_UP) {
                for dz in -MARGIN..=MARGIN {
                    for dx in -MARGIN..=MARGIN {
                        if let Some(block) = self.legacy_block_at(plan, dx, dy, dz) {
                            f(dx, dy, dz, block);
                        }
                    }
                }
            }
        }
    }

    fn legacy_block_at(&self, p: &TreePlan, dx: i32, dy: i32, dz: i32) -> Option<Block> {
        if dy >= 0 && dy < p.trunk && dx == 0 && dz == 0 {
            return Some(Block::Wood);
        }
        if self.in_canopy(p, dx, dy, dz) {
            Some(Block::Leaves)
        } else {
            None
        }
    }

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

/// Nodo del esqueleto.
#[derive(Clone, Copy)]
struct Node {
    p: [f32; 3],
    parent: u16,
    depth: u8,
}

impl Node {
    const ZERO: Self = Self {
        p: [0.0; 3],
        parent: 0,
        depth: 0,
    };

    fn p_i16(self) -> [i16; 3] {
        [
            self.p[0].round() as i16,
            self.p[1].round() as i16,
            self.p[2].round() as i16,
        ]
    }
}

impl TreeShape {
    fn push_branch(&mut self, b: Branch) {
        let i = self.branch_count as usize;
        if i < MAX_BRANCHES {
            self.branches[i] = b;
            self.branch_count += 1;
        }
    }

    fn push_leaf(&mut self, p: [i16; 3]) {
        if p[0].abs() > ORGANIC_MARGIN as i16 || p[2].abs() > ORGANIC_MARGIN as i16 {
            return;
        }
        let i = self.leaf_count as usize;
        if i >= MAX_LEAVES {
            return;
        }
        if self.leaves[..i].contains(&p) {
            return;
        }
        self.leaves[i] = p;
        self.leaf_count += 1;
    }
}

/// Rasteriza un tramo en voxeles de 1-2 bloques de grosor.
fn raster_branch(br: &Branch, f: &mut impl FnMut(i32, i32, i32, Block)) {
    let a = [br.a[0] as f32, br.a[1] as f32, br.a[2] as f32];
    let b = [br.b[0] as f32, br.b[1] as f32, br.b[2] as f32];
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    let steps = (len * 2.0).ceil().max(1.0) as i32;
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let x = (a[0] + (b[0] - a[0]) * t).round() as i32;
        let y = (a[1] + (b[1] - a[1]) * t).round() as i32;
        let z = (a[2] + (b[2] - a[2]) * t).round() as i32;
        if br.thickness >= 2 {
            for ox in 0..2 {
                for oz in 0..2 {
                    f(x + ox, y, z + oz, Block::Wood);
                }
            }
        } else {
            f(x, y, z, Block::Wood);
        }
    }
}

/// Copa ovoide legacy (roble, abedul) o plana (acacia).
fn ellipsoid_canopy(p: &TreePlan, dx: i32, dy: i32, dz: i32, n: f32) -> bool {
    let (cy, rx, ry, rz) = match p.species {
        Species::Oak => (p.trunk - 1, 2.6f32, 2.0f32, 2.6f32),
        Species::Birch => (p.trunk - 1, 1.7, 2.6, 1.7),
        Species::Acacia => (p.trunk, 3.0, 1.0, 3.0),
        Species::Spruce => unreachable!("la picea usa el cono"),
    };
    let grow = 1.0 + 0.28 * n;
    let fx = dx as f32 / rx;
    let fy = (dy - cy) as f32 / ry;
    let fz = dz as f32 / rz;
    fx * fx + fy * fy + fz * fz <= grow
}

/// Copa conica en capas de la picea legacy.
fn spruce_canopy(p: &TreePlan, dx: i32, dy: i32, dz: i32, n: f32) -> bool {
    if dy < 1 || dy > p.trunk + 1 {
        return false;
    }
    let t = (dy - 1) as f32 / p.trunk.max(1) as f32;
    let r = (3.0 * (1.0 - t) + 0.5) + 0.5 * n;
    if r <= 0.0 {
        return false;
    }
    (dx * dx + dz * dz) as f32 <= r * r
}

/// Candidato Poisson de una celda.
#[derive(Clone, Copy)]
struct Candidate {
    wx: i32,
    wz: i32,
    priority: u64,
}

/// PRNG SplitMix64: pequeno, rapido y determinista con semilla explicita.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }
}

fn cell_seed(seed: u32, cx: i32, cz: i32) -> u64 {
    (seed as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((cx as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((cz as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB))
}

/// Hash determinista de `(x, z)` en `[0, 1)` (solo camino legacy).
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

    fn collect_blocks(p: &TreePlacer, plan: &TreePlan) -> Vec<(i32, i32, i32, Block)> {
        // Un voxel puede ser tronco y hoja a la vez; el dibujo da prioridad al
        // tronco, asi que el mapa tiene que reproducir esa prioridad.
        let mut map = std::collections::BTreeMap::new();
        p.for_each_block(plan, |dx, dy, dz, b| {
            let key = (dx, dy, dz);
            let prev = map.get(&key).copied();
            if prev != Some(Block::Wood) {
                map.insert(key, b);
            }
        });
        map.into_iter()
            .map(|((dx, dy, dz), b)| (dx, dy, dz, b))
            .collect()
    }

    // --- Legacy: se conserva el comportamiento historico. ---

    #[test]
    fn el_decidor_es_determinista_y_global() {
        let a = TreePlacer::new(7);
        let b = TreePlacer::new(7);
        for (x, z) in [(0, 0), (321, -98), (-4000, 2500)] {
            assert_eq!(
                a.plan(x, z, Biome::Forest, 70, 0)
                    .map(|p| (p.species, p.trunk)),
                b.plan(x, z, Biome::Forest, 70, 0)
                    .map(|p| (p.species, p.trunk))
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
                    return;
                }
            }
        }
        panic!("no se encontro un arbol que cruce la frontera");
    }

    #[test]
    fn las_hojas_no_pisan_solidos_ni_agua() {
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

    // --- Organico (P0.3). ---

    #[test]
    fn test_poisson_min_distance() {
        // Dos troncos aceptados nunca estan a menos de `r` (densidad constante,
        // mismo radio para ambos: r = 0.5*(r_i+r_j)).
        let p = TreePlacer::new(13_371);
        let density = 0.75;
        let r = TreePlacer::radius_for_density(density);
        let mut points = Vec::new();
        for wz in 0..96 {
            for wx in 0..96 {
                if let Some(plan) = p.plan_organic(wx, wz, Biome::Forest, 70, 0, density) {
                    points.push((plan.wx, plan.wz));
                }
            }
        }
        assert!(points.len() > 12, "muy pocos arboles: {}", points.len());
        for i in 0..points.len() {
            for j in i + 1..points.len() {
                let dx = (points[i].0 - points[j].0) as f32;
                let dz = (points[i].1 - points[j].1) as f32;
                let d = (dx * dx + dz * dz).sqrt();
                assert!(
                    d + 1e-3 >= r,
                    "troncos a {d} bloques (< {r}) en {:?} y {:?}",
                    points[i],
                    points[j]
                );
            }
        }
    }

    #[test]
    fn test_tree_crosses_chunk_fully() {
        // Un arbol organico cuyo centro cae fuera de un chunk se dibuja entero
        // entre los dos chunks que toca (margen `ORGANIC_MARGIN`).
        let p = TreePlacer::new(13_371);
        let mut plan = None;
        'search: for wz in 2..64 {
            for wx in 12..20 {
                if let Some(pl) = p.plan_organic(wx, wz, Biome::Forest, 70, 0, 0.9)
                    && pl.wx - ORGANIC_MARGIN < 16
                    && pl.wx + ORGANIC_MARGIN >= 16
                {
                    plan = Some(pl);
                    break 'search;
                }
            }
        }
        let plan = plan.expect("no se encontro un arbol que cruce x=16");
        let base_x = plan.wx.div_euclid(16) * 16;
        let mut a = Column::empty();
        let mut b = Column::empty();
        p.draw(&mut a, &plan, base_x, 0);
        p.draw(&mut b, &plan, base_x + 16, 0);

        let mut total = 0u32;
        for (dx, dy, dz, block) in collect_blocks(&p, &plan) {
            let wx = plan.wx + dx;
            let wz = plan.wz + dz;
            let wy = plan.ground + dy;
            if !(0..CHUNK_SIZE as i32).contains(&wz) || !(0..WORLD_HEIGHT as i32).contains(&wy) {
                continue;
            }
            let got = if wx < base_x + 16 {
                let lx = wx - base_x;
                if !(0..CHUNK_SIZE as i32).contains(&lx) {
                    continue;
                }
                a.get(lx as usize, wy as usize, wz as usize)
            } else {
                let lx = wx - (base_x + 16);
                if !(0..CHUNK_SIZE as i32).contains(&lx) {
                    continue;
                }
                b.get(lx as usize, wy as usize, wz as usize)
            };
            assert_eq!(got, block, "bloque perdido en ({wx},{wy},{wz})");
            total += 1;
        }
        assert!(total > 0, "el arbol no emitio bloques");
    }

    #[test]
    fn test_tree_not_on_water() {
        let p = TreePlacer::new(13_371);
        // Densidad 0 (agua/cauce): no hay arbol.
        for wz in 0..40 {
            for wx in 0..40 {
                assert!(
                    p.plan_organic(wx, wz, Biome::Forest, 70, 0, 0.0).is_none(),
                    "arbol sobre agua en ({wx},{wz})"
                );
            }
        }
        // Bajo el nivel del mar tampoco.
        for wx in 0..40 {
            for wz in 0..40 {
                assert!(
                    p.plan_organic(wx, wz, Biome::Forest, SEA_LEVEL, 0, 1.0)
                        .is_none(),
                    "arbol bajo el mar en ({wx},{wz})"
                );
            }
        }
    }

    #[test]
    fn test_organic_shape_is_branched_and_bounded() {
        let p = TreePlacer::new(9);
        let mut found = 0;
        for wz in 0..80 {
            for wx in 0..80 {
                let Some(plan) = p.plan_organic(wx, wz, Biome::Forest, 70, 0, 1.0) else {
                    continue;
                };
                let shape = plan.shape.expect("el plan organico debe traer esqueleto");
                assert!(shape.branches().len() >= 4, "esqueleto sin ramas");
                assert!(!shape.leaves().is_empty(), "arbol sin hojas");
                assert_eq!(
                    shape,
                    p.build_shape(wx, wz, plan.species, plan.seed),
                    "Space Colonization no es determinista"
                );
                for (dx, dy, dz, _) in collect_blocks(&p, &plan) {
                    assert!(
                        dx.abs() <= ORGANIC_MARGIN && dz.abs() <= ORGANIC_MARGIN,
                        "fuera de margen"
                    );
                    assert!(
                        (0..=plan.trunk + MAX_CANOPY_UP).contains(&dy),
                        "altura fuera de cota: {dy}"
                    );
                }
                found += 1;
                if found >= 8 {
                    return;
                }
            }
        }
        assert!(found > 0, "no se genero ningun arbol organico");
    }
}
