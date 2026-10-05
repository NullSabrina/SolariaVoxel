//! # Modulo `scene` — que hay en el mundo
//!
//! Aqui vivira todo lo que describe la escena: la camara, y mas adelante el
//! jugador, las entidades, los chunks cargados, etc. Hoy existe la [`Camera`]
//! (FPS: posicion, yaw/pitch, matrices) y el [`DayCycle`] (hora del mundo,
//! luz del sol y color del cielo).

mod camera;
mod daynight;

pub use camera::Camera;
pub use daynight::{DayCycle, NIGHT_FLOOR};
