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
///
/// Cuidado con la alineacion: en WGSL un `vec3<f32>` exige offset multiple de 16
/// (aunque ocupe 12), por eso el orden y el relleno estan pensados para que Rust
/// y WGSL coincidan byte a byte.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    /// Matriz modelo * vista * proyeccion, aplanada en 16 floats.
    mvp: [f32; 16],
    /// Posicion de la camara (para la niebla por distancia).
    camera_pos: [f32; 3],
    /// Factor dia/noche (0..1) que multiplica la **luz de cielo**. La luz de
    /// bloque (antorchas) no se ve afectada.
    day_factor: f32,
    /// Color del cielo (lineal) al que se funde la niebla.
    fog_color: [f32; 3],
    /// Distancia a la que empieza la niebla.
    fog_start: f32,
    /// Distancia a la que la niebla es total.
    fog_end: f32,
    /// Tiempo (s) para animar el agua. Ocupa el primer `pad` del shader de
    /// escena; asi el uniform sigue midiendo 112 bytes.
    time: f32,
    /// Relleno para que el uniform mida un multiplo de 16 bytes.
    _pad: [f32; 2],
}

/// Pipeline de dibujo de la escena (voxeles texturizados con el atlas).
pub struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    /// Variante para el **agua**: mismo shader/layout/bind group, pero con
    /// blending alfa y sin escritura de z (translucido).
    water_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    // Mantenemos vivos estos recursos: aunque el bind group ya los referencia,
    // guardarlos deja claro quien es el dueno. El prefijo `_` evita avisos de
    // "campo nunca leido".
    _atlas_texture: wgpu::Texture,
    _atlas_view: wgpu::TextureView,
    _sampler: wgpu::Sampler,

    /// Layout del pipeline, para que otros pipelines (el resaltado) compartan el
    /// mismo bind group (uniform con la mvp).
    layout: wgpu::PipelineLayout,
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
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
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
                    // La matriz la usa el vertex shader; `day_factor`, el
                    // fragment. Por eso el uniform es visible en ambas etapas.
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
                // Descartamos caras traseras: la geometria esta orientada hacia
                // fuera, asi que solo se rasteriza lo visible (menos trabajo de
                // fragmento). Las antorchas emiten sus dos orientaciones.
                cull_mode: Some(wgpu::Face::Back),
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

        // 7. Variante de **agua**: shader propio (`water.wgsl`, con UVs animadas
        //    y especular), sin culling (se ve desde dentro) y sin escritura de z.
        let water_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/water.wgsl").into()),
        });
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene.water.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &water_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &water_shader,
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

        Self {
            pipeline,
            water_pipeline,
            uniform_buffer,
            bind_group,
            _atlas_texture: atlas_texture,
            _atlas_view: atlas_view,
            _sampler: sampler,
            layout: pipeline_layout,
        }
    }

    /// Crea el array de texturas (una capa de 16x16 por tile) a partir del atlas
    /// y lo sube a la GPU. Cada capa es un tile aislado, asi que las UVs pueden
    /// repetirse por bloque sin sangrado entre tiles.
    fn create_atlas_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        use crate::world::atlas::{TILE, TILES};
        let size = wgpu::Extent3d {
            width: TILE,
            height: TILE,
            depth_or_array_layers: TILES,
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

        let tiles = atlas::split_tiles(&atlas::load_pixels());
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tiles,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE * 4),
                rows_per_image: Some(TILE),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        (texture, view)
    }

    /// Sube la matriz MVP, el factor dia/noche y los parametros de niebla.
    #[allow(clippy::too_many_arguments)]
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        mvp: &Mat4,
        camera_pos: [f32; 3],
        day_factor: f32,
        fog_color: [f32; 3],
        fog_start: f32,
        fog_end: f32,
        time: f32,
    ) {
        let uniforms = Uniforms {
            mvp: mvp.to_cols_array(),
            camera_pos,
            day_factor,
            fog_color,
            fog_start,
            fog_end,
            time,
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Acceso al pipeline (para `pass.set_pipeline`).
    #[inline]
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    /// Pipeline del agua (blending, sin escritura de z).
    #[inline]
    pub fn water_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.water_pipeline
    }

    /// Acceso al bind group (para `pass.set_bind_group`).
    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Layout del pipeline, compartido con el pipeline de resaltado.
    #[inline]
    pub fn layout(&self) -> &wgpu::PipelineLayout {
        &self.layout
    }

    /// Vista del atlas (la comparte el pipeline de interfaz para los iconos).
    #[inline]
    pub fn atlas_view(&self) -> &wgpu::TextureView {
        &self._atlas_view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_uniform_mide_112_bytes() {
        // El shader (scene.wgsl) asume esta medida exacta; si cambia el layout
        // hay que actualizar alli tambien.
        assert_eq!(std::mem::size_of::<Uniforms>(), 112);
    }
}
