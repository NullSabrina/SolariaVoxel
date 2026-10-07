//! Preview offline del cielo (Parte A): 24 h en una tira + un hemisferio completo.
//!
//! No arranca el juego ni toca la GPU: usa `scene::SkyState` (funcion pura) para
//! **ver** el gradiente cenit <-> horizonte y afinarlo con datos.
//!
//! Uso:
//! ```text
//! cargo run --release --example sky_preview -- [hora]
//! ```
//!
//! `hora` en `[0,1)` (0 = medianoche, 0.5 = mediodia); por defecto 0.28.
//! Salida en `screenshots/sky_preview.png`.

use std::io::BufWriter;

use solaria_voxel::math::Vec3;
use solaria_voxel::math::color::linear_to_srgb3;
use solaria_voxel::scene::{DayCycle, SKY_EXPONENT, SkyParams, SkyState, sun_direction};

const W: usize = 768;
const STRIP_H: usize = 256;
const HEMI_H: usize = 384;

fn to_rgb(linear: Vec3) -> [u8; 3] {
    let s = linear_to_srgb3([linear.x, linear.y, linear.z]);
    [
        (s[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (s[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (s[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

/// Direccion unitaria desde azimut (grados, 0 = +X) y elevacion (grados).
fn dir_from(az_deg: f32, el_deg: f32) -> Vec3 {
    let az = az_deg.to_radians();
    let el = el_deg.to_radians();
    Vec3::new(az.cos() * el.cos(), el.sin(), az.sin() * el.cos())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let time: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.28);
    let params = SkyParams::default();

    let height = STRIP_H + HEMI_H;
    let mut px = vec![0u8; W * height * 4];

    let mut set = |x: usize, y: usize, rgb: [u8; 3]| {
        if x < W && y < height {
            let i = (y * W + x) * 4;
            px[i] = rgb[0];
            px[i + 1] = rgb[1];
            px[i + 2] = rgb[2];
            px[i + 3] = 255;
        }
    };

    // Tira: 24 columnas horarias, gradiente cenit (arriba) -> horizonte (abajo),
    // mirando hacia el sol de esa hora.
    for x in 0..W {
        let h = x as f32 / W as f32 * 24.0;
        let state = SkyState::at(&DayCycle::new(h / 24.0), &params);
        let az = azimuth_of(state.sun_dir);
        for y in 0..STRIP_H {
            let v = y as f32 / (STRIP_H - 1) as f32;
            let el = 90.0 * (1.0 - v) - 10.0 * v;
            let c = state.sample(dir_from(az, el), SKY_EXPONENT);
            set(x, y, to_rgb(c));
        }
    }

    // Hemisferio completo a la hora pedida (azimut horizontal, elevacion vertical).
    let state = SkyState::at(&DayCycle::new(time), &params);
    for y in 0..HEMI_H {
        let el = 90.0 - 180.0 * (y as f32 / (HEMI_H - 1) as f32);
        for x in 0..W {
            let az = 360.0 * (x as f32 / (W - 1) as f32);
            let c = state.sample(dir_from(az, el), SKY_EXPONENT);
            set(x, STRIP_H + y, to_rgb(c));
        }
    }

    std::fs::create_dir_all("screenshots").ok();
    let path = "screenshots/sky_preview.png";
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut encoder = png::Encoder::new(BufWriter::new(file), W as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("cabecera PNG");
    writer.write_image_data(&px).expect("datos PNG");

    let sun = sun_direction(time, params.sun_tilt_deg);
    println!(
        "sky_preview: hora {time:.3} | elevacion solar {:.1} deg | sol dir ({:.2},{:.2},{:.2}) | {path}",
        state.sun_elevation_deg, sun.x, sun.y, sun.z
    );
}

/// Azimut (grados) de una direccion horizontal.
fn azimuth_of(dir: Vec3) -> f32 {
    dir.z.atan2(dir.x).to_degrees()
}
