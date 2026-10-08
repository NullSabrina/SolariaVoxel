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
    _sun_texture: wgpu::Texture,
    _moon_texture: wgpu::Texture,
    _sampler: wgpu::Sampler,
}

/// Lado de la textura del sol, en pixels.
const SUN_TEX: u32 = 16;
/// Lado de la textura (superficie) de la luna.
const MOON_TEX: u32 = 16;

/// Rutas de los assets (superficies por cara para el cubo; ver
/// `examples/gen_celestial.rs`).
mod assets {
    pub const SUN_PATH: &str = "assets/sun.png";
    pub const MOON_PATH: &str = "assets/moon.png";
}

/// Carga un PNG de `path` (RGBA8) o usa el generador procedural `fallback`.
fn load_or(path: &str, w: u32, h: u32, fallback: fn() -> Vec<u8>) -> Vec<u8> {
    match crate::world::atlas::load_png_rgba(path, w, h) {
        Some(p) => {
            println!("[sky] cargado {path} ({w}x{h})");
            p
        }
        None => {
            println!("[sky] sin {path}; uso textura procedural del cielo");
            fallback()
        }
    }
}

/// Crea una textura sRGB RGBA8 y sube sus pixels.
fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Fallback procedural: superficie solar moteada (sin `assets/sun.png`).
fn sun_pixels() -> Vec<u8> {
    let mut px = vec![0u8; (SUN_TEX * SUN_TEX * 4) as usize];
    for y in 0..SUN_TEX {
        for x in 0..SUN_TEX {
            let n = hash01(x, y, 11);
            let r = 255u8;
            let g = (224.0 + 24.0 * n) as u8;
            let b = (70.0 + 110.0 * n) as u8;
            let i = ((y * SUN_TEX + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    px
}

/// Fallback procedural: superficie lunar con crateres (sin `assets/moon.png`).
fn moon_pixels() -> Vec<u8> {
    let craters: [(f32, f32, f32); 5] = [
        (3.5, 4.5, 2.2),
        (11.0, 3.0, 1.6),
        (7.0, 9.5, 2.6),
        (13.0, 12.0, 1.4),
        (2.5, 12.5, 1.8),
    ];
    let mut px = vec![0u8; (MOON_TEX * MOON_TEX * 4) as usize];
    for y in 0..MOON_TEX {
        for x in 0..MOON_TEX {
            let mut v = 170.0 + 26.0 * hash01(x, y, 7);
            for (cx, cy, cr) in craters {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d < cr {
                    v -= 46.0 * (1.0 - d / cr);
                } else if d < cr + 1.0 {
                    v += 12.0;
                }
            }
            let v = v.clamp(0.0, 255.0) as u8;
            let i = ((y * MOON_TEX + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[v, v, (v as f32 * 0.96) as u8, 255]);
        }
    }
    px
}

/// Hash determinista de una celda a `[0, 1)` (fallback del cielo).
fn hash01(x: u32, y: u32, salt: u32) -> f32 {
    let mut h = salt ^ x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65536.0
}

impl SkyPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky.bind_group.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Texturas (superficies por cara para el cubo) con fallback procedural.
        let (sun_texture, sun_view) = create_texture(
            device,
            queue,
            "sky.sun",
            SUN_TEX,
            SUN_TEX,
            &load_or(assets::SUN_PATH, SUN_TEX, SUN_TEX, sun_pixels),
        );
        let (moon_texture, moon_view) = create_texture(
            device,
            queue,
            "sky.moon",
            MOON_TEX,
            MOON_TEX,
            &load_or(assets::MOON_PATH, MOON_TEX, MOON_TEX, moon_pixels),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
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
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sun_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&moon_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
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
            _sun_texture: sun_texture,
            _moon_texture: moon_texture,
            _sampler: sampler,
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

    #[test]
    fn los_fallbacks_procedurales_tienen_el_tamano_esperado() {
        assert_eq!(sun_pixels().len(), (SUN_TEX * SUN_TEX * 4) as usize);
        assert_eq!(moon_pixels().len(), (MOON_TEX * MOON_TEX * 4) as usize);
    }
}
