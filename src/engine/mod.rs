//! # Modulo `engine` — el corazon del motor
//!
//! Aqui vive el ciclo de vida de la aplicacion: crear la ventana, arrancar el
//! renderer y repartir los eventos del sistema operativo. Es la capa que "pega"
//! todos los demas modulos ([`crate::render`], [`crate::scene`],
//! [`crate::math`]).

mod app;
mod window;

pub use app::{App, run};
