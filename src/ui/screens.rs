//! Maquina de estados de **pantallas** (pila): solo la superior recibe input.
//!
//! Logica pura y testeable; `engine::app` la consume y `render` solo dibuja. La
//! pila nunca se vacia: siempre queda al menos `Title` o `Playing`.

/// Pantalla de la interfaz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Pantalla de titulo (logo + botones).
    Title,
    /// Selector de mundos.
    WorldSelect,
    /// Crear mundo (nombre + semilla).
    CreateWorld,
    /// Opciones (video, juego, teclas).
    Options,
    /// Controles (reasignar teclas).
    Controls,
    /// Jugando (mundo + HUD).
    Playing,
    /// Menu de pausa.
    Pause,
}

/// Pila de pantallas. `top` es la unica que recibe input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenStack {
    stack: Vec<Screen>,
}

impl Default for ScreenStack {
    fn default() -> Self {
        Self::with_title()
    }
}

impl ScreenStack {
    /// Pila empezando en la pantalla de titulo.
    pub fn with_title() -> Self {
        Self {
            stack: vec![Screen::Title],
        }
    }

    /// Pila empezando a jugar (modo demo).
    pub fn with_playing() -> Self {
        Self {
            stack: vec![Screen::Playing],
        }
    }

    /// Pantalla superior.
    pub fn top(&self) -> Screen {
        *self.stack.last().unwrap_or(&Screen::Playing)
    }

    /// ¿Estamos jugando (nada encima)?
    pub fn is_playing(&self) -> bool {
        self.top() == Screen::Playing
    }

    /// Apila una pantalla.
    pub fn push(&mut self, screen: Screen) {
        self.stack.push(screen);
    }

    /// Desapila; nunca deja la pila vacia (conserva la base).
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Sustituye la cima (p.ej. Title -> WorldSelect).
    pub fn replace(&mut self, screen: Screen) {
        if self.stack.is_empty() {
            self.stack.push(screen);
        } else {
            *self.stack.last_mut().unwrap() = screen;
        }
    }

    /// Vuelve a `Title` limpiando todo.
    pub fn to_title(&mut self) {
        self.stack.truncate(1);
        if self.stack.is_empty() {
            self.stack.push(Screen::Title);
        } else {
            self.stack[0] = Screen::Title;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_pila_nunca_se_vacia() {
        let mut s = ScreenStack::with_title();
        assert_eq!(s.top(), Screen::Title);
        s.pop();
        assert_eq!(s.top(), Screen::Title, "no se desapila la base");
        s.push(Screen::WorldSelect);
        assert_eq!(s.top(), Screen::WorldSelect);
        s.pop();
        assert_eq!(s.top(), Screen::Title);
    }

    #[test]
    fn pausa_se_apila_sobre_playing_y_se_quita() {
        let mut s = ScreenStack::with_playing();
        assert!(s.is_playing());
        s.push(Screen::Pause);
        assert!(!s.is_playing());
        s.pop();
        assert!(s.is_playing());
    }

    #[test]
    fn to_title_limpia_todo() {
        let mut s = ScreenStack::with_playing();
        s.push(Screen::Pause);
        s.to_title();
        assert_eq!(s.top(), Screen::Title);
    }
}
