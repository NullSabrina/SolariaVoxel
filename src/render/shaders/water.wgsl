// Shader del AGUA (v0.8.8): lamina animada, translucida y con brillo especular.
//
// Comparte el bind group de la escena (uniform + atlas + sampler): el struct
// `Uniforms` DEBE coincidir byte a byte con el de `scene.wgsl` (144 bytes).

struct Uniforms {
    mvp: mat4x4<f32>,
    camera_pos: vec3<f32>,
    day_factor: f32,
    horizon_sun: vec3<f32>,
    fog_start: f32,
    horizon_anti: vec3<f32>,
    fog_end: f32,
    sun_dir: vec3<f32>,
    time: f32,
    pad0: vec3<f32>,
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

// Ruido de valor 3D **continuo** (hash en la rejilla + interpolacion suave).
// Se muestrea en espacio de mundo para todas las caras: la cara superior y las
// laterales comparten el mismo campo, sin costura en la arista ni bandas al
// apilar agua.
fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(123.34, 345.45, 567.89));
    q += dot(q, q + 34.345);
    return fract(q.x * q.y * q.z);
}

fn value_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash31(i + vec3<f32>(0.0, 0.0, 0.0));
    let c100 = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let t = uniforms.time;

    // La normal se obtiene de las derivadas de la posicion en pantalla: la
    // superficie interpolada ya es una rampa, y dpdx/dpdy dan su inclinacion
    // real por pixel (sin necesitar normales por vertice). Se calcula ANTES del
    // color porque decide que plano usar para el tinte.
    let view_dir = normalize(uniforms.camera_pos - input.world_pos);
    var n = cross(dpdx(input.world_pos), dpdy(input.world_pos));
    if (dot(n, n) < 1.0e-8) {
        n = vec3<f32>(0.0, 1.0, 0.0);
    }
    var normal = normalize(n);
    // Orientada **hacia la camara**: el producto vectorial puede salir al reves
    // segun el lado, y asi la cara superior y las laterales dan el mismo brillo.
    if (dot(normal, view_dir) < 0.0) {
        normal = -normal;
    }

    let is_top = normal.y > 0.5;

    // Color base del tile de agua: una sola muestra **fija**. La animacion es un
    // campo de ruido continuo en **3D mundo**, sin repeticion ni costura entre
    // la cara superior y las laterales.
    let base = textureSample(atlas, atlas_sampler, vec2<f32>(0.5, 0.5), input.tile);

    // Dos capas de ruido a distinta velocidad y direccion: corriente.
    let flow_a = input.world_pos * 0.32 + vec3<f32>(t * 0.060, t * 0.025, t * 0.010);
    let flow_b = input.world_pos * 0.21 + vec3<f32>(-t * 0.040, t * 0.055, -t * 0.015);
    let ripple = (value_noise3(flow_a) - 0.5) * 0.35
        + (value_noise3(flow_b) - 0.5) * 0.25;

    // Perturba la normal con el gradiente del ruido solo en la superficie: los
    // reflejos especulares se mueven y el agua deja de parecer una lamina
    // plastica. Las paredes mantienen la normal vertical (sin brillos raros).
    if (is_top) {
        let e = 0.06;
        let h0 = value_noise3(flow_a);
        let hx = value_noise3(flow_a + vec3<f32>(e, 0.0, 0.0));
        let hz = value_noise3(flow_a + vec3<f32>(0.0, 0.0, e));
        normal = normalize(normal + vec3<f32>((h0 - hx) * 4.0, 0.0, (h0 - hz) * 4.0));
    }
    // Color base con un minimo azul: en sombra/noche el tile puede quedar casi
    // negro y el agua se lee como un agujero.
    var color = max(base.rgb, vec3<f32>(0.06, 0.15, 0.30)) * (1.0 + ripple);

    // Tinte por nivel de agua: la fuente (nivel 8) llega con `fract(y) ~ 8/9`
    // y se ve mas clara; el flujo (1..7) queda mas oscuro. Solo en la cara
    // superior: en las laterales `fract(y)` no representa el nivel.
    let level01 = clamp(fract(input.world_pos.y) / (8.0 / 9.0), 0.0, 1.0);
    let level_tint = select(1.0, mix(0.78, 1.10, level01), is_top);
    color = color * level_tint;

    // Fresnel: mas opaco en angulos rasantes (el agua se ve "de canto").
    let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 3.0);

    // TODO (Fase C, pendiente): espuma en la base de cascadas. Requiere que el
    // mesher pase `waterfall` por vertice; hoy no existe ese atributo y no se
    // inventa un canal (scene.wgsl comparte el layout).
    let alpha = clamp(base.a * 0.85 + 0.15 + fresnel * 0.35, 0.0, 0.95);

    // Blinn-Phong: especular con la direccion real del sol + la de la camara.
    let light_dir = normalize(uniforms.sun_dir + vec3<f32>(0.0, 0.15, 0.0));
    let half_vec = normalize(light_dir + view_dir);
    let spec = pow(max(dot(normal, half_vec), 0.0), 48.0);

    // Luz (cielo por dia/noche + bloque) y oscurecido en cuevas: con poca luz
    // el agua se ve mas oscura y apagada.
    let lum = max(input.sky * uniforms.day_factor, input.block);
    // El agua conserva algo de azul incluso a la sombra (en Minecraft el agua
    // no se vuelve negra en un canon); antes el suelo era 0.30 y se veia negro.
    let shade = mix(0.55, 1.0, lum);
    color = color * shade
        + vec3<f32>(1.0, 1.0, 1.0) * spec * 0.5 * (0.25 + 0.75 * lum);

    // Niebla direccional por distancia (mismo horizonte que cielo y escena).
    let to_frag = input.world_pos - uniforms.camera_pos;
    let hd = normalize(vec3<f32>(to_frag.x, 0.0, to_frag.z) + vec3<f32>(1.0e-5, 0.0, 1.0e-5));
    let sun_hd = normalize(vec3<f32>(uniforms.sun_dir.x, 0.0, uniforms.sun_dir.z) + vec3<f32>(1.0e-5, 0.0, 1.0e-5));
    let fog_color = mix(uniforms.horizon_anti, uniforms.horizon_sun, smoothstep(-1.0, 1.0, dot(hd, sun_hd)));
    let dist = length(to_frag);
    let span = max(uniforms.fog_end - uniforms.fog_start, 0.001);
    let fog = clamp((dist - uniforms.fog_start) / span, 0.0, 1.0);
    color = mix(color, fog_color, fog);

    return vec4<f32>(color, alpha);
}
