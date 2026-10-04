// Shader del cubo (v0.1.2).
//
// Es un shader minimo pero completo:
//  * El vertex shader transforma cada vertice del mundo a la pantalla usando la
//    matriz `mvp` (modelo * vista * proyeccion) que le pasamos desde Rust.
//  * El fragment shader "pinta" cada pixel con el color que venia del vertice.
//
// Todavia no hay texturas ni iluminacion: cada cara del cubo tiene su propio
// color, asi que ya se distingue bien en 3D.

// Uniform: un solo bloque de datos que la CPU actualiza cada frame.
struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Entrada del vertex shader: debe coincidir con `Vertex::layout()` en Rust.
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

// Salida del vertex shader y entrada del fragment shader.
struct VertexOutput {
    // `clip_position` es obligatorio: wgpu sabe que es la posicion en clip space.
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Pasamos el vertice a "clip space": vec4 con w = 1 (punto, no direccion).
    output.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    // Interpolamos el color a lo largo de los triangulos.
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
