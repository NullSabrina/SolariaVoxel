// Shader del resaltado (wireframe del bloque apuntado, v0.4.0).
//
// Muy simple: transforma con la misma matriz mvp y pinta un color naranja fijo.
// La geometria (8 vertices/12 aristas) la construye Rust en coordenadas de mundo.

struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // Naranja brillante en sRGB (ya convertido a lineal en CPU seria lo ideal,
    // pero para un color fijo aproximamos con un valor lineal directo).
    return vec4<f32>(0.95, 0.55, 0.10, 1.0);
}
