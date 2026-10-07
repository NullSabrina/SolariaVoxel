//! Muestreador **celular (Worley)** determinista (FASE 2).
//!
//! Divide el plano en celdas de lado `cell_distance` y da a cada una un centro
//! **jittered** obtenido por hash de sus coordenadas enteras. La consulta
//! devuelve la celda mas cercana (F1), la segunda (F2), su id estable y un
//! `edge` (0 en la frontera entre celdas, ~1 en el interior).
//!
//! El `cell_id` depende **solo** de `(seed, celda_entera)`, nunca del punto
//! consultado: asi el ancho de costa, la familia de bioma o features raras se
//! mantienen coherentes dentro de toda la celda (requisito de determinismo).

/// Resultado de muestrear la red celular en un punto.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellSample {
    /// Id estable de la celda mas cercana.
    pub id: u64,
    /// Coordenadas enteras de la celda mas cercana.
    pub cell_x: i32,
    pub cell_z: i32,
    /// Centro jittered de esa celda.
    pub center_x: f32,
    pub center_z: f32,
    /// Distancia al centro mas cercano (F1).
    pub f1: f32,
    /// Distancia al segundo centro (F2).
    pub f2: f32,
    /// `saturate((F2 - F1) / cell_distance)`: 0 en la frontera, ~1 dentro.
    pub edge: f32,
}

/// Hash entero `u64` determinista de una celda `(gx, gz)` con `salt`.
#[inline]
pub fn hash_u64(seed: u32, gx: i32, gz: i32, salt: u32) -> u64 {
    let mut h = (seed as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((gx as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add((gz as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9))
        .wrapping_add((salt as u64).wrapping_mul(0x27D4_EB2F_1656_67C5));
    // Mezcla final (splitmix64).
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

/// Hash de celda en `[0, 1)`.
#[inline]
pub fn hash01(seed: u32, gx: i32, gz: i32, salt: u32) -> f32 {
    // 24 bits altos -> fraccion exacta en [0,1).
    ((hash_u64(seed, gx, gz, salt) >> 40) as f32) / (1u32 << 24) as f32
}

/// Id estable de la celda `(gx, gz)`.
#[inline]
pub fn cell_id(seed: u32, gx: i32, gz: i32) -> u64 {
    hash_u64(seed, gx, gz, 0)
}

/// Muestrea la red celular en `(x, z)`. Revisa el barrio 3x3 de celdas para
/// obtener F1 y F2 correctos aun con jitter alto.
pub fn sample(seed: u32, cell_distance: f32, jitter: f32, x: f32, z: f32) -> CellSample {
    // `floor` funciona bien con negativos (no truncamiento hacia cero).
    let qx = (x / cell_distance).floor() as i32;
    let qz = (z / cell_distance).floor() as i32;

    let mut f1 = f32::INFINITY;
    let mut f2 = f32::INFINITY;
    let mut best = (qx, qz);

    for dz in -1..=1 {
        for dx in -1..=1 {
            let (gx, gz) = (qx + dx, qz + dz);
            // Centro jittered dentro de la celda: 0.5 +/- jitter/2.
            let jx = 0.5 + (hash01(seed, gx, gz, 1) - 0.5) * jitter;
            let jz = 0.5 + (hash01(seed, gx, gz, 2) - 0.5) * jitter;
            let cx = (gx as f32 + jx) * cell_distance;
            let cz = (gz as f32 + jz) * cell_distance;
            let d = ((cx - x).powi(2) + (cz - z).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
                best = (gx, gz);
            } else if d < f2 {
                f2 = d;
            }
        }
    }

    let (cell_x, cell_z) = best;
    let jx = 0.5 + (hash01(seed, cell_x, cell_z, 1) - 0.5) * jitter;
    let jz = 0.5 + (hash01(seed, cell_x, cell_z, 2) - 0.5) * jitter;

    CellSample {
        id: cell_id(seed, cell_x, cell_z),
        cell_x,
        cell_z,
        center_x: (cell_x as f32 + jx) * cell_distance,
        center_z: (cell_z as f32 + jz) * cell_distance,
        f1,
        f2,
        edge: super::math::saturate((f2 - f1) / cell_distance),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_id_de_celda_es_estable_e_independiente_del_punto() {
        // Dos puntos dentro de la misma celda deben dar el mismo id. Con
        // jitter 0 los centros son deterministas y la celda cubre un cuadrado.
        let s = sample(7, 300.0, 0.0, 10.0, 10.0);
        let s2 = sample(7, 300.0, 0.0, 200.0, 200.0);
        assert_eq!(s.id, s2.id, "mismo punto de rejilla -> misma celda");
        assert_eq!(s.cell_x, 0);
        assert_eq!(s.cell_z, 0);
    }

    #[test]
    fn el_id_cambia_entre_celdas_vecinas() {
        let a = sample(7, 300.0, 0.0, 150.0, 150.0); // celda 0,0
        let b = sample(7, 300.0, 0.0, 450.0, 150.0); // celda 1,0
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn funciona_en_coordenadas_negativas() {
        // floor(-0.3) = -1 (no 0 por truncamiento).
        let s = sample(7, 300.0, 0.0, -10.0, -10.0);
        assert_eq!(s.cell_x, -1);
        assert_eq!(s.cell_z, -1);
        // Continuidad: un punto a ambos lados de x=0 no debe dar un id absurdo.
        let l = sample(7, 300.0, 0.85, -1.0, 5.0);
        let r = sample(7, 300.0, 0.85, 1.0, 5.0);
        assert!(
            l.id == r.id,
            "a 2 bloques del origen debe ser la misma celda"
        );
    }

    #[test]
    fn f1_es_menor_o_igual_que_f2() {
        for i in 0..50 {
            let x = i as f32 * 37.0 - 500.0;
            let z = i as f32 * 53.0 - 400.0;
            let s = sample(1234, 320.0, 0.85, x, z);
            assert!(s.f1 <= s.f2 + 1e-3);
            assert!((0.0..=1.0).contains(&s.edge));
        }
    }

    #[test]
    fn es_determinista() {
        let a = sample(99, 320.0, 0.85, 123.4, -56.7);
        let b = sample(99, 320.0, 0.85, 123.4, -56.7);
        assert_eq!(a, b);
    }
}
