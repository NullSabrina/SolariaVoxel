//! Configuracion de la ventana.
//!
//! Separamos la "receta" de la ventana (sus atributos) de su creacion real, que
//! ocurre en [`crate::engine::App::resumed`]. Asi podemos tener los valores en
//! constantes y, mas adelante, cambiarlos desde un fichero de opciones.

use winit::dpi::LogicalSize;
use winit::window::WindowAttributes;

/// Ancho inicial de la ventana, en pixels logicos.
pub const INITIAL_WIDTH: u32 = 1280;

/// Alto inicial de la ventana, en pixels logicos.
pub const INITIAL_HEIGHT: u32 = 720;

/// Nombre que aparece en la barra de titulo.
pub const TITLE: &str = "Solaria Voxel — v0.14.0";

/// Devuelve los atributos con los que se creara la ventana.
pub fn default_window_attributes() -> WindowAttributes {
    WindowAttributes::default()
        .with_title(TITLE)
        // Tamano logico: es independiente del factor de escala (DPI) del monitor.
        .with_inner_size(LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT))
        // Evitamos que el usuario encoja la ventana hasta algo inclickable.
        .with_min_inner_size(LogicalSize::new(320, 200))
}
