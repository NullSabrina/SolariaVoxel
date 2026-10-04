//! # Modulo `scene` — que hay en el mundo
//!
//! Aqui vivira todo lo que describe la escena: la camara, y mas adelante el
//! jugador, las entidades, los chunks cargados, etc. En v0.1.0 solo existe la
//! [`Camera`], que de momento es estatica (no la movemos), pero ya calcula sus
//! matrices de vista y proyeccion para que en versiones siguientes solo haya
//! que anadir el input.

mod camera;

pub use camera::Camera;
