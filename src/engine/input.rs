//! Estado del input del usuario (teclado y raton).
//!
//! winit nos entrega eventos *puntuales* ("se pulso W", "el raton se movio 3
//! pixels"), pero la camara necesita el estado *continuo* ("¿esta W pulsada
//! ahora mismo?"). Este modulo guarda ese estado y lo expone en forma de:
//!
//! * [`Input::is_pressed`] — ¿esta pulsada esta tecla?
//! * [`Input::axis_with`] — eje de movimiento `[-1, 1]` entre dos teclas (para
//!   teclas **reasignables**), listo para [`crate::scene::Camera::walk`].
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

    /// Eje `-1|0|1` entre dos teclas (para teclas reasignables).
    #[inline]
    pub fn axis_with(&self, positive: KeyCode, negative: KeyCode) -> f32 {
        axis(self.is_pressed(positive), self.is_pressed(negative))
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
}

/// Convierte dos booleanos (positivo/negativo) en un eje `-1 | 0 | 1`.
#[inline]
fn axis(positive: bool, negative: bool) -> f32 {
    (positive as i32 - negative as i32) as f32
}

/// Teclas reasignables: nombre estable <-> `KeyCode`. Genera las dos funciones
/// de conversion para un conjunto curado (letras, digitos y modificadores).
macro_rules! bindable_keys {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Nombre estable de una tecla reasignable (o `None` si no lo es).
        pub fn key_name(code: KeyCode) -> Option<&'static str> {
            match code {
                $(KeyCode::$variant => Some($name),)*
                _ => None,
            }
        }

        /// `KeyCode` a partir de su nombre estable (o `None`).
        pub fn parse_key(name: &str) -> Option<KeyCode> {
            match name {
                $($name => Some(KeyCode::$variant),)*
                _ => None,
            }
        }
    };
}

bindable_keys! {
    KeyA => "KeyA", KeyB => "KeyB", KeyC => "KeyC", KeyD => "KeyD",
    KeyE => "KeyE", KeyF => "KeyF", KeyG => "KeyG", KeyH => "KeyH",
    KeyI => "KeyI", KeyJ => "KeyJ", KeyK => "KeyK", KeyL => "KeyL",
    KeyM => "KeyM", KeyN => "KeyN", KeyO => "KeyO", KeyP => "KeyP",
    KeyQ => "KeyQ", KeyR => "KeyR", KeyS => "KeyS", KeyT => "KeyT",
    KeyU => "KeyU", KeyV => "KeyV", KeyW => "KeyW", KeyX => "KeyX",
    KeyY => "KeyY", KeyZ => "KeyZ",
    Digit0 => "Digit0", Digit1 => "Digit1", Digit2 => "Digit2", Digit3 => "Digit3",
    Digit4 => "Digit4", Digit5 => "Digit5", Digit6 => "Digit6", Digit7 => "Digit7",
    Digit8 => "Digit8", Digit9 => "Digit9",
    Space => "Space",
    ShiftLeft => "ShiftLeft", ShiftRight => "ShiftRight",
    ControlLeft => "ControlLeft", ControlRight => "ControlRight",
    ArrowUp => "ArrowUp", ArrowDown => "ArrowDown", ArrowLeft => "ArrowLeft",
    ArrowRight => "ArrowRight",
}

/// Nombre corto para mostrar una tecla ("KeyW" -> "W", "Space" -> "Espacio").
pub fn key_display(name: &str) -> String {
    match name {
        "Space" => "Espacio".to_string(),
        "ShiftLeft" | "ShiftRight" => "Shift".to_string(),
        "ControlLeft" | "ControlRight" => "Ctrl".to_string(),
        "ArrowUp" => "Flecha arriba".to_string(),
        "ArrowDown" => "Flecha abajo".to_string(),
        "ArrowLeft" => "Flecha izq".to_string(),
        "ArrowRight" => "Flecha der".to_string(),
        n if n.starts_with("Key") => n[3..].to_string(),
        n if n.starts_with("Digit") => n[5..].to_string(),
        n => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eje_adelante_combina_w_y_s() {
        let mut input = Input::default();
        assert_eq!(input.axis_with(KeyCode::KeyW, KeyCode::KeyS), 0.0);
        input.on_key(KeyCode::KeyW, ElementState::Pressed);
        assert_eq!(input.axis_with(KeyCode::KeyW, KeyCode::KeyS), 1.0);
        input.on_key(KeyCode::KeyS, ElementState::Pressed);
        // W y S a la vez se cancelan.
        assert_eq!(input.axis_with(KeyCode::KeyW, KeyCode::KeyS), 0.0);
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
        assert_eq!(input.axis_with(KeyCode::KeyD, KeyCode::KeyA), 1.0);
        input.on_key(KeyCode::KeyD, ElementState::Released);
        assert_eq!(input.axis_with(KeyCode::KeyD, KeyCode::KeyA), 0.0);
    }

    #[test]
    fn las_teclas_bindables_van_y_vuelven() {
        assert_eq!(key_name(KeyCode::KeyW), Some("KeyW"));
        assert_eq!(parse_key("KeyW"), Some(KeyCode::KeyW));
        assert_eq!(key_name(KeyCode::Space), Some("Space"));
        assert_eq!(parse_key("Space"), Some(KeyCode::Space));
        assert_eq!(parse_key("no.existe"), None);
        assert_eq!(key_display("KeyW"), "W");
        assert_eq!(key_display("Digit3"), "3");
        assert_eq!(key_display("Space"), "Espacio");
    }
}
