//! Geometria: vertices, indices y los buffers de GPU que los contienen.
//!
//! Un [`Mesh`] es un par de buffers (vertices + indices) ya subidos a la GPU,
//! listos para dibujar. En v0.1.2 solo sabemos construir un cubo, pero la
//! estructura ya sirve para cualquier malla (los chunks seran mallas grandes).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::render::color::srgb_rgb;

/// Un vertice tal y como lo ve la GPU.
///
/// `#[repr(C)]` garantiza que los campos van seguidos en memoria en este orden
/// (sin reordenar), que es lo que espera el VertexBufferLayout. `Pod`/`Zeroable`
/// (de bytemuck) nos permiten convertir una rebanada de `Vertex` a bytes para
/// subirla al buffer sin `unsafe`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Posicion en el espacio del modelo (x, y, z).
    pub position: [f32; 3],
    /// Color RGB (0..1). De momento el color va por vertice; mas adelante lo
    /// cambiaremos por coordenadas de textura y una paleta de bloques.
    pub color: [f32; 3],
}

impl Vertex {
    /// Constructor de conveniencia.
    pub const fn new(position: [f32; 3], color: [f32; 3]) -> Self {
        Self { position, color }
    }

    /// Atributos del vertice: location 0 -> vec3 posicion, location 1 -> vec3
    /// color. Es una constante porque `Vertex::layout` devuelve una referencia
    /// de vida `'static` a este array.
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

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

    /// Construye un cubo de semilado `half` (el lado mide `2 * half`),
    /// centrado en el origen del modelo.
    ///
    /// Usamos 24 vertices (4 por cara) en lugar de 8: aunque comparten posicion,
    /// cada cara necesita su propio color, y un vertice solo puede tener un
    /// color. 6 caras x 4 = 24 vertices, y 6 x 2 triangulos x 3 = 36 indices.
    pub fn cube(device: &wgpu::Device, half: f32) -> Self {
        let h = half;

        // Un color distinto por cara para reconocer la orientacion en 3D.
        // Los definimos en sRGB (como se ven) y los pasamos a lineal.
        let color_px = srgb_rgb([0.85, 0.25, 0.20]); // +X  rojo
        let color_nx = srgb_rgb([0.50, 0.15, 0.12]); // -X  rojo oscuro
        let color_py = srgb_rgb([0.35, 0.80, 0.30]); // +Y  verde
        let color_ny = srgb_rgb([0.15, 0.45, 0.15]); // -Y  verde oscuro
        let color_pz = srgb_rgb([0.25, 0.50, 0.95]); // +Z  azul
        let color_nz = srgb_rgb([0.12, 0.22, 0.50]); // -Z  azul oscuro

        #[rustfmt::skip]
        let vertices: [Vertex; 24] = [
            // +X
            Vertex::new([ h, -h, -h], color_px),
            Vertex::new([ h, -h,  h], color_px),
            Vertex::new([ h,  h,  h], color_px),
            Vertex::new([ h,  h, -h], color_px),
            // -X
            Vertex::new([-h, -h,  h], color_nx),
            Vertex::new([-h, -h, -h], color_nx),
            Vertex::new([-h,  h, -h], color_nx),
            Vertex::new([-h,  h,  h], color_nx),
            // +Y
            Vertex::new([-h,  h, -h], color_py),
            Vertex::new([-h,  h,  h], color_py),
            Vertex::new([ h,  h,  h], color_py),
            Vertex::new([ h,  h, -h], color_py),
            // -Y
            Vertex::new([-h, -h,  h], color_ny),
            Vertex::new([-h, -h, -h], color_ny),
            Vertex::new([ h, -h, -h], color_ny),
            Vertex::new([ h, -h,  h], color_ny),
            // +Z
            Vertex::new([-h, -h,  h], color_pz),
            Vertex::new([ h, -h,  h], color_pz),
            Vertex::new([ h,  h,  h], color_pz),
            Vertex::new([-h,  h,  h], color_pz),
            // -Z
            Vertex::new([ h, -h, -h], color_nz),
            Vertex::new([-h, -h, -h], color_nz),
            Vertex::new([-h,  h, -h], color_nz),
            Vertex::new([ h,  h, -h], color_nz),
        ];

        // Cada cara usa sus 4 vertices como dos triangulos: (0,1,2) y (0,2,3).
        let mut indices = [0u32; 36];
        for (face, chunk) in indices.chunks_exact_mut(6).enumerate() {
            let base = (face * 4) as u32;
            chunk.copy_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        Self::new(device, "cube", &vertices, &indices)
    }

    /// Emite los comandos de dibujo de esta malla en un render pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}
