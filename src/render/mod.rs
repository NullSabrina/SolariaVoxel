//! # Modulo `render` — el puente a la GPU
//!
//! Aqui envolveremos wgpu. El objetivo es que el resto del motor (el `engine`,
//! la escena, los chunks...) no sepa NADA de wgpu: solo habla con [`Renderer`].
//! Si algun dia cambiamos de API grafica, este es el unico modulo que cambia.
//!
//! En v0.1.0 el renderer solo sabe hacer una cosa: adquirir el frame, limpiar
//! la pantalla a un color (el "cielo") y presentarlo. En v0.1.2 anadira el
//! pipeline, los buffers y el dibujado de geometria.

mod renderer;

pub use renderer::{Renderer, RendererError};
