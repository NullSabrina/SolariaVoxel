//! # Modulo `scene` — que hay en el mundo
//!
//! Aqui vivira todo lo que describe la escena: la camara, y mas adelante el
//! jugador, las entidades, los chunks cargados, etc. Hoy existe la [`Camera`]
//! (FPS: posicion, yaw/pitch, matrices), el [`DayCycle`] (hora del mundo) y el
//! [`SkyState`] (cielo y atmosfera derivados de la hora; unica fuente de verdad).

mod camera;
mod daynight;
pub mod player;
mod sky;

pub use camera::Camera;
pub use daynight::DayCycle;
pub use sky::{SKY_EXPONENT, SkyParams, SkyState, sun_direction};
