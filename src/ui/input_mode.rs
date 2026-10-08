//! Maquina de estados del **modo de input**: que capa recibe input y si el
//! cursor debe estar capturado. Logica **pura** (sin GPU, sin winit): devuelve
//! [`Effect`]s que `engine::app` aplica. Corrige el "doble Esc" y el cierre del
//! inventario sin recapturar el cursor (MEGA PROMPT 3, Fase B).
//!
//! Reglas (seccion 3.1 del prompt): un **solo** Esc por capa; cerrar cualquier
//! overlay **recaptura** el cursor sin click; Esc en menus vuelve atras.

/// Overlay de juego que puede estar abierto sobre `Playing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    Inventory,
    Crafting,
    Pause,
}

/// Capa de input activa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Jugando: el cursor esta capturado y el mundo recibe input.
    Playing,
    /// Un overlay de juego (inventario, mesa, pausa) esta encima.
    Overlay(Overlay),
    /// Menus (titulo, mundos, opciones, controles...).
    Menu,
}

/// Efecto que `engine::app` aplica (aqui no se toca winit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Pedir la captura del cursor (reintentando si el SO la rechaza).
    CaptureCursor,
    /// Liberar el cursor.
    ReleaseCursor,
    /// Apilar la pantalla de pausa.
    OpenPause,
    /// Desapilar la pantalla de pausa.
    ClosePause,
    /// Abrir el inventario.
    OpenInventory,
    /// Cerrar el inventario.
    CloseInventory,
    /// Cerrar la mesa de crafteo.
    CloseCrafting,
    /// Volver atras en los menus.
    MenuBack,
}

/// Estado del modo de input. `Default` = `Menu` (la app arranca en el titulo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputMode {
    mode: Mode,
}

impl Default for InputMode {
    fn default() -> Self {
        Self { mode: Mode::Menu }
    }
}

impl InputMode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Modo actual.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Sincroniza el modo con el estado real de la app (pantallas y flags). La
    /// app lo llama antes de decidir una transicion, para no duplicar estado.
    pub fn set(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Esc: devuelve los efectos a aplicar y actualiza el modo.
    pub fn on_escape(&mut self) -> Vec<Effect> {
        match self.mode {
            // Un **solo** Esc abre la pausa y libera el cursor.
            Mode::Playing => {
                self.mode = Mode::Overlay(Overlay::Pause);
                vec![Effect::OpenPause, Effect::ReleaseCursor]
            }
            Mode::Overlay(Overlay::Inventory) => {
                self.mode = Mode::Playing;
                vec![Effect::CloseInventory, Effect::CaptureCursor]
            }
            Mode::Overlay(Overlay::Crafting) => {
                self.mode = Mode::Playing;
                vec![Effect::CloseCrafting, Effect::CaptureCursor]
            }
            Mode::Overlay(Overlay::Pause) => {
                self.mode = Mode::Playing;
                vec![Effect::ClosePause, Effect::CaptureCursor]
            }
            Mode::Menu => vec![Effect::MenuBack],
        }
    }

    /// Tecla de inventario (E): abre/cierra el inventario. En pausa o menus no
    /// hace nada.
    pub fn on_inventory_key(&mut self) -> Vec<Effect> {
        match self.mode {
            Mode::Playing => {
                self.mode = Mode::Overlay(Overlay::Inventory);
                vec![Effect::OpenInventory, Effect::ReleaseCursor]
            }
            Mode::Overlay(Overlay::Inventory) => {
                self.mode = Mode::Playing;
                vec![Effect::CloseInventory, Effect::CaptureCursor]
            }
            Mode::Overlay(Overlay::Crafting) => {
                self.mode = Mode::Playing;
                vec![Effect::CloseCrafting, Effect::CaptureCursor]
            }
            Mode::Overlay(Overlay::Pause) | Mode::Menu => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> InputMode {
        let mut m = InputMode::new();
        m.set(Mode::Playing);
        m
    }

    #[test]
    fn esc_en_juego_abre_pausa_y_libera_cursor_en_un_solo_paso() {
        let mut m = playing();
        let fx = m.on_escape();
        assert_eq!(m.mode(), Mode::Overlay(Overlay::Pause));
        assert!(fx.contains(&Effect::OpenPause));
        assert!(fx.contains(&Effect::ReleaseCursor));
        // Un solo Esc: no queda "primero liberar, luego abrir".
    }

    #[test]
    fn esc_en_inventario_cierra_y_recaptura_sin_click() {
        let mut m = InputMode::new();
        m.set(Mode::Overlay(Overlay::Inventory));
        let fx = m.on_escape();
        assert_eq!(m.mode(), Mode::Playing);
        assert!(fx.contains(&Effect::CloseInventory));
        assert!(fx.contains(&Effect::CaptureCursor));
    }

    #[test]
    fn e_en_inventario_cierra_y_recaptura_sin_click() {
        let mut m = InputMode::new();
        m.set(Mode::Overlay(Overlay::Inventory));
        let fx = m.on_inventory_key();
        assert_eq!(m.mode(), Mode::Playing);
        assert!(fx.contains(&Effect::CloseInventory));
        assert!(fx.contains(&Effect::CaptureCursor));
    }

    #[test]
    fn e_en_juego_abre_inventario_y_libera_cursor() {
        let mut m = playing();
        let fx = m.on_inventory_key();
        assert_eq!(m.mode(), Mode::Overlay(Overlay::Inventory));
        assert!(fx.contains(&Effect::OpenInventory));
        assert!(fx.contains(&Effect::ReleaseCursor));
    }

    #[test]
    fn pausa_volver_recaptura_cursor_y_vuelve_a_playing() {
        let mut m = InputMode::new();
        m.set(Mode::Overlay(Overlay::Pause));
        let fx = m.on_escape();
        assert_eq!(m.mode(), Mode::Playing);
        assert!(fx.contains(&Effect::ClosePause));
        assert!(fx.contains(&Effect::CaptureCursor));
    }

    #[test]
    fn esc_en_mesa_cierra_y_recaptura() {
        let mut m = InputMode::new();
        m.set(Mode::Overlay(Overlay::Crafting));
        let fx = m.on_escape();
        assert_eq!(m.mode(), Mode::Playing);
        assert!(fx.contains(&Effect::CloseCrafting));
        assert!(fx.contains(&Effect::CaptureCursor));
    }

    #[test]
    fn esc_en_menu_vuelve_atras() {
        let mut m = InputMode::new();
        m.set(Mode::Menu);
        assert_eq!(m.on_escape(), vec![Effect::MenuBack]);
        assert_eq!(m.mode(), Mode::Menu);
    }

    #[test]
    fn e_en_pausa_o_menu_no_hace_nada() {
        let mut m = InputMode::new();
        m.set(Mode::Overlay(Overlay::Pause));
        assert!(m.on_inventory_key().is_empty());
        assert_eq!(m.mode(), Mode::Overlay(Overlay::Pause));
        m.set(Mode::Menu);
        assert!(m.on_inventory_key().is_empty());
    }
}
