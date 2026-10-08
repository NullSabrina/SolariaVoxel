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
//! * [`sky`] — el pase de cielo (gradiente cenit <-> horizonte).
//!
//! En v0.1.2 el renderer ya dibuja un cubo 3D con z-buffer sobre el cielo.

pub(crate) mod font;
pub(crate) mod gui;
mod highlight;
// `mesh` es pub(crate) porque el mesher del mundo (`crate::world::mesh_chunk`)
// construye `Vertex`, el tipo de vertice que la GPU entiende.
pub(crate) mod mesh;
mod mesh_worker;
pub(crate) mod model;
mod pipeline;
mod renderer;
mod sky;
mod ui;

pub use renderer::{CharacterView, FrameStats, HandView, Renderer, RendererError};
pub use sky::SkyBasis;
pub use ui::{UiQuad, region_uv};

#[cfg(test)]
mod shader_tests {
    /// Compila (parsea) todos los shaders WGSL del motor sin GPU: naga valida
    /// sintaxis y tipos, asi un error de shader se caza en `cargo test` y no al
    /// arrancar la ventana.
    #[test]
    fn los_shaders_wgsl_compilan() {
        for (name, src) in [
            ("scene", include_str!("scene.wgsl")),
            ("water", include_str!("shaders/water.wgsl")),
            ("sky", include_str!("sky.wgsl")),
            ("model", include_str!("model.wgsl")),
            ("ui", include_str!("ui.wgsl")),
            ("highlight", include_str!("highlight.wgsl")),
        ] {
            let parsed = wgpu::naga::front::wgsl::parse_str(src);
            assert!(parsed.is_ok(), "shader {name} no compila: {:?}", parsed.err());
        }
    }
}
