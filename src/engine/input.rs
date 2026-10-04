//! Estado del input del usuario (teclado y raton).
//!
//! winit nos entrega eventos *puntuales* ("se pulso W", "el raton se movio 3
//! pixels"), pero la camara necesita el estado *continuo* ("¿esta W pulsada
//! ahora mismo?"). Este modulo guarda ese estado y lo expone en forma de:
//!
//! * [`Input::is_pressed`] — ¿esta pulsada esta tecla?
//! * [`Input::forward_axis`], [`Input::right_axis`], [`Input::up_axis`] — los
//!   tres ejes de movimiento en `[-1, 1]`, listos para [`crate::scene::Camera::walk`].
//! * [`Input::take_mouse_delta`] — el desplazamiento acumulado del raton, que se
//!   *consume* (se pone a cero) al leerlo, para no aplicarlo dos veces.
//!
//! Guardamos las teclas por su `KeyCode` **fisico**: asi WASD sigue funcionando
//! en un teclado AZERTY o Dvorak (las teclas estan donde estan, no importa la
//! letra impresa).

use std::collections::HashSet;

use winit::event::ElementState;
use winit::keyboard::KeyCode;

/// Estado actual del teclado y acumulador del raton.
#[derive(Default)]
pub struct Input {
    /// Teclas pulsadas ahora mismo.
    pressed: HashSet<KeyCode>,
    /// Desplazamiento del raton acumulado desde la ultima lectura.
    mouse_delta: (f64, f64),
}

impl Input {
    /// Registra que una tecla se pulso o se solto.
    pub fn on_key(&mut self, code: KeyCode, state: ElementState) {
        match state {
            ElementState::Pressed => {
                self.pressed.insert(code);
            }
            ElementState::Released => {
                self.pressed.remove(&code);
            }
        }
    }

    /// ¿Esta pulsada esta tecla ahora mismo?
    #[inline]
    pub fn is_pressed(&self, code: KeyCode) -> bool {
        self.pressed.contains(&code)
    }

    /// Acumula un movimiento del raton (en la unidad cruda del dispositivo).
    pub fn on_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    /// Devuelve y resetea el desplazamiento acumulado del raton.
    #[inline]
    pub fn take_mouse_delta(&mut self) -> (f32, f32) {
        let d = (self.mouse_delta.0 as f32, self.mouse_delta.1 as f32);
        self.mouse_delta = (0.0, 0.0);
        d
    }

    /// Eje adelante/atras: W = +1, S = -1, ambos = 0.
    pub fn forward_axis(&self) -> f32 {
        axis(
            self.is_pressed(KeyCode::KeyW),
            self.is_pressed(KeyCode::KeyS),
        )
    }

    /// Eje derecha/izquierda: D = +1, A = -1.
    pub fn right_axis(&self) -> f32 {
        axis(
            self.is_pressed(KeyCode::KeyD),
            self.is_pressed(KeyCode::KeyA),
        )
    }

    /// ¿Esta pulsada la tecla de "subir" (Espacio)? Se usa en modo vuelo y salto.
    pub fn jump_held(&self) -> bool {
        self.is_pressed(KeyCode::Space)
    }

    /// ¿Se ha pedido un salto? (Espacio). En modo normal, salta si esta en suelo.
    pub fn jump_axis(&self) -> bool {
        self.is_pressed(KeyCode::Space)
    }
}

/// Convierte dos booleanos (positivo/negativo) en un eje `-1 | 0 | 1`.
#[inline]
fn axis(positive: bool, negative: bool) -> f32 {
    (positive as i32 - negative as i32) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eje_adelante_combina_w_y_s() {
        let mut input = Input::default();
        assert_eq!(input.forward_axis(), 0.0);
        input.on_key(KeyCode::KeyW, ElementState::Pressed);
        assert_eq!(input.forward_axis(), 1.0);
        input.on_key(KeyCode::KeyS, ElementState::Pressed);
        // W y S a la vez se cancelan.
        assert_eq!(input.forward_axis(), 0.0);
    }

    #[test]
    fn take_mouse_delta_resetea() {
        let mut input = Input::default();
        input.on_mouse_motion(3.0, -2.0);
        input.on_mouse_motion(1.0, 1.0);
        assert_eq!(input.take_mouse_delta(), (4.0, -1.0));
        // La segunda lectura ya esta a cero.
        assert_eq!(input.take_mouse_delta(), (0.0, 0.0));
    }

    #[test]
    fn soltar_tecla_la_quita() {
        let mut input = Input::default();
        input.on_key(KeyCode::KeyD, ElementState::Pressed);
        assert_eq!(input.right_axis(), 1.0);
        input.on_key(KeyCode::KeyD, ElementState::Released);
        assert_eq!(input.right_axis(), 0.0);
    }
}
