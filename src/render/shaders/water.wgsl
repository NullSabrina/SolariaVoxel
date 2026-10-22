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

// Ruido de valor 2D **continuo** (hash en la rejilla + interpolacion suave).
// No es periodico: a diferencia de repetir el tile del atlas, no tiene costura
// cada N bloques, asi que se puede desplazar con el tiempo sin lineas.
fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 345.45));
    q += dot(q, q + 34.345);
    return fract(q.x * q.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let t = uniforms.time;

    // La normal se obtiene de las derivadas de la posicion en pantalla: la
    // superficie interpolada ya es una rampa, y dpdx/dpdy dan su inclinacion
    // real por pixel (sin necesitar normales por vertice). Se calcula ANTES del
    // color porque decide que plano usar para las UV.
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

    // UV en **espacio de mundo**, derivadas de `world_pos` (no del UV local del
    // vertice). Asi dos chunks adyacentes comparten fase en el borde aunque el
    // mesher cambie, y las caras laterales (que antes usaban 0..1 por bloque)
    // tambien quedan continuas.
    let is_top = normal.y > 0.5;
    let side_uv = vec2<f32>(input.world_pos.x + input.world_pos.z, input.world_pos.y);
    let surf_uv = select(side_uv, input.world_pos.xz, is_top);

    // Color base del tile de agua: una sola muestra **fija**. Antes se hacia
    // scroll del atlas (`uv + time`), pero el tile (16x16) NO es periodico y
    // cada repeticion dejaba una costura recta que se movia con el agua: eso
    // eran las "grietas" que dejaban ver el fondo. La animacion ahora es un
    // campo de ruido continuo en mundo, sin repeticion.
    let base = textureSample(atlas, atlas_sampler, vec2<f32>(0.5, 0.5), input.tile);

    // Dos capas de ruido a distinta velocidad y direccion: corriente.
    let flow_a = surf_uv * 0.32 + vec2<f32>(t * 0.060, t * 0.025);
    let flow_b = surf_uv * 0.21 + vec2<f32>(-t * 0.040, t * 0.055);
    let ripple = (value_noise(flow_a) - 0.5) * 0.35
        + (value_noise(flow_b) - 0.5) * 0.25;
    var color = base.rgb * (1.0 + ripple);

    // Tinte por nivel de agua: la fuente (nivel 8) llega con `fract(y) ~ 0.875`
    // y se ve mas clara; el flujo (1..7) queda mas oscuro. Solo en la cara
    // superior: en las laterales `fract(y)` no representa el nivel.
    let level01 = clamp(fract(input.world_pos.y) / 0.875, 0.0, 1.0);
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
    let shade = mix(0.30, 1.0, lum);
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
