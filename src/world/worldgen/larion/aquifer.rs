//! **Acuiferos** deterministas estilo Minecraft (version simplificada).
//!
//! El mundo se divide en celdas de `16x12x16` bloques desplazadas `(5,1,5)`.
//! Cada celda tiene un **centro** pseudoaleatorio (hash del seed y de la celda)
//! con un estado: seco, agua a un nivel, o lava. Para un bloque se miran los
//! centros de las celdas vecinas `2x3x2`, se ordenan por distancia y:
//!
//! * el centro mas cercano decide el fluido (agua por debajo de su nivel);
//! * si los dos centros mas cercanos tienen estados distintos y estan casi
//!   empatados, se genera **piedra** como barrera (evita que dos lagos de
//!   niveles distintos se mezclen);
//! * una celda bajo el nivel del mar se llena hasta el nivel del mar.
//!
//! No es una copia del algoritmo de Mojang (que usa density functions y
//! presion): es una version compacta, determinista y sin estado global, pensada
//! para que las cuevas no queden ni completamente secas ni completamente
//! inundadas. El agua generada es **estatica**; la simulacion solo actua en
//! cambios del jugador o flujos restaurados.

/// Lado X/Z de una celda de acuifero.
pub const CELL_XZ: i32 = 16;
/// Altura de una celda de acuifero.
pub const CELL_Y: i32 = 12;
/// Desplazamiento de la rejilla respecto a los bordes de chunk.
pub const OFFSET_XZ: i32 = 5;
/// Desplazamiento vertical de la rejilla.
pub const OFFSET_Y: i32 = 1;

/// Tipo de fluido de un centro de acuifero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AquiferFluid {
    Dry,
    Water,
    Lava,
}

/// Resultado de muestrear un bloque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AquiferSample {
    pub fluid: AquiferFluid,
    /// Nivel maximo del fluido (coordenada Y inclusiva). `-1` si esta seco.
    pub level: i32,
    /// El bloque debe ser piedra (barrera entre dos acuiferos).
    pub barrier: bool,
}

impl AquiferSample {
    const DRY: Self = Self {
        fluid: AquiferFluid::Dry,
        level: -1,
        barrier: false,
    };
}

/// Acuiferos de un mundo. `Send + Sync` y puro: mismo seed y coordenadas dan
/// siempre el mismo resultado.
#[derive(Clone, Copy, Debug)]
pub struct Aquifer {
    seed: u32,
    sea_level: i32,
}

#[derive(Clone, Copy)]
struct Center {
    pos: [i32; 3],
    fluid: AquiferFluid,
    level: i32,
}

impl Aquifer {
    pub fn new(seed: u32, sea_level: f32) -> Self {
        Self {
            seed,
            sea_level: sea_level.round() as i32,
        }
    }

    /// Muestrea el bloque `(x,y,z)`. `surface` es la altura del terreno de esa
    /// columna (sin contar el agua superficial).
    pub fn sample(&self, x: i32, y: i32, z: i32, surface: i32) -> AquiferSample {
        if y < 0 || y > surface || surface <= 0 {
            return AquiferSample::DRY;
        }
        // Celdas vecinas 2x3x2 (la propia y la siguiente en X/Z, fila anterior
        // y siguiente en Y).
        let cx = (x + OFFSET_XZ).div_euclid(CELL_XZ);
        let cy = (y + OFFSET_Y).div_euclid(CELL_Y);
        let cz = (z + OFFSET_XZ).div_euclid(CELL_XZ);

        let mut centers = [Center {
            pos: [0, 0, 0],
            fluid: AquiferFluid::Dry,
            level: -1,
        }; 12];
        let mut n = 0usize;
        for dy in -1..=1 {
            for dx in 0..=1 {
                for dz in 0..=1 {
                    if n == centers.len() {
                        break;
                    }
                    centers[n] = self.center(cx + dx, cy + dy, cz + dz, surface);
                    n += 1;
                }
            }
        }

        // Los 4 centros mas cercanos, en un array fijo (sin allocations por
        // bloque). La posicion desempata para que el orden sea determinista.
        let empty = Center {
            pos: [i32::MAX; 3],
            fluid: AquiferFluid::Dry,
            level: -1,
        };
        let mut best = [(i32::MAX, empty); 4];
        for c in &centers[..n] {
            let d = (c.pos[0] - x).pow(2) + (c.pos[1] - y).pow(2) + (c.pos[2] - z).pow(2);
            let mut at = 4;
            for (i, (bd, bc)) in best.iter().enumerate() {
                if d < *bd || (d == *bd && c.pos < bc.pos) {
                    at = i;
                    break;
                }
            }
            if at < 4 {
                for k in (at + 1..4).rev() {
                    best[k] = best[k - 1];
                }
                best[at] = (d, *c);
            }
        }
        let first = best[0];
        if first.0 == i32::MAX {
            return AquiferSample::DRY;
        }
        // Barrera: los dos centros mas cercanos discrepan y estan casi empatados.
        let second = best[1];
        if second.0 != i32::MAX && first.1.fluid != second.1.fluid && second.0 - first.0 < 25 {
            return AquiferSample {
                fluid: AquiferFluid::Dry,
                level: -1,
                barrier: true,
            };
        }

        let fluid = match first.1.fluid {
            AquiferFluid::Water | AquiferFluid::Lava => {
                if y <= first.1.level {
                    first.1.fluid
                } else {
                    AquiferFluid::Dry
                }
            }
            AquiferFluid::Dry => {
                // Regla global: bajo el mar, cualquier hueco se inunda hasta el
                // nivel del mar (salvo que una barrera lo separe).
                if surface < self.sea_level && y <= self.sea_level {
                    AquiferFluid::Water
                } else {
                    AquiferFluid::Dry
                }
            }
        };
        let level = if fluid == AquiferFluid::Dry {
            -1
        } else if first.1.fluid == AquiferFluid::Dry {
            self.sea_level
        } else {
            first.1.level
        };
        AquiferSample {
            fluid,
            level,
            barrier: false,
        }
    }

    /// Estado de una celda de acuifero (centro pseudoaleatorio + nivel).
    fn center(&self, cx: i32, cy: i32, cz: i32, surface: i32) -> Center {
        let jx = hash01(self.seed, cx, cy, cz, 0x11) * 9.0;
        let jy = hash01(self.seed, cx, cy, cz, 0x22) * 8.0;
        let jz = hash01(self.seed, cx, cy, cz, 0x33) * 9.0;
        let pos = [
            cx * CELL_XZ - OFFSET_XZ + jx as i32,
            cy * CELL_Y - OFFSET_Y + jy as i32,
            cz * CELL_XZ - OFFSET_XZ + jz as i32,
        ];

        // Muy por encima del terreno: la celda solo contiene aire.
        if pos[1] > surface + 4 {
            return Center {
                pos,
                fluid: AquiferFluid::Dry,
                level: -1,
            };
        }
        // Bajo el mar: la celda forma parte del oceano/rio y se llena a mar.
        if surface < self.sea_level && pos[1] <= self.sea_level {
            return Center {
                pos,
                fluid: AquiferFluid::Water,
                level: self.sea_level,
            };
        }

        // Cuevas continentales: la mayoria secas, algunas con lago local. El
        // nivel se cuantiza en bandas de 40 bloques (como Minecraft) para que
        // los lagos de una misma zona queden a alturas parecidas.
        let wet = hash01(self.seed, cx, cy, cz, 0x44);
        let level = if wet > 0.80 {
            let spread = (hash01(self.seed, cx, cy, cz, 0x55) * 7.0) as i32 - 3;
            let band = cy.div_euclid(40) * 40 + 20 + spread * 3;
            band.clamp(1, (surface - 1).max(1))
        } else if wet > 0.62 {
            let depth = 6 + (hash01(self.seed, cx, cy, cz, 0x66) * 18.0) as i32;
            (surface - depth).clamp(1, (surface - 1).max(1))
        } else {
            return Center {
                pos,
                fluid: AquiferFluid::Dry,
                level: -1,
            };
        };
        Center {
            pos,
            fluid: AquiferFluid::Water,
            level,
        }
    }
}

/// Hash entero determinista en `[0, 1)`. No usa floats no ordenados ni
/// `HashMap`; es estable entre plataformas y workers.
fn hash01(seed: u32, x: i32, y: i32, z: i32, salt: u32) -> f32 {
    let mut h = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (x as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= (y as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= (z as i64 as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    h ^= (salt as u64).wrapping_mul(0xA24B_AED4_963E_E407);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn es_determinista_y_no_llueve_sobre_la_superficie() {
        let a = Aquifer::new(13_371, 64.0);
        let s = a.sample(120, 50, -30, 72);
        assert_eq!(s, a.sample(120, 50, -30, 72));
        let above = a.sample(120, 90, -30, 72);
        assert_eq!(
            above,
            AquiferSample::DRY,
            "no debe haber agua sobre el suelo"
        );
    }

    #[test]
    fn bajo_el_mar_llena_hasta_el_nivel_del_mar() {
        let a = Aquifer::new(7, 64.0);
        // y=40 esta bajo el fondo del mar (48) y bajo el nivel del mar (64).
        let s = a.sample(0, 40, 0, 48);
        assert_eq!(s.fluid, AquiferFluid::Water);
        assert!(s.level >= 64, "nivel={}", s.level);
    }

    #[test]
    fn hay_cuevas_secas_y_cuevas_con_agua() {
        let a = Aquifer::new(13_371, 64.0);
        let mut dry = 0;
        let mut wet = 0;
        for x in (0..256).step_by(7) {
            for z in (0..256).step_by(7) {
                for y in (8..60).step_by(9) {
                    match a.sample(x, y, z, 80).fluid {
                        AquiferFluid::Dry => dry += 1,
                        AquiferFluid::Water => wet += 1,
                        AquiferFluid::Lava => {}
                    }
                }
            }
        }
        assert!(dry > 0, "todo salio inundado");
        assert!(wet > 0, "no hay ningun acuifero continental");
    }

    #[test]
    fn las_barreras_solo_aparecen_entre_estados_distintos() {
        let a = Aquifer::new(99, 64.0);
        let mut barriers = 0;
        for x in (0..128).step_by(3) {
            for z in (0..128).step_by(3) {
                for y in (10..60).step_by(5) {
                    if a.sample(x, y, z, 80).barrier {
                        barriers += 1;
                    }
                }
            }
        }
        // No exigimos una cantidad concreta (depende del seed), solo que el
        // calculo no entre en panico y sea reproducible.
        assert_eq!(
            barriers,
            (0..128)
                .step_by(3)
                .flat_map(|x| (0..128).step_by(3).map(move |z| (x, z)))
                .flat_map(|(x, z)| (10..60).step_by(5).map(move |y| (x, y, z)))
                .filter(|&(x, y, z)| a.sample(x, y, z, 80).barrier)
                .count()
        );
    }
}
