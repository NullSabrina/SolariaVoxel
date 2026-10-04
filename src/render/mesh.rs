//! Geometria: vertices, indices y los buffers de GPU que los contienen.
//!
//! Un [`Mesh`] es un par de buffers (vertices + indices) ya subidos a la GPU,
//! listos para dibujar. Lo llena el mesher del mundo
//! ([`crate::world::mesh_chunk`]), que genera un vertice por esquina de cara.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Un vertice tal y como lo ve la GPU.
///
/// `#[repr(C)]` garantiza el orden de campos en memoria y `Pod`/`Zeroable` (de
/// bytemuck) nos permiten subir la rebanada al buffer sin `unsafe`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Posicion en el espacio del mundo (x, y, z).
    pub position: [f32; 3],
    /// Coordenadas de textura dentro del atlas (u, v).
    pub uv: [f32; 2],
}

impl Vertex {
    /// Constructor de conveniencia.
    pub const fn new(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self { position, uv }
    }

    /// Atributos: location 0 -> vec3 posicion, location 1 -> vec2 uv.
    /// Es una constante porque `layout` devuelve una referencia `'static`.
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    /// Describe como leer este vertice desde un buffer. Debe coincidir con los
    /// `@location` del shader (`scene.wgsl`).
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Una malla subida a la GPU.
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl Mesh {
    /// Crea la malla a partir de vertices e indices (u32).
    pub fn new(device: &wgpu::Device, label: &str, vertices: &[Vertex], indices: &[u32]) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label}.vertices")),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label}.indices")),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }

    /// Emite los comandos de dibujo de esta malla en un render pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}
