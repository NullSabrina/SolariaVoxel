//! # Modulo `render` — el puente a la GPU
//!
//! Aqui envolveremos wgpu. El objetivo es que el resto del motor (el `engine`,
//! la escena, los chunks...) no sepa NADA de wgpu: solo habla con [`Renderer`].
//! Si algun dia cambiamos de API grafica, este es el unico modulo que cambia.
//!
//! Piezas del render:
//! * [`renderer`] — duena de los recursos de GPU y del bucle de un frame.
//! * [`pipeline`] — como se dibuja (shader, vertices, z-buffer, uniforms).
//! * [`mesh`] — la geometria (vertices + indices) subida a la GPU.
//!
//! En v0.1.2 el renderer ya dibuja un cubo 3D con z-buffer sobre el cielo.

mod color;
pub(crate) mod gui;
mod highlight;
// `mesh` es pub(crate) porque el mesher del mundo (`crate::world::mesh_chunk`)
// construye `Vertex`, el tipo de vertice que la GPU entiende.
pub(crate) mod mesh;
mod pipeline;
mod renderer;
mod ui;

pub use renderer::{Renderer, RendererError};
pub use ui::{UiQuad, region_uv};
