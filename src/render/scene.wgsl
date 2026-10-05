// Shader de la escena (v0.2.0).
//
// * El vertex shader transforma cada vertice del mundo a la pantalla con la
//   matriz `mvp` (modelo * vista * proyeccion) y pasa las coordenadas UV.
// * El fragment shader muestrea el atlas de texturas en esas UV para pintar el
//   bloque. El atlas y el sampler llegan por el bind group.

// Uniform: la matriz que la CPU actualiza cada frame.
struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// El atlas como array de texturas (una capa de 16x16 por tile) y su sampler.
// Las UV llegan en unidades de tile (0..W, 0..H) y el sampler repite.
@group(0) @binding(1)
var atlas: texture_2d_array<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

// Entrada del vertex shader: debe coincidir con `Vertex::layout()` en Rust.
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
    @location(3) tile: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    // Los enteros entre etapas exigen interpolacion plana (sin interpolar).
    @location(2) @interpolate(flat) tile: u32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.light = input.light;
    output.tile = input.tile;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas, atlas_sampler, input.uv, input.tile);
    // Transparencia por recorte (cutout), como las hojas en Minecraft: los
    // texeles con alfa bajo se descartan y se ve a traves.
    if (tex.a < 0.5) {
        discard;
    }
    // Iluminacion: la luz de cielo (0..1) modula el color. Un minimo (0.15)
    // evita que las zonas a oscuras queden totalmente negras e ilegibles.
    let ambient = 0.15;
    let shade = ambient + (1.0 - ambient) * input.light;
    return vec4<f32>(tex.rgb * shade, tex.a);
}
