//! Dibujo de la **interfaz 2D** (hotbar e inventario) en espacio de pantalla.
//!
//! Es un pipeline aparte del de la escena: los vertices ya vienen en NDC (la CPU
//! convierte de pixels), no hay z-buffer (la interfaz va siempre encima) y el
//! fragment muestrea la textura de interfaz ([`crate::render::gui`]) o el atlas
//! (para los iconos de bloque), segun la capa del vertice.

use bytemuck::{Pod, Zeroable};

use crate::render::gui;

/// Un vertice de interfaz: posicion NDC (x, y) + UV + capa. La capa `< 0` usa la
/// textura de interfaz; `>= 0` es el tile del atlas (icono de bloque).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub layer: i32,
}

impl UiVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Sint32];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Un rectangulo a dibujar, en pixels (origen arriba-izquierda).
///
/// `rect` = `[x, y, ancho, alto]`; `uv` = `[u0, v0, u1, v1]` normalizado;
/// `layer` = `-1` (interfaz) o el indice del tile del atlas (icono).
#[derive(Clone, Copy, Debug)]
pub struct UiQuad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub layer: i32,
}

/// Tope de vertices del buffer dinamico (se amplia si hiciera falta).
const INITIAL_CAPACITY: usize = 8192;

/// Pipeline y recursos de la interfaz.
pub struct UiPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    capacity: usize,
    vertex_count: u32,
}

impl UiPipeline {
    /// Crea el pipeline de interfaz. Recibe la vista del atlas y un sampler para
    /// reutilizarlos (mismo formato de textura).
    pub fn new(
        device: &wgpu::Device,
        atlas_view: &wgpu::TextureView,
        gui_view: &wgpu::TextureView,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui.bind_group.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui.bind_group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(gui_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(UiVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
                // La interfaz siempre gana: se dibuja encima de todo.
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui.vertices"),
            size: (INITIAL_CAPACITY * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group,
            vertex_buffer,
            capacity: INITIAL_CAPACITY,
            vertex_count: 0,
        }
    }

    /// Construye los vertices de los quads (pixels -> NDC) y los sube a la GPU.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[UiQuad],
        width: u32,
        height: u32,
    ) {
        let vertices = build_vertices(quads, width, height);
        self.vertex_count = vertices.len() as u32;
        if vertices.len() > self.capacity {
            self.capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui.vertices"),
                size: (self.capacity * std::mem::size_of::<UiVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
    }

    /// Dibuja los quads preparados en el pase actual.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// Convierte quads en pixels (origen arriba-izquierda) a vertices en NDC.
pub fn build_vertices(quads: &[UiQuad], width: u32, height: u32) -> Vec<UiVertex> {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let ndc_x = |x: f32| (x / w) * 2.0 - 1.0;
    let ndc_y = |y: f32| 1.0 - (y / h) * 2.0;
    let mut out = Vec::with_capacity(quads.len() * 6);
    for q in quads {
        let [x, y, qw, qh] = q.rect;
        let [u0, v0, u1, v1] = q.uv;
        let (x0, x1) = (ndc_x(x), ndc_x(x + qw));
        let (y0, y1) = (ndc_y(y), ndc_y(y + qh));
        let corners = [
            ([x0, y0], [u0, v0]),
            ([x1, y0], [u1, v0]),
            ([x1, y1], [u1, v1]),
            ([x0, y1], [u0, v1]),
        ];
        for idx in [0usize, 1, 2, 0, 2, 3] {
            out.push(UiVertex {
                pos: corners[idx].0,
                uv: corners[idx].1,
                layer: q.layer,
            });
        }
    }
    out
}

/// UV normalizada de una region de la textura de interfaz.
pub fn region_uv(r: gui::Region) -> [f32; 4] {
    [
        r.x as f32 / gui::GUI_W as f32,
        r.y as f32 / gui::GUI_H as f32,
        (r.x + r.w) as f32 / gui::GUI_W as f32,
        (r.y + r.h) as f32 / gui::GUI_H as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_quad_da_seis_vertices_en_ndc() {
        let quad = UiQuad {
            rect: [0.0, 0.0, 100.0, 100.0],
            uv: [0.0, 0.0, 1.0, 1.0],
            layer: -1,
        };
        let v = build_vertices(&[quad], 200, 100);
        assert_eq!(v.len(), 6);
        // Esquina superior izquierda -> NDC (-1, 1).
        assert!((v[0].pos[0] + 1.0).abs() < 1e-6);
        assert!((v[0].pos[1] - 1.0).abs() < 1e-6);
        assert_eq!(v[0].layer, -1);
    }
}
