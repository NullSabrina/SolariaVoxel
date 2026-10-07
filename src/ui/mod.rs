//! # Modulo `ui` — estado y logica de interfaz (sin GPU)
//!
//! La interfaz tiene dos mitades separadas a proposito:
//!
//! * **`ui`** (este modulo): el **estado y la logica** — idioma, nombres de
//!   bloque, busqueda del inventario, semantica de clicks. Es puro y 100 %
//!   testeable sin GPU.
//! * **[`crate::render::ui`]**: solo **dibuja** lo que este modulo describe.
//!
//! Regla: nada de `wgpu` aqui, y `render` no toma decisiones de interfaz.

pub mod inventory;
pub mod lang;

pub use lang::{Lang, translate};
