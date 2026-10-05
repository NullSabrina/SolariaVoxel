// Shader de la escena (v0.2.0).
//
// * El vertex shader transforma cada vertice del mundo a la pantalla con la
//   matriz `mvp` (modelo * vista * proyeccion) y pasa las coordenadas UV.
// * El fragment shader muestrea el atlas de texturas en esas UV para pintar el
//   bloque. El atlas y el sampler llegan por el bind group.

// Uniform: la matriz que la CPU actualiza cada frame, mas el factor dia/noche.
// El relleno (`pad0..2`) mantiene el tamano en multiplo de 16 bytes y evita que
// WGSL alinee un `vec3` a 16 (lo que descuadraria el layout respecto a Rust).
struct Uniforms {
    mvp: mat4x4<f32>,
    camera_pos: vec3<f32>,
    day_factor: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
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
    @location(2) sky: f32,
    @location(3) block: f32,
    @location(4) tile: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) sky: f32,
    @location(2) block: f32,
    // Los enteros entre etapas exigen interpolacion plana (sin interpolar).
    @location(3) @interpolate(flat) tile: u32,
    // Posicion en el mundo, para la niebla por distancia.
    @location(4) world_pos: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.sky = input.sky;
    output.block = input.block;
    output.tile = input.tile;
    output.world_pos = input.position;
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
    // Iluminacion: la luz de cielo se apaga con la noche (`day_factor`), pero la
    // de bloque (antorchas) no. Nos quedamos con la mayor de las dos. Un minimo
    // (0.15) evita que las zonas a oscuras queden totalmente negras.
    let light = max(input.sky * uniforms.day_factor, input.block);
    let ambient = 0.15;
    let shade = ambient + (1.0 - ambient) * light;
    let lit = tex.rgb * shade;
    // Niebla a distancia: funde el terreno lejano con el color del cielo, lo que
    // ademas disimula el borde del area cargada.
    let dist = length(input.world_pos - uniforms.camera_pos);
    let span = max(uniforms.fog_end - uniforms.fog_start, 0.001);
    let fog = clamp((dist - uniforms.fog_start) / span, 0.0, 1.0);
    return vec4<f32>(mix(lit, uniforms.fog_color, fog), tex.a);
}
