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

// El atlas (una textura 2D con todos los bloques) y su sampler.
@group(0) @binding(1)
var atlas: texture_2d<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

// Entrada del vertex shader: debe coincidir con `Vertex::layout()` en Rust.
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Lee el pixel del atlas correspondiente a este vertice de la cara.
    return textureSample(atlas, atlas_sampler, input.uv);
}
