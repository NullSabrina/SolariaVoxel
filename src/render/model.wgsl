// Shader del **modelo** de color plano (mano en primera persona, personaje).
//
// Cada vertice trae su color ya sombreado por cara (bajado en la CPU); el
// fragment lo multiplica por la luz dia/noche. No usa el atlas: el modelo son
// cubos de color, no voxeles texturizados.

struct Uniforms {
    mvp: mat4x4<f32>,
    // Luz global (0..1): mezcla dia/noche.
    light: f32,
    // Relleno a 16 bytes (WGSL alinea el struct a 16). Debe medir 80 bytes,
    // igual que `ModelUniforms` en Rust.
    _a: f32,
    _b: f32,
    _c: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct In {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: In) -> Out {
    var out: Out;
    out.clip = u.mvp * vec4<f32>(input.pos, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color * u.light, 1.0);
}
