//! Camara en primera persona (FPS).
//!
//! Guarda su posicion y su orientacion (yaw/pitch) y sabe calcular:
//!
//! * [`Camera::forward`] — hacia donde mira, en coordenadas de mundo.
//! * [`Camera::view`] — la matriz que lleva el mundo al espacio de la camara.
//! * [`Camera::projection`] — la matriz que lleva ese espacio a la pantalla.
//!
//! Desde v0.1.1 la camara es **movible**: [`Camera::add_look`] aplica el giro
//! del raton y [`Camera::walk`] desplaza la camara segun WASD. La camara NO lee
//! el input directamente: recibe ya resuelto "cuanto girar" y "hacia donde
//! andar". Eso la mantiene facil de testear y desacoplada de winit.
//!
//! Convenio de orientacion (grados):
//! * `yaw` gira sobre el eje Y. En `yaw = 0` miramos hacia **-Z** (al frente),
//!   que es hacia donde miran los modelos de Minecraft.
//! * `pitch` gira sobre el eje X. Positivo mira hacia **arriba**.

use crate::math::{Mat4, Vec3};

/// Camara FPS con posicion y rotacion en grados.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Posicion de la camara en el mundo.
    pub position: Vec3,
    /// Giro horizontal en grados (izquierda/derecha).
    pub yaw_deg: f32,
    /// Giro vertical en grados (arriba/abajo), limitado a (-89, 89).
    pub pitch_deg: f32,
    /// Campo de vision vertical, en grados.
    pub fov_y_deg: f32,
    /// Plano de recorte cercano.
    pub near: f32,
    /// Plano de recorte lejano. Mas adelante dependera de la distancia de chunks.
    pub far: f32,
    /// Velocidad de desplazamiento en unidades por segundo.
    pub speed: f32,
    /// Sensibilidad del raton en grados de giro por pixel movido.
    pub sensitivity_deg_per_px: f32,

    /// Matriz de vista cacheada (se recalcula con `update_view`).
    view: Mat4,
    /// Matriz de proyeccion cacheada (se recalcula con `update_projection`).
    projection: Mat4,
}

impl Camera {
    /// Crea una camara en `position`, mirando al frente (-Z).
    pub fn new(position: Vec3) -> Self {
        let mut cam = Self {
            position,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            fov_y_deg: 70.0,
            near: 0.1,
            far: 1000.0,
            speed: 8.0,
            sensitivity_deg_per_px: 0.12,
            view: Mat4::IDENTITY,
            projection: Mat4::IDENTITY,
        };
        // Calculamos la vista inicial para que `view()` nunca devuelva basura.
        cam.update_view();
        cam
    }

    /// Direccion hacia la que mira la camara, normalizada.
    pub fn forward(&self) -> Vec3 {
        let yaw = self.yaw_deg.to_radians();
        let pitch = self.pitch_deg.to_radians();
        // Con yaw=0, pitch=0 -> (0, 0, -1): miramos al frente (-Z).
        let cos_pitch = pitch.cos();
        Vec3::new(yaw.sin() * cos_pitch, pitch.sin(), -yaw.cos() * cos_pitch).normalize()
    }

    /// Aplica el movimiento del raton (en pixels) al giro de la camara.
    ///
    /// * `dx` positivo = raton hacia la derecha -> giramos a la derecha.
    /// * `dy` positivo = raton hacia abajo -> miramos hacia abajo.
    ///
    /// El `pitch` se limita a (-89, 89) grados para no dar la vuelta (pole
    /// flip) y el `yaw` se envuelve a (-180, 180] para no perder precision tras
    /// muchas vueltas.
    pub fn add_look(&mut self, dx: f32, dy: f32) {
        self.yaw_deg += dx * self.sensitivity_deg_per_px;
        self.pitch_deg -= dy * self.sensitivity_deg_per_px;

        // Envolvemos el yaw.
        self.yaw_deg = self.yaw_deg.rem_euclid(360.0);
        if self.yaw_deg > 180.0 {
            self.yaw_deg -= 360.0;
        }
        // Limitamos el pitch.
        self.pitch_deg = self.pitch_deg.clamp(-89.0, 89.0);

        self.update_view();
    }

    /// Desplaza la camara segun el input (cada eje en `[-1, 1]`) durante `dt`
    /// segundos.
    ///
    /// `forward` es adelante/atras, `right` es derecha/izquierda y `up` es
    /// subir/bajar. El movimiento horizontal se calcula solo con el `yaw`
    /// (ignora el `pitch`): asi al mirar al suelo no frenamos, como en cualquier
    /// FPS. La direccion resultante se normaliza para que andar en diagonal no
    /// sea mas rapido que andar en recto.
    pub fn walk(&mut self, forward: f32, right: f32, up: f32, dt: f32) {
        let yaw = self.yaw_deg.to_radians();
        // "Adelante" horizontal (sin componente vertical).
        let fwd = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        // "Derecha" = adelante x arriba (mano derecha).
        let right_v = fwd.cross(Vec3::Y);

        let mut dir = fwd * forward + right_v * right + Vec3::Y * up;
        if dir.length_squared() > 0.0 {
            dir = dir.normalize();
        }

        self.position += dir * (self.speed * dt);
        self.update_view();
    }

    /// Recalcula la matriz de vista a partir de posicion y orientacion.
    pub fn update_view(&mut self) {
        let target = self.position + self.forward();
        self.view = Mat4::look_at_rh(self.position, target, Vec3::Y);
    }

    /// Recalcula la matriz de proyeccion. `aspect` es ancho/alto de la ventana.
    ///
    /// Si `aspect` no es valido (0, NaN, infinito) usamos 1.0 para no romper
    /// la matriz cuando la ventana esta minimizada.
    pub fn update_projection(&mut self, aspect: f32) {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        self.projection =
            Mat4::perspective_rh(self.fov_y_deg.to_radians(), aspect, self.near, self.far);
    }

    /// Matriz de vista actual.
    #[inline]
    pub fn view(&self) -> Mat4 {
        self.view
    }

    /// Matriz de proyeccion actual.
    #[inline]
    pub fn projection(&self) -> Mat4 {
        self.projection
    }

    /// Producto `proyeccion * vista`, la matriz que en v0.1.2 enviaremos al
    /// shader para transformar los vertices.
    #[inline]
    pub fn view_projection(&self) -> Mat4 {
        self.projection * self.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn al_frente_es_menos_z() {
        let cam = Camera::new(Vec3::ZERO);
        let f = cam.forward();
        // Sin rotacion debe mirar a -Z.
        assert!((f.x).abs() < 1e-6);
        assert!((f.y).abs() < 1e-6);
        assert!((f.z + 1.0).abs() < 1e-6);
    }

    #[test]
    fn yaw_90_mira_a_mas_x() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.yaw_deg = 90.0;
        let f = cam.forward();
        // yaw positivo gira hacia la derecha del modelo (+X).
        assert!((f.x - 1.0).abs() < 1e-5, "x = {}", f.x);
        assert!((f.z).abs() < 1e-5, "z = {}", f.z);
    }

    #[test]
    fn pitch_positivo_mira_hacia_arriba() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.pitch_deg = 45.0;
        assert!(cam.forward().y > 0.0);
    }

    #[test]
    fn aspect_invalido_no_produce_nan() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.update_projection(0.0);
        // Ninguna componente de la matriz debe ser NaN.
        assert!(
            cam.projection()
                .to_cols_array()
                .iter()
                .all(|v| v.is_finite())
        );
    }

    #[test]
    fn andar_hacia_adelante_con_yaw_cero_va_a_menos_z() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.walk(1.0, 0.0, 0.0, 1.0);
        assert!(cam.position.z < -1.0, "z = {}", cam.position.z);
        assert!(cam.position.x.abs() < 1e-5);
    }

    #[test]
    fn andar_en_diagonal_no_es_mas_rapido() {
        let mut recto = Camera::new(Vec3::ZERO);
        recto.walk(1.0, 0.0, 0.0, 1.0);
        let dist_recto = recto.position.length();

        let mut diagonal = Camera::new(Vec3::ZERO);
        diagonal.walk(1.0, 1.0, 0.0, 1.0);
        let dist_diagonal = diagonal.position.length();

        assert!((dist_recto - dist_diagonal).abs() < 1e-4);
    }

    #[test]
    fn add_look_limita_el_pitch() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.add_look(0.0, -100000.0); // raton muy hacia arriba
        assert!(cam.pitch_deg <= 89.0 + 1e-3);
        cam.add_look(0.0, 100000.0); // raton muy hacia abajo
        assert!(cam.pitch_deg >= -89.0 - 1e-3);
    }

    #[test]
    fn add_look_envuelve_el_yaw() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.add_look(100000.0, 0.0);
        assert!(cam.yaw_deg <= 180.0 && cam.yaw_deg > -180.0);
    }
}
