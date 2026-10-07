//! Modelo de **cubos de color** (sin atlas): mano en primera persona y el
//! personaje. Es un pipeline aparte del de escena (que usa el atlas de voxeles).
//!
//! Los cubos se **bajan a vertices** con el sombreado por cara ya horneado
//! (arriba mas claro, abajo mas oscuro): asi el shader es minimo y no necesita
//! normales. Cada pieza (brazo, cabeza, pierna...) es una [`ModelMesh`] propia
//! para poder transformarla por separado (animacion).

use bytemuck::{Pod, Zeroable};

use crate::math::Mat4;
use crate::scene::player::Cuboid;

/// Vertice del modelo: posicion + color (sRGB).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

impl ModelVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Sombreado por cara horneado (arriba claro, lados medios, abajo oscuro).
const FACE_SHADE: [f32; 6] = [0.84, 0.84, 1.0, 0.52, 0.70, 0.70];

/// Convierte una lista de cubos en vertices + indices. Las caras no se culling
/// (el pipeline usa `cull_mode: None`), asi que el orden de los vertices no
/// importa para la visibilidad.
pub fn cuboids_to_mesh(cuboids: &[Cuboid]) -> (Vec<ModelVertex>, Vec<u32>) {
    let mut verts = Vec::with_capacity(cuboids.len() * 24);
    let mut idx = Vec::with_capacity(cuboids.len() * 36);
    for c in cuboids {
        let [x0, y0, z0] = c.from;
        let [x1, y1, z1] = c.to;
        // Las 6 caras, cada una como 4 esquinas (orden indiferente: sin culling).
        let faces: [[[f32; 3]; 4]; 6] = [
            // +X
            [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
            // -X
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            // +Y
            [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
            // -Y
            [[x0, y0, z0], [x0, y0, z1], [x1, y0, z1], [x1, y0, z0]],
            // +Z
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            // -Z
            [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
        ];
        for (fi, face) in faces.iter().enumerate() {
            let s = FACE_SHADE[fi];
            let base = verts.len() as u32;
            for p in face {
                verts.push(ModelVertex {
                    pos: *p,
                    color: [c.color[0] * s, c.color[1] * s, c.color[2] * s],
                });
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    (verts, idx)
}

/// Una malla estatica de modelo subida a la GPU (capacidad reservada).
pub struct ModelMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl ModelMesh {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        cuboids: &[Cuboid],
    ) -> Self {
        let (verts, idx) = cuboids_to_mesh(cuboids);
        let vbytes = std::mem::size_of_val(verts.as_slice()) as u64;
        let ibytes = std::mem::size_of_val(idx.as_slice()) as u64;
        let vcap = vbytes.next_power_of_two().max(1);
        let icap = ibytes.next_power_of_two().max(1);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}.vertices")),
            size: vcap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}.indices")),
            size: icap,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if vbytes > 0 {
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));
        }
        if ibytes > 0 {
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&idx));
        }
        Self {
            vertex_buffer,
            index_buffer,
            index_count: idx.len() as u32,
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.index_count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelUniforms {
    mvp: [f32; 16],
    light: f32,
    _pad: [f32; 3],
}

/// Tamano de cada "slot" (pieza) en el buffer uniform, alineado a 256 (el minimo
/// de `min_uniform_buffer_offset_alignment` en todos los backends).
const SLOT: u64 = 256;
/// Maximo de piezas dibujadas por frame (mano + personaje).
pub const MAX_PARTS: u32 = 16;

/// Pipeline del modelo (color plano, con z-buffer, sin culling).
///
/// Usa **dynamic offsets** en el uniform: varias piezas por pase comparten un
/// solo buffer (cada una en su slot), porque `queue.write_buffer` se aplica
/// antes del pase y no por draw.
pub struct ModelPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl ModelPipeline {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("model.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model.bind_group.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<ModelUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model.uniforms"),
            size: SLOT * MAX_PARTS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model.bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                // Con dynamic offset hay que fijar el **tamano** de la vista del
                // binding (un slot); si no, el offset debe ser 0.
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(SLOT),
                }),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(ModelVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Sin culling: el modelo son pocos triangulos y asi ninguna cara
                // sale del reves por el orden de los vertices.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
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
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    /// Escribe la matriz y la luz de la pieza `slot` (0..[`MAX_PARTS`]).
    pub fn set(&self, queue: &wgpu::Queue, slot: u32, mvp: &Mat4, light: f32) {
        let u = ModelUniforms {
            mvp: mvp.to_cols_array(),
            light,
            _pad: [0.0; 3],
        };
        queue.write_buffer(
            &self.uniform_buffer,
            slot as u64 * SLOT,
            bytemuck::bytes_of(&u),
        );
    }

    /// Prepara el pipeline y dibuja la pieza `slot` (bind group con offset).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, slot: u32, mesh: &ModelMesh) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[slot * SLOT as u32]);
        mesh.draw(pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::player::Cuboid;

    #[test]
    fn un_cubo_da_24_vertices_y_36_indices() {
        let c = Cuboid {
            from: [0.0, 0.0, 0.0],
            to: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0],
        };
        let (v, i) = cuboids_to_mesh(&[c]);
        assert_eq!(v.len(), 24);
        assert_eq!(i.len(), 36);
    }

    #[test]
    fn el_sombreado_horneado_oscurece_la_base() {
        let c = Cuboid {
            from: [0.0, 0.0, 0.0],
            to: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0],
        };
        let (v, _) = cuboids_to_mesh(&[c]);
        // El vertice superior (+Y, indices 8..12) es mas claro que el inferior (-Y, 12..16).
        let top = v[8].color[0];
        let bottom = v[12].color[0];
        assert!(top > bottom, "top {top} <= bottom {bottom}");
    }
}
