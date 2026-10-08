// Shader del cielo (v0.31.2).
//
// Se dibuja como un **triangulo a pantalla completa** sin vertex buffer y sin
// escribir profundidad: el mundo lo tapa despues. El fragment reconstruye la
// direccion de mirada, pinta el gradiente cenit <-> horizonte, el sol y la luna
// como **discos texturizados** orientados a la camara (arte de LibreSprite:
// `sun.png`, `moon_phases.png`) con giro propio, y un campo de **estrellas**
// determinista por hash. Los colores llegan ya resueltos desde la CPU (lineales).

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
    moon_dir: vec3<f32>,
    moon_phase: f32,
    sun_ang_radius: f32,
    moon_ang_radius: f32,
    star_vis: f32,
    self_spin: f32,
};

@group(0) @binding(0)
var<uniform> sky: SkyUniforms;

// Texturas pintadas en LibreSprite: cara del sol (16x16) y tira de 8 fases
// lunares (128x16, alpha fuera del disco).
@group(0) @binding(1)
var sun_tex: texture_2d<f32>;
@group(0) @binding(2)
var moon_tex: texture_2d<f32>;
@group(0) @binding(3)
var sky_samp: sampler;

const PI: f32 = 3.14159265;
const BODY_DIST: f32 = 100.0;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = corners[vi];
    var out: VertexOutput;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn hash33(p: vec3<f32>) -> vec3<f32> {
    let q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    let d = dot(q, q.yzx + 33.33);
    return fract((q + d) * vec3<f32>(q.z, q.x, q.y));
}

struct BodyHit {
    hit: f32,
    uv: vec2<f32>,
};

// Interseccion rayo-plano del **billboard** del cuerpo (un cuadrado de lado
// `2*D*tan(ang)` centrado en `center_dir * D`, mirando a la camara, girado por
// `self_spin`). Devuelve UV en 0..1 (alpha fuera del disco la resuelve el arte).
fn body_hit(rd: vec3<f32>, center_dir: vec3<f32>, ang: f32) -> BodyHit {
    let center = center_dir * BODY_DIST;
    let n = normalize(-center_dir);
    let h = BODY_DIST * tan(ang);
    var out: BodyHit;
    out.hit = 0.0;
    out.uv = vec2<f32>(0.0);
    let denom = dot(rd, n);
    if (abs(denom) < 1.0e-6) {
        return out;
    }
    let t = dot(center, n) / denom;
    if (t <= 0.0) {
        return out;
    }
    let rel = rd * t - center;
    let right0 = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), n));
    let up0 = cross(n, right0);
    let sa = sin(sky.self_spin);
    let ca = cos(sky.self_spin);
    let r = right0 * ca + up0 * sa;
    let u = -right0 * sa + up0 * ca;
    let su = dot(rel, r);
    let sv = dot(rel, u);
    if (abs(su) > h || abs(sv) > h) {
        return out;
    }
    out.hit = 1.0;
    out.uv = vec2<f32>(su / (2.0 * h) + 0.5, 0.5 - sv / (2.0 * h));
    return out;
}

// Devuelve rgb del cuerpo en .rgb y cobertura en .a (con fade al horizonte).
fn sun_body(rd: vec3<f32>) -> vec4<f32> {
    let hit = body_hit(rd, sky.sun_dir, sky.sun_ang_radius);
    if (hit.hit < 0.5) {
        return vec4<f32>(0.0);
    }
    let tex = textureSample(sun_tex, sky_samp, hit.uv);
    let fade = smoothstep(-sky.sun_ang_radius - 0.03, -sky.sun_ang_radius + 0.03, sky.sun_dir.y);
    // Emisivo: se aclara por encima de 1 para dar un nucleo caliente.
    return vec4<f32>(tex.rgb * 2.1, tex.a * fade);
}

fn moon_body(rd: vec3<f32>) -> vec4<f32> {
    let hit = body_hit(rd, sky.moon_dir, sky.moon_ang_radius);
    if (hit.hit < 0.5) {
        return vec4<f32>(0.0);
    }
    // La tira de fases es 8 x (16x16); elegimos el frame por `moon_phase`.
    let frame = floor(sky.moon_phase);
    let tex = textureSample(moon_tex, sky_samp, vec2<f32>((frame + hit.uv.x) / 8.0, hit.uv.y));
    let fade = smoothstep(-sky.moon_ang_radius - 0.03, -sky.moon_ang_radius + 0.03, sky.moon_dir.y);
    return vec4<f32>(tex.rgb, tex.a * fade);
}

// Campo de estrellas determinista: cada celda de una rejilla esferica puede
// tener una estrella con brillo y parpadeo propios (sin RNG con estado).
fn star_field(dir: vec3<f32>, t: f32) -> f32 {
    let s = dir * 90.0;
    let base = floor(s);
    var acc = 0.0;
    for (var i = 0; i < 2; i = i + 1) {
        for (var j = 0; j < 2; j = j + 1) {
            for (var k = 0; k < 2; k = k + 1) {
                let cell = base + vec3<f32>(f32(i), f32(j), f32(k));
                let r = hash33(cell);
                let d = length(s - (cell + r));
                let present = step(0.965, hash31(cell + 7.3));
                let bright = 0.35 + 0.65 * fract(r.x * 91.0);
                let tw = 0.75 + 0.25 * sin(t * 3.0 + r.y * 40.0);
                acc += present * smoothstep(0.32, 0.0, d) * bright * tw;
            }
        }
    }
    return acc;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
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

    let h = max(dir.y, 0.0);
    let tgrad = smoothstep(0.0, 1.0, pow(h, sky.exponent));
    var col = mix(horizon, sky.zenith, tgrad);

    // Halo solar: lobulo de Mie hacia adelante (Henyey-Greenstein, g < 1), que
    // concentra la luz alrededor del sol y deja un halo calido cerca del horizonte.
    let cos_sun = dot(dir, sky.sun_dir);
    let g = 0.76;
    let mu = cos_sun;
    let hg = (1.0 - g * g) / (4.0 * PI * pow(1.0 + g * g - 2.0 * g * mu, 1.5));
    col += sky.sun_color * (hg * sky.sun_glow * 0.5);

    // Cinturon de Venus: banda rosa sobre la sombra de la Tierra, en el lado
    // opuesto al sol, poco despues del atardecer / antes del amanecer.
    let anti = 1.0 - towards_sun;
    let band = exp(-pow((dir.y - 0.06) / 0.045, 2.0));
    col += vec3<f32>(0.85, 0.30, 0.55) * (band * sky.belt * anti * 0.35);

    // Estrellas (solo de noche y por encima del horizonte).
    let above = smoothstep(-0.05, 0.2, dir.y);
    col += vec3<f32>(star_field(dir, sky.time) * sky.star_vis * above * 0.9);

    // Cuerpos celestes: discos texturizados, con blending por alpha (fuera del
    // disco se ve el cielo).
    let sun = sun_body(dir);
    col = mix(col, sun.rgb, sun.a);
    let moon = moon_body(dir);
    col = mix(col, moon.rgb, moon.a);

    // Dithering triangular +-1/255: los degradados oscuros hacen banding en 8 bits.
    let n = hash31(vec3<f32>(input.clip.xy, sky.time * 13.0)) - 0.5;
    col += vec3<f32>(n / 255.0);

    return vec4<f32>(col, 1.0);
}
