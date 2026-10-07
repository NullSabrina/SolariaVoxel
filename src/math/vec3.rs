//! Vector de 3 componentes `f32`.
//!
//! Lo usamos para posiciones, direcciones y (en el futuro) normales de los
//! voxeles. Es `Copy` porque son solo 12 bytes; copiarlo por valor es mas
//! rapido y comodo que pasarlo por referencia.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Vector de 3 dimensiones en coma flotante.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// El origen / vector nulo `(0, 0, 0)`.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    /// La base del eje X `(1, 0, 0)`.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);

    /// La base del eje Y `(0, 1, 0)` — "arriba" en nuestro mundo.
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);

    /// La base del eje Z `(0, 0, 1)`.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Constructor por componentes. Es `const` para poder usarlo en `const`.
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Producto escalar (dot product): cuanto apuntan dos vectores en la misma
    /// direccion. Vale 0 si son perpendiculares y es negativo si van opuestos.
    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Producto vectorial (cross product). Devuelve un vector perpendicular a
    /// `self` y `other`. El orden importa: `a.cross(b) == -b.cross(a)`.
    #[inline]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Longitud al cuadrado. Preferible a [`Vec3::length`] para comparar
    /// distancias (evita una raiz cuadrada).
    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Longitud (modulo) del vector.
    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Devuelve el vector normalizado (longitud 1). Si el vector es nulo
    /// devolvemos el vector nulo para no producir `NaN` con la division.
    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    /// Interpolacion lineal componente a componente (`t = 0` -> `self`,
    /// `t = 1` -> `other`). Se usa para interpolar la posicion de render entre
    /// dos pasos de fisica de timestep fijo.
    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

// --- Operadores aritmeticos (azucar sintactico sobre las componentes) --------

impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

/// Multiplicacion por un escalar: `v * 2.0`.
impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

/// Division por un escalar: `v / 2.0`.
impl Div<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comprobamos que redondeamos a 4 decimales para no pelear con el binario.
    fn approx(a: Vec3, b: Vec3) -> bool {
        let d = (a - b).length();
        d < 1e-4
    }

    #[test]
    fn cross_sigue_la_regla_de_la_mano_derecha() {
        // X x Y debe dar Z en un sistema de mano derecha.
        assert!(approx(Vec3::X.cross(Vec3::Y), Vec3::Z));
        // Y el producto es anticonmutativo.
        assert!(approx(Vec3::Y.cross(Vec3::X), -Vec3::Z));
    }

    #[test]
    fn normalize_da_longitud_uno() {
        let v = Vec3::new(3.0, 4.0, 0.0).normalize();
        assert!((v.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_de_vector_nulo_no_es_nan() {
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
    }

    #[test]
    fn dot_de_perpendiculares_es_cero() {
        assert!(Vec3::X.dot(Vec3::Y).abs() < 1e-6);
    }

    #[test]
    fn lerp_interpola_entre_dos_puntos() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 20.0, -30.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Vec3::new(5.0, 10.0, -15.0));
    }
}
