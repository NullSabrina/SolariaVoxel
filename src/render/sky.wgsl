// Shader del cielo (v0.31.0).
//
// Se dibuja como un **triangulo a pantalla completa** sin vertex buffer y sin
// escribir profundidad: el mundo lo tapa despues. El fragment reconstruye la
// direccion de mirada, pinta el gradiente cenit <-> horizonte, el sol y la luna
// como **cubos 3D** (interseccion rayo-caja orientada) con fases lunares, y un
// campo de **estrellas** determinista por hash. Los colores llegan ya resueltos
// desde la CPU (lineales).

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

const PI: f32 = 3.14159265;
const BODY_DIST: f32 = 100.0;
const BODY_YAW: f32 = 0.52; // ~30 grados
const BODY_PITCH: f32 = -0.34; // ~-20 grados

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

fn rot_y(a: f32) -> mat3x3<f32> {
    let s = sin(a);
    let c = cos(a);
    return mat3x3<f32>(vec3<f32>(c, 0.0, -s), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(s, 0.0, c));
}

fn rot_x(a: f32) -> mat3x3<f32> {
    let s = sin(a);
    let c = cos(a);
    return mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, c, s), vec3<f32>(0.0, -s, c));
}

struct BodyHit {
    hit: f32,
    normal: vec3<f32>,
    local: vec3<f32>,
};

// Interseccion rayo-caja orientada (cubo centrado en `center_dir * BODY_DIST`).
fn body_hit(rd: vec3<f32>, center_dir: vec3<f32>, ang: f32) -> BodyHit {
    let center = center_dir * BODY_DIST;
    let h = BODY_DIST * tan(ang);
    // R = Ry(self_spin) * Ry(yaw) * Rx(pitch); su traspuesta pasa a espacio local.
    let r = rot_y(sky.self_spin) * rot_y(BODY_YAW) * rot_x(BODY_PITCH);
    let rt = transpose(r);
    let ro = rt * (-center);
    let rdd = rt * rd;
    let inv = 1.0 / rdd;
    let ta = (-h - ro) * inv;
    let tb = (h - ro) * inv;
    let tmin = min(ta, tb);
    let tmax = max(ta, tb);
    let tenter = max(max(tmin.x, tmin.y), tmin.z);
    let texit = min(min(tmax.x, tmax.y), tmax.z);

    var out: BodyHit;
    out.hit = 0.0;
    out.normal = vec3<f32>(0.0, 1.0, 0.0);
    out.local = vec3<f32>(0.0);
    if (texit < max(tenter, 0.0)) {
        return out;
    }
    let p = ro + rdd * tenter;
    let ap = abs(p);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (ap.x >= ap.y && ap.x >= ap.z) {
        n = vec3<f32>(sign(p.x), 0.0, 0.0);
    } else if (ap.y >= ap.z) {
        n = vec3<f32>(0.0, sign(p.y), 0.0);
    } else {
        n = vec3<f32>(0.0, 0.0, sign(p.z));
    }
    out.hit = 1.0;
    out.normal = normalize(r * n);
    out.local = p / h;
    return out;
}

// Devuelve rgb del cuerpo en .rgb y cobertura en .a.
fn sun_body(rd: vec3<f32>) -> vec4<f32> {
    let hit = body_hit(rd, sky.sun_dir, sky.sun_ang_radius);
    if (hit.hit < 0.5) {
        return vec4<f32>(0.0);
    }
    let shade = 0.6 + 0.4 * max(hit.normal.y, 0.0);
    // Fade al cruzar el horizonte.
    let fade = smoothstep(-sky.sun_ang_radius - 0.03, -sky.sun_ang_radius + 0.03, sky.sun_dir.y);
    let col = sky.sun_color * (shade * 1.7) + vec3<f32>(0.30);
    return vec4<f32>(col, fade);
}

fn moon_body(rd: vec3<f32>) -> vec4<f32> {
    let hit = body_hit(rd, sky.moon_dir, sky.moon_ang_radius);
    if (hit.hit < 0.5) {
        return vec4<f32>(0.0);
    }
    let shade = 0.6 + 0.4 * max(hit.normal.y, 0.0);
    // Fase: terminador a lo largo de la cara local (aproximacion estilizada).
    let p = fract(sky.moon_phase / 8.0);
    let pa = p * 2.0 * PI;
    let lit = 0.5 * (1.0 - cos(pa));
    var u = hit.local.x;
    if (sin(pa) < 0.0) {
        u = -u;
    }
    let term = 1.0 - 2.0 * lit;
    let s = smoothstep(term - 0.25, term + 0.25, u);
    let dark = vec3<f32>(0.10, 0.10, 0.13);
    let bright = vec3<f32>(0.92, 0.92, 0.86);
    let fade = smoothstep(-sky.moon_ang_radius - 0.03, -sky.moon_ang_radius + 0.03, sky.moon_dir.y);
    return vec4<f32>(mix(dark, bright, s) * shade, fade);
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

    // Halo solar (lobulo Mie hacia adelante): calido cerca del horizonte.
    let cos_sun = dot(dir, sky.sun_dir);
    let glow = pow(max(cos_sun, 0.0), 8.0) * sky.sun_glow;
    col += sky.sun_color * (glow * 0.35);

    // Estrellas (solo de noche y por encima del horizonte).
    let above = smoothstep(-0.05, 0.2, dir.y);
    col += vec3<f32>(star_field(dir, sky.time) * sky.star_vis * above * 0.9);

    // Cuerpos celestes (cubos 3D): primero el que este mas lejos, luego el otro.
    let sun = sun_body(dir);
    col = mix(col, sun.rgb, sun.a);
    let moon = moon_body(dir);
    col = mix(col, moon.rgb, moon.a);

    // Dithering triangular +-1/255: los degradados oscuros hacen banding en 8 bits.
    let n = hash31(vec3<f32>(input.clip.xy, sky.time * 13.0)) - 0.5;
    col += vec3<f32>(n / 255.0);

    return vec4<f32>(col, 1.0);
}
