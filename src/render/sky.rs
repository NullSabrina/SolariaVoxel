//! Pase de **cielo**: un triangulo a pantalla completa que sustituye el color de
//! fondo plano por un gradiente cenit <-> horizonte dependiente de la direccion.
//!
//! Los colores llegan **resueltos desde la CPU** ([`crate::scene::SkyState`]
//! convertido a lineal); el shader solo reconstruye el rayo de vista y aplica la
//! forma vertical. Asi no se duplican tablas de keyframes en WGSL.

use bytemuck::{Pod, Zeroable};

use crate::math::Vec3;
use crate::scene::SkyState;

/// Base de la camara para reconstruir el rayo de vista en el shader.
#[derive(Debug, Clone, Copy)]
pub struct SkyBasis {
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// `tan(fov_y / 2)`, en radianes.
    pub tan_half_fov_y: f32,
    /// Ancho / alto de la superficie.
    pub aspect: f32,
}

/// Radio angular del sol, en radianes (~6.3 grados de radio).
pub const SUN_ANG_RADIUS: f32 = 0.11;
/// Radio angular de la luna, en radianes.
pub const MOON_ANG_RADIUS: f32 = 0.085;

/// Uniform del pase de cielo. DEBE coincidir con `SkyUniforms` de `sky.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniforms {
    forward: [f32; 3],
    tan_half_fov_y: f32,
    right: [f32; 3],
    aspect: f32,
    up: [f32; 3],
    haze: f32,
    zenith: [f32; 3],
    exponent: f32,
    horizon_sun: [f32; 3],
    sun_glow: f32,
    horizon_anti: [f32; 3],
    belt: f32,
    sun_dir: [f32; 3],
    time: f32,
    sun_color: [f32; 3],
    _pad: f32,
    moon_dir: [f32; 3],
    moon_phase: f32,
    sun_ang_radius: f32,
    moon_ang_radius: f32,
    star_vis: f32,
    self_spin: f32,
}

/// Recursos de GPU del pase de cielo.
pub struct SkyPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl SkyPipeline {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky.bind_group.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<SkyUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky.uniforms"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky.bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky.pipeline.layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            // No escribe ni prueba profundidad (el mundo lo tapa despues). Si se
            // probara con `Less`, el cielo a z=0 ganaria a todo; con `Always` y sin
            // escritura siempre se dibuja y no estorba.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
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

    /// Sube los colores resueltos de `state` mas la base de camara.
    pub fn update(&self, queue: &wgpu::Queue, state: &SkyState, basis: &SkyBasis, time: f32) {
        let uniforms = SkyUniforms {
            forward: basis.forward.into(),
            tan_half_fov_y: basis.tan_half_fov_y,
            right: basis.right.into(),
            aspect: basis.aspect,
            up: basis.up.into(),
            haze: state.haze,
            zenith: state.zenith.into(),
            exponent: crate::scene::SKY_EXPONENT,
            horizon_sun: state.horizon_sun_side.into(),
            sun_glow: state.sun_glow,
            horizon_anti: state.horizon_anti_side.into(),
            belt: state.belt_of_venus,
            sun_dir: state.sun_dir.into(),
            time,
            sun_color: state.sun_color.into(),
            _pad: 0.0,
            moon_dir: state.moon_dir.into(),
            moon_phase: state.moon_phase as f32,
            sun_ang_radius: SUN_ANG_RADIUS,
            moon_ang_radius: MOON_ANG_RADIUS,
            star_vis: state.star_visibility,
            self_spin: state.self_spin,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Convierte `Vec3` a `[f32; 3]` para el uniform.
impl From<Vec3> for [f32; 3] {
    fn from(v: Vec3) -> Self {
        [v.x, v.y, v.z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_uniform_del_cielo_mide_160_bytes() {
        // sky.wgsl asume este layout exacto (vec3 alineados a 16).
        assert_eq!(std::mem::size_of::<SkyUniforms>(), 160);
    }
}
