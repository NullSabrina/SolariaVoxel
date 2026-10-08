//! Frustum de la camara, para descartar lo que no se ve (frustum culling).
//!
//! El frustum es la piramide de vision: todo lo que cae fuera de sus seis planos
//! (izquierda, derecha, abajo, arriba, cercano, lejano) no puede aparecer en
//! pantalla. Lo extraemos de la matriz `view_projection` (metodo de Gribb y
//! Hartmann: cada plano es una combinacion de las filas de la matriz en espacio
//! de clip) y lo usamos para saltarnos mallas enteras antes de dibujarlas.

use super::Mat4;

/// Los seis planos del frustum, como `(a, b, c, d)` de `a*x + b*y + c*z + d`.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extrae los planos de una matriz `view_projection`.
    pub fn from_view_projection(vp: &Mat4) -> Self {
        // `to_cols_array` es column-major: el elemento de la fila `r`, columna
        // `c`, esta en `arr[c * 4 + r]`.
        let m = vp.to_cols_array();
        let row = |r: usize| [m[r], m[4 + r], m[8 + r], m[12 + r]];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));

        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];

        let planes = [
            add(r3, r0), // izquierda
            sub(r3, r0), // derecha
            add(r3, r1), // abajo
            sub(r3, r1), // arriba
            r2,          // cercano (wgpu usa NDC z en [0,1]: z_clip >= 0)
            sub(r3, r2), // lejano
        ];
        Self { planes }
    }

    /// ¿El AABB `[min, max]` esta al menos parcialmente dentro del frustum?
    ///
    /// Para cada plano tomamos el vertice del AABB mas alejado en la direccion de
    /// la normal (`p-vertex`); si ese vertice queda fuera (`dot < 0`), todo el
    /// AABB esta fuera y se descarta.
    pub fn intersects_aabb(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        for p in &self.planes {
            let px = if p[0] >= 0.0 { max[0] } else { min[0] };
            let py = if p[1] >= 0.0 { max[1] } else { min[1] };
            let pz = if p[2] >= 0.0 { max[2] } else { min[2] };
            let dist = p[0] * px + p[1] * py + p[2] * pz + p[3];
            if dist < 0.0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    /// Vista desde el origen mirando a -Z, con una proyeccion de 70 grados.
    fn vp() -> Mat4 {
        let proj = Mat4::perspective_rh(70f32.to_radians(), 16.0 / 9.0, 0.1, 1000.0);
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        proj * view
    }

    #[test]
    fn una_caja_delante_se_ve() {
        let f = Frustum::from_view_projection(&vp());
        assert!(f.intersects_aabb([-1.0, -1.0, -20.0], [1.0, 1.0, -18.0]));
    }

    #[test]
    fn una_caja_detras_no_se_ve() {
        let f = Frustum::from_view_projection(&vp());
        assert!(!f.intersects_aabb([-1.0, -1.0, 20.0], [1.0, 1.0, 22.0]));
    }

    #[test]
    fn una_caja_muy_a_la_derecha_no_se_ve() {
        let f = Frustum::from_view_projection(&vp());
        assert!(!f.intersects_aabb([500.0, -1.0, -20.0], [502.0, 1.0, -18.0]));
    }

    #[test]
    fn una_caja_que_rodea_la_camara_se_ve() {
        let f = Frustum::from_view_projection(&vp());
        assert!(f.intersects_aabb([-5.0, -5.0, -5.0], [5.0, 5.0, 5.0]));
    }
}
