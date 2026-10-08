//! Genera las **texturas del rig celeste** para el cubo del sol y de la luna.
//!
//! No son sprites 2D (un disco) sino **superficies por cara** pensadas para un
//! cubo: el sol es una superficie emisiva moteada y la luna una superficie lunar
//! con crateres. Se aplican a las 6 caras y el sombreado por cara + la fase dan
//! la forma. Reproducible: `cargo run --example gen_celestial`.
//!
//! Salida: `assets/sun.png` y `assets/moon.png` (16x16, RGBA8).

use std::io::BufWriter;

const N: u32 = 16;

/// Hash determinista de una celda a `[0, 1)`.
fn hash(x: u32, y: u32, salt: u32) -> f32 {
    let mut h = salt ^ (x.wrapping_mul(0x9E37_79B9)) ^ (y.wrapping_mul(0x85EB_CA6B));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65536.0
}

/// Ruido suave (interpolado) para grano sin pixelado duro.
fn smooth_noise(x: f32, y: f32, salt: u32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let (fx, fy) = (x - xi, y - yi);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let c = |dx, dy| hash((xi as i32 + dx) as u32, (yi as i32 + dy) as u32, salt);
    let a = c(0, 0) + (c(1, 0) - c(0, 0)) * ux;
    let b = c(0, 1) + (c(1, 1) - c(0, 1)) * ux;
    a + (b - a) * uy
}

fn main() {
    std::fs::create_dir_all("assets").ok();

    // --- Sol: superficie emisiva moteada (amarillo claro con zonas calidas). ---
    let mut sun = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let n = smooth_noise(x as f32 * 0.35, y as f32 * 0.35, 11);
            let n2 = smooth_noise(x as f32 * 0.9, y as f32 * 0.9, 23);
            // Base amarilla calida, aclarada por el ruido.
            let r = 255.0f32;
            let g = 224.0 + 24.0 * n;
            let b = 70.0 + 80.0 * n + 30.0 * n2;
            let i = ((y * N + x) * 4) as usize;
            sun[i] = r.min(255.0) as u8;
            sun[i + 1] = g.min(255.0) as u8;
            sun[i + 2] = b.min(255.0) as u8;
            sun[i + 3] = 255;
        }
    }
    write_png("assets/sun.png", &sun);

    // --- Luna: superficie gris con crateres (albedo del cubo). ---
    let mut moon = vec![0u8; (N * N * 4) as usize];
    let craters: [(f32, f32, f32); 5] = [
        (3.5, 4.5, 2.2),
        (11.0, 3.0, 1.6),
        (7.0, 9.5, 2.6),
        (13.0, 12.0, 1.4),
        (2.5, 12.5, 1.8),
    ];
    for y in 0..N {
        for x in 0..N {
            let n = smooth_noise(x as f32 * 0.5, y as f32 * 0.5, 7);
            let mut v = 170.0 + 26.0 * n; // gris base con grano
            for (cx, cy, cr) in craters {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d < cr {
                    // Fondo del crater oscuro, borde ligeramente claro.
                    v -= 46.0 * (1.0 - d / cr);
                } else if d < cr + 1.0 {
                    v += 12.0;
                }
            }
            let v = v.clamp(0.0, 255.0) as u8;
            let i = ((y * N + x) * 4) as usize;
            moon[i] = v;
            moon[i + 1] = v;
            moon[i + 2] = (v as f32 * 0.96) as u8;
            moon[i + 3] = 255;
        }
    }
    write_png("assets/moon.png", &moon);

    println!("gen_celestial: assets/sun.png y assets/moon.png ({N}x{N})");
}

fn write_png(path: &str, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut encoder = png::Encoder::new(BufWriter::new(file), N, N);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("cabecera PNG");
    writer.write_image_data(rgba).expect("datos PNG");
}
