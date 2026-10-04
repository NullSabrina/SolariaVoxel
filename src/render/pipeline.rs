//! El pipeline grafico: la "configuracion" de como se dibuja.
//!
//! Un `RenderPipeline` de wgpu reune: el shader, el formato de los vertices, el
//! tipo de primitivas (triangulos), el estado de profundidad (z-buffer) y el
//! formato del color de salida. Tambien creamos aqui:
//!
//! * el *uniform buffer* con la matriz MVP,
//! * la textura del *atlas* de bloques y su sampler,
//! * el *bind group* que agrupa todo eso para el shader.

use bytemuck::{Pod, Zeroable};

use crate::math::Mat4;
use crate::render::mesh::Vertex;
use crate::world::atlas;

/// Datos que la CPU envia a la GPU cada frame. El layout DEBE coincidir con el
/// `struct Uniforms` del shader `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    /// Matriz modelo * vista * proyeccion, aplanada en 16 floats.
    mvp: [f32; 16],
}

/// Pipeline de dibujo de la escena (voxeles texturizados con el atlas).
pub struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    // Mantenemos vivos estos recursos: aunque el bind group ya los referencia,
    // guardarlos deja claro quien es el dueno. El prefijo `_` evita avisos de
    // "campo nunca leido".
    _atlas_texture: wgpu::Texture,
    _atlas_view: wgpu::TextureView,
    _sampler: wgpu::Sampler,
}

impl ScenePipeline {
    /// Crea el pipeline para un color target de formato `color_format` y un
    /// z-buffer de formato `depth_format`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // 1. El modulo de shader, cargado en tiempo de compilacion del binario.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });

        // 2. La textura del atlas (generada por codigo) y su sampler. El sampler
        //    es "nearest" para que la pixel-art se vea nitida y no borrosa.
        let (atlas_texture, atlas_view) = Self::create_atlas_texture(device, queue);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene.atlas.sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // 3. El layout del bind group: uniform + textura + sampler.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene.bind_group.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniforms>() as u64
                        ),
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

        // 4. El buffer uniform (64 bytes) y su bind group.
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene.uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene.bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // 5. El layout del pipeline (que bind groups usa).
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene.pipeline.layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // 6. El pipeline en si.
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
                // Seguimos sin descartar caras traseras; el z-buffer basta.
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
            _atlas_texture: atlas_texture,
            _atlas_view: atlas_view,
            _sampler: sampler,
        }
    }

    /// Crea la textura del atlas a partir de los pixels generados en
    /// [`crate::world::atlas::build_pixels`] y los sube a la GPU.
    fn create_atlas_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: atlas::WIDTH,
            height: atlas::HEIGHT,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene.atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let pixels = atlas::build_pixels();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas::WIDTH * 4),
                rows_per_image: Some(atlas::HEIGHT),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
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

    /// Acceso al bind group (para `pass.set_bind_group`).
    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
