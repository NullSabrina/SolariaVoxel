//! Matriz de 4x4 `f32`, guardada en **column-major** (columnas primero).
//!
//! Por que column-major: wgpu, Vulkan, OpenGL y Metal reciben las matrices asi.
//! Si la guardaramos por filas tendriamos que transponerla en cada `write_buffer`,
//! y transponer cuesta y se olvida facil. [`Mat4::to_cols_array`] devuelve los
//! 16 floats listos para subir a la GPU.
//!
//! Notacion: `cols[c][r]` es el elemento de la fila `r` y la columna `c`, que es
//! la posicion `4*c + r` en el array plano.

use std::ops::Mul;

use super::Vec3;

/// Matriz de transformacion 4x4 afin/proyectiva.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    cols: [[f32; 4]; 4],
}

impl Mat4 {
    /// La matriz identidad: no transforma nada.
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Construye una matriz a partir de sus 4 columnas.
    #[inline]
    pub const fn from_cols(c0: [f32; 4], c1: [f32; 4], c2: [f32; 4], c3: [f32; 4]) -> Self {
        Self {
            cols: [c0, c1, c2, c3],
        }
    }

    /// Aplana la matriz a 16 `f32` en orden column-major, listo para `wgpu`.
    #[inline]
    pub fn to_cols_array(&self) -> [f32; 16] {
        let c = &self.cols;
        [
            c[0][0], c[0][1], c[0][2], c[0][3], // columna 0
            c[1][0], c[1][1], c[1][2], c[1][3], // columna 1
            c[2][0], c[2][1], c[2][2], c[2][3], // columna 2
            c[3][0], c[3][1], c[3][2], c[3][3], // columna 3
        ]
    }

    /// Matriz de proyeccion en perspectiva, mano derecha, con profundidad en
    /// `[0, 1]` (el rango que exige wgpu/Direct3D/Metal/Vulkan).
    ///
    /// * `fov_y_rad`: campo de vision vertical en radianes.
    /// * `aspect`: ancho / alto de la ventana.
    /// * `near`, `far`: planos de recorte (deben ser > 0 y `near < far`).
    pub fn perspective_rh(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        // f = 1 / tan(fov/2): factor de escala del plano cercano.
        let f = 1.0 / (fov_y_rad * 0.5).tan();
        Self::from_cols(
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, far / (near - far), -1.0],
            [0.0, 0.0, (far * near) / (near - far), 0.0],
        )
    }

    /// Matriz "view" (de camara), mano derecha. Coloca el mundo respecto a una
    /// camara situada en `eye`, mirando a `center`, con `up` como "arriba".
    pub fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Self {
        // f: direccion de vision (de la camara al objetivo).
        let f = (center - eye).normalize();
        // s: "derecha" de la camara, perpendicular a f y a up.
        let s = f.cross(up).normalize();
        // u: "arriba" real de la camara, ya ortogonalizado.
        let u = s.cross(f);
        Self::from_cols(
            [s.x, u.x, -f.x, 0.0],
            [s.y, u.y, -f.y, 0.0],
            [s.z, u.z, -f.z, 0.0],
            [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
        )
    }
}

/// Producto de matrices: `a * b` aplica primero `b` y luego `a`, como en
/// algebra lineal. Lo usaremos para calcular `proyeccion * vista`.
impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for (c, col) in out.iter_mut().enumerate() {
            for (r, elem) in col.iter_mut().enumerate() {
                // out.cols[c][r] = sum_k rhs.cols[c][k] * self.cols[k][r]
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += rhs.cols[c][k] * self.cols[k][r];
                }
                *elem = sum;
            }
        }
        Self { cols: out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identidad_no_cambia_a_otra_matriz() {
        let m = Mat4::perspective_rh(1.0, 1.6, 0.1, 1000.0);
        assert_eq!(m * Mat4::IDENTITY, m);
        assert_eq!(Mat4::IDENTITY * m, m);
    }

    #[test]
    fn look_at_desde_origen_mirando_a_menos_z_es_identidad() {
        // Camara en el origen mirando hacia -Z con "arriba" = +Y:
        // su matriz de vista debe ser la identidad.
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        let expect = Mat4::IDENTITY.to_cols_array();
        let got = view.to_cols_array();
        for (a, b) in expect.iter().zip(got.iter()) {
            assert!((a - b).abs() < 1e-5, "esperado {a}, obtenido {b}");
        }
    }
}
