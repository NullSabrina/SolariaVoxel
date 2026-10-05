// Shader del AGUA (v0.8.8): lamina animada, translucida y con brillo especular.
//
// Comparte el bind group de la escena (uniform + atlas + sampler), pero usa un
// struct de uniform con un campo `time` en el hueco de relleno (offset 100), de
// modo que el buffer sigue midiendo 112 bytes y el layout coincide byte a byte.

struct Uniforms {
    mvp: mat4x4<f32>,
    camera_pos: vec3<f32>,
    day_factor: f32,
    fog_color: vec3<f32>,
    fog_start: f32,
    fog_end: f32,
    // Tiempo (s) para animar las UV; ocupa el primer `pad` del shader de escena.
    time: f32,
    pad0: f32,
    pad1: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var atlas: texture_2d_array<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

// Debe coincidir con `Vertex::layout()` en Rust.
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
    @location(3) @interpolate(flat) tile: u32,
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
    let t = uniforms.time;

    // UVs animadas: dos muestras del atlas con fases de tiempo opuestas. Al
    // mezclarlas con sin() aparece una ondulacion que se mueve, sin textura
    // de flujo aparte. El factor 0.25 mantiene la escala del tile cerca de 1.
    let uv_a = input.uv * 0.25 + vec2<f32>(t * 0.05, t * 0.02);
    let uv_b = input.uv * 0.25 + vec2<f32>(-t * 0.04, t * 0.03);
    let a = textureSample(atlas, atlas_sampler, uv_a, input.tile);
    let b = textureSample(atlas, atlas_sampler, uv_b, input.tile);
    let flow = 0.5 + 0.5 * sin(t * 1.3);
    var color = mix(a.rgb, b.rgb, flow);

    // La normal se obtiene de las derivadas de la posicion en pantalla: la
    // superficie interpolada ya es una rampa, y dpdx/dpdy dan su inclinacion
    // real por pixel (sin necesitar normales por vertice).
    var normal = normalize(cross(dpdx(input.world_pos), dpdy(input.world_pos)));
    if (normal.y < 0.0) {
        normal = -normal;
    }

    // Blinn-Phong: especular con una direccion de sol fija + la de la camara.
    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let view_dir = normalize(uniforms.camera_pos - input.world_pos);
    let half_vec = normalize(light_dir + view_dir);
    let spec = pow(max(dot(normal, half_vec), 0.0), 48.0);

    // Luz (cielo por dia/noche + bloque) y oscurecido en cuevas: con poca luz
    // el agua se ve mas oscura y apagada.
    let lum = max(input.sky * uniforms.day_factor, input.block);
    let shade = mix(0.30, 1.0, lum);
    color = color * shade
        + vec3<f32>(1.0, 1.0, 1.0) * spec * 0.5 * (0.25 + 0.75 * lum);

    // Niebla por distancia, igual que la escena, para fundir con el cielo.
    let dist = length(input.world_pos - uniforms.camera_pos);
    let span = max(uniforms.fog_end - uniforms.fog_start, 0.001);
    let fog = clamp((dist - uniforms.fog_start) / span, 0.0, 1.0);
    color = mix(color, uniforms.fog_color, fog);

    let alpha = clamp(a.a * 0.85 + 0.15, 0.0, 1.0);
    return vec4<f32>(color, alpha);
}
