//! Pipeline de resaltado del bloque apuntado.
//!
//! Dibuja la caja del bloque seleccionado como una **rejilla de lineas** (wireframe).
//! Reutiliza el tipo [`Vertex`] (posicion + uv), pero este pipeline no texturiza:
//! el shader `highlight.wgsl` pinta cada vertice de un color plano (naranja).
//!
//! Se dibuja un pelin inflada (0.01) para que las lineas queden por fuera del
//! bloque y no parpadeen (z-fighting) con las caras de la geometria.

use crate::render::mesh::Vertex;

/// Construye los 8 vertices y 12 aristas (24 indices) de una caja de lado 1,
/// centrada en `[cx, cy, cz]` (que suele ser el centro del bloque).
pub fn cube_edges(cx: f32, cy: f32, cz: f32, inflate: f32) -> ([Vertex; 8], [u32; 24]) {
    let h = 0.5 + inflate;
    // Esquinas: combinacion de ±h en cada eje.
    let corners = [
        [cx - h, cy - h, cz - h], // 0
        [cx + h, cy - h, cz - h], // 1
        [cx + h, cy + h, cz - h], // 2
        [cx - h, cy + h, cz - h], // 3
        [cx - h, cy - h, cz + h], // 4
        [cx + h, cy - h, cz + h], // 5
        [cx + h, cy + h, cz + h], // 6
        [cx - h, cy + h, cz + h], // 7
    ];
    // UV sin usar, a cero.
    let vertices = corners.map(|p| Vertex::new(p, [0.0, 0.0]));
    // 12 aristas (pares de esquinas).
    let indices = [
        0, 1, 1, 2, 2, 3, 3, 0, // cara -Z
        4, 5, 5, 6, 6, 7, 7, 4, // cara +Z
        0, 4, 1, 5, 2, 6, 3, 7, // aristas que unen ambas caras
    ];
    (vertices, indices)
}

/// Pipeline y recursos para dibujar el resaltado.
pub struct HighlightPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl HighlightPipeline {
    /// Crea el pipeline de resaltado (lineas, color plano, sin textura).
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("highlight.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("highlight.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("highlight.pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                // Lineas en lugar de triangulos: asi dibujamos las aristas.
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
                // `LessEqual` (y no `Less`) para que las lineas, que van justo
                // sobre la superficie del bloque, no se descarten por empate.
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                // Nota: el *depth bias* no es compatible con topologia `LineList`,
                // asi que no lo usamos; la caja se infla un pelin en CPU.
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline }
    }

    /// El pipeline (para `pass.set_pipeline`).
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}
