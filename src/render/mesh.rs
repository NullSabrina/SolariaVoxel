//! Geometria: vertices, indices y los buffers de GPU que los contienen.
//!
//! Un [`Mesh`] es un par de buffers (vertices + indices) ya subidos a la GPU,
//! listos para dibujar. Lo llena el mesher del mundo
//! ([`crate::world::mesh_chunk`]), que genera un vertice por esquina de cara.

use bytemuck::{Pod, Zeroable};

/// Un vertice tal y como lo ve la GPU.
///
/// `#[repr(C)]` garantiza el orden de campos en memoria y `Pod`/`Zeroable` (de
/// bytemuck) nos permiten subir la rebanada al buffer sin `unsafe`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Posicion en el espacio del mundo (x, y, z).
    pub position: [f32; 3],
    /// Coordenadas de textura **dentro del tile**, en unidades de tile (una cara
    /// fusionada de W x H bloques usa 0..W, 0..H; el sampler repite).
    pub uv: [f32; 2],
    /// Luz de **cielo** (0..1) que llega a este vertice. El shader la multiplica
    /// por el factor dia/noche.
    pub sky: f32,
    /// Luz de **bloque** (antorchas, 0..1). No la apaga la noche.
    pub block: f32,
    /// Indice del tile dentro del array de texturas (0..TILES).
    pub tile: u32,
}

impl Vertex {
    /// Constructor de conveniencia (luz a pleno sol por defecto).
    pub const fn new(position: [f32; 3], uv: [f32; 2], tile: u32) -> Self {
        Self {
            position,
            uv,
            sky: 1.0,
            block: 0.0,
            tile,
        }
    }

    /// Constructor con las dos luces explicitas (0..1 cada una).
    pub const fn with_light(
        position: [f32; 3],
        uv: [f32; 2],
        sky: f32,
        block: f32,
        tile: u32,
    ) -> Self {
        Self {
            position,
            uv,
            sky,
            block,
            tile,
        }
    }

    /// Atributos: location 0 -> vec3 posicion, location 1 -> vec2 uv,
    /// location 2 -> float cielo, location 3 -> float bloque, location 4 -> uint
    /// tile. Es una constante porque `layout` devuelve una referencia `'static`.
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Float32, 4 => Uint32
    ];

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

/// Capacidad reservada para `bytes`: la potencia de dos siguiente (>= `bytes`).
///
/// Invariante critica: el buffer GPU se crea con **este** tamano (no con el
/// tamano exacto de los datos). Si se creara con el exacto, `update` creeria que
/// cabe una malla mayor y `write_buffer` se saldria del buffer (crash de
/// validacion de wgpu).
#[inline]
pub fn buffer_capacity(bytes: u64) -> u64 {
    bytes.next_power_of_two().max(1)
}

/// Una malla subida a la GPU.
///
/// Los buffers se crean con holgura (`next_power_of_two`) y se **reutilizan** al
/// re-meshear con [`Mesh::update`] mientras quepan: asi editar un bloque no
/// crea/destruye buffers GPU cada vez.
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    vertex_capacity: u64,
    index_capacity: u64,
}

impl Mesh {
    /// Crea la malla a partir de vertices e indices (u32).
    ///
    /// Los buffers se crean con la capacidad **reservada** (`next_power_of_two`),
    /// no con el tamano exacto de los datos. Si se crearan con el tamano exacto,
    /// `vertex_capacity` mentiria: `update` creeria que cabe una malla mas grande
    /// y `write_buffer` se saldria del buffer (overrun de la GPU). Ese era el
    /// crash que aparecia al re-meshear una seccion que crecia.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Self {
        let vbytes = std::mem::size_of_val(vertices) as u64;
        let ibytes = std::mem::size_of_val(indices) as u64;
        let vertex_capacity = buffer_capacity(vbytes);
        let index_capacity = buffer_capacity(ibytes);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}.vertices")),
            size: vertex_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label}.indices")),
            size: index_capacity,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if vbytes > 0 {
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }
        if ibytes > 0 {
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(indices));
        }
        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            vertex_capacity,
            index_capacity,
        }
    }

    /// Reutiliza los buffers para nuevos vertices/indices (re-mesheo). Solo los
    /// recrea si el nuevo tamano no cabe en la capacidad reservada.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        indices: &[u32],
    ) {
        let vbytes = std::mem::size_of_val(vertices) as u64;
        let ibytes = std::mem::size_of_val(indices) as u64;
        if vbytes > self.vertex_capacity {
            self.vertex_capacity = buffer_capacity(vbytes);
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh.vertices"),
                size: self.vertex_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if ibytes > self.index_capacity {
            self.index_capacity = buffer_capacity(ibytes);
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh.indices"),
                size: self.index_capacity,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if vbytes > 0 {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }
        if ibytes > 0 {
            queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(indices));
        }
        self.index_count = indices.len() as u32;
    }

    /// Bytes reservados en GPU (vertices + indices) de esta malla. Es la
    /// capacidad reservada (con holgura), no el uso exacto; sirve de cota.
    pub fn gpu_bytes(&self) -> u64 {
        self.vertex_capacity + self.index_capacity
    }

    /// Numero de indices (0 si la malla esta vacia). `indices / 3` = triangulos.
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Emite los comandos de dibujo de esta malla en un render pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.index_count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_capacidad_cubre_los_datos_y_es_potencia_de_dos() {
        // Regresion del overrun: la capacidad reservada debe ser SIEMPRE >= que
        // los bytes escritos, o `write_buffer` se sale del buffer GPU.
        for bytes in [0u64, 1, 3, 4, 4096, 84_352, 84_992, 100_000, 1 << 20] {
            let cap = buffer_capacity(bytes);
            assert!(cap >= bytes, "capacidad {cap} < bytes {bytes}");
            assert!(
                cap.is_power_of_two(),
                "capacidad {cap} no es potencia de dos"
            );
        }
    }
}
