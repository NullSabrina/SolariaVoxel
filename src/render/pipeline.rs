//! El pipeline grafico: la "configuracion" de como se dibuja.
//!
//! Un `RenderPipeline` de wgpu reune: el shader, el formato de los vertices, el
//! tipo de primitivas (triangulos), el estado de profundidad (z-buffer) y el
//! formato del color de salida. Tambien creamos aqui el *uniform buffer* donde
//! subimos la matriz MVP cada frame.

use bytemuck::{Pod, Zeroable};

use crate::math::Mat4;
use crate::render::mesh::Vertex;

/// Datos que la CPU envia a la GPU cada frame. El layout DEBE coincidir con el
/// `struct Uniforms` del shader `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    /// Matriz modelo * vista * proyeccion, aplanada en 16 floats.
    mvp: [f32; 16],
}

/// Pipeline de dibujo de la escena (un solo shader, color por vertice).
pub struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl ScenePipeline {
    /// Crea el pipeline para un color target de formato `color_format` y un
    /// z-buffer de formato `depth_format`.
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // 1. El modulo de shader, cargado en tiempo de compilacion del binario.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });

        // 2. El layout del bind group: un unico uniform buffer visible al vertex.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.uniforms.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        // 3. El buffer uniform (64 bytes) y su bind group.
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene.uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene.uniforms.bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // 4. El layout del pipeline (que bind groups usa).
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene.pipeline.layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // 5. El pipeline en si.
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                // De momento NO descartamos caras traseras: el z-buffer ya
                // resuelve la oclusion y asi evitamos depender del orden de
                // los vertices. Lo activaremos al hacer meshing de chunks.
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

    /// Sube la matriz MVP al uniform buffer.
    pub fn update_mvp(&self, queue: &wgpu::Queue, mvp: &Mat4) {
        let uniforms = Uniforms {
            mvp: mvp.to_cols_array(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Acceso al pipeline (para `pass.set_pipeline`).
    #[inline]
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// Acceso al bind group de uniforms (para `pass.set_bind_group`).
    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
