// Shader de la interfaz 2D (hotbar/inventario).
//
// * Los vertices ya llegan en NDC (la CPU convierte de pixels), asi que el vertex
//   shader solo los pasa.
// * El fragment elige la textura segun la capa: `< 0` = textura de interfaz,
//   `>= 0` = tile del atlas (icono de bloque).

struct UiInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: i32,
};

struct UiOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: i32,
};

@group(0) @binding(0) var atlas: texture_2d_array<f32>;
@group(0) @binding(1) var gui: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(input: UiInput) -> UiOutput {
    var out: UiOutput;
    out.clip = vec4<f32>(input.pos, 0.0, 1.0);
    out.uv = input.uv;
    out.layer = input.layer;
    return out;
}

@fragment
fn fs_main(input: UiOutput) -> @location(0) vec4<f32> {
    if (input.layer < 0) {
        return textureSample(gui, samp, input.uv);
    }
    return textureSample(atlas, samp, input.uv, input.layer);
}
