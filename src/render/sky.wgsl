// Shader del cielo (v0.30.0).
//
// Se dibuja como un **triangulo a pantalla completa** sin vertex buffer y sin
// escribir profundidad: el mundo lo tapa despues. El fragment reconstruye la
// direccion de mirada a partir de la base de la camara y pinta el gradiente
// cenit <-> horizonte. Los colores llegan ya resueltos desde la CPU (lineales).

struct SkyUniforms {
    forward: vec3<f32>,
    tan_half_fov_y: f32,
    right: vec3<f32>,
    aspect: f32,
    up: vec3<f32>,
    haze: f32,
    zenith: vec3<f32>,
    exponent: f32,
    horizon_sun: vec3<f32>,
    sun_glow: f32,
    horizon_anti: vec3<f32>,
    belt: f32,
    sun_dir: vec3<f32>,
    time: f32,
    sun_color: vec3<f32>,
    pad0: f32,
};

@group(0) @binding(0)
var<uniform> sky: SkyUniforms;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// Triangulo a pantalla completa: cubre el viewport con 3 vertices en NDC.
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = corners[vi];
    var out: VertexOutput;
    // z = 0 (cerca) con depth-compare Always y sin escritura: no importa.
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

// Ruido barato para el dithering triangular.
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Rayo de vista. La camara mira a `forward`; `right`/`up` son su base.
    let dir = normalize(
        sky.forward
            + sky.right * (input.ndc.x * sky.aspect * sky.tan_half_fov_y)
            + sky.up * (input.ndc.y * sky.tan_half_fov_y),
    );

    // Horizonte segun azimut (lado opuesto <-> lado del sol).
    let hd = normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(1.0e-5, 0.0, 1.0e-5));
    let sun_hd = normalize(vec3<f32>(sky.sun_dir.x, 0.0, sky.sun_dir.z) + vec3<f32>(1.0e-5, 0.0, 1.0e-5));
    let towards_sun = smoothstep(-1.0, 1.0, dot(hd, sun_hd));
    let horizon = mix(sky.horizon_anti, sky.horizon_sun, towards_sun);

    // Gradiente vertical: `pow` hace la banda de horizonte fina; por debajo del
    // horizonte el cielo se apoya en el color de horizonte (sin costura).
    let h = max(dir.y, 0.0);
    let t = smoothstep(0.0, 1.0, pow(h, sky.exponent));
    var col = mix(horizon, sky.zenith, t);

    // Halo solar (lobulo Mie hacia adelante): calido cerca del horizonte.
    let cos_sun = dot(dir, sky.sun_dir);
    let glow = pow(max(cos_sun, 0.0), 8.0) * sky.sun_glow;
    col += sky.sun_color * (glow * 0.35);

    // Dithering triangular +-1/255: los degradados oscuros hacen banding en 8 bits.
    let n = hash12(input.clip.xy + vec2<f32>(sky.time, sky.time * 7.0)) - 0.5;
    col += vec3<f32>(n / 255.0);

    return vec4<f32>(col, 1.0);
}
