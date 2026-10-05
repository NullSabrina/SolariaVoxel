//! # Modulo `math` — matematicas propias
//!
//! En Solaria Voxel escribimos nuestra propia matematica 3D en lugar de usar
//! una crate externa (como `glam`). Es mas trabajo, pero es la unica forma de
//! entender de verdad que hay debajo de la camara, las matrices y (mas adelante)
//! las transformaciones de los chunks.
//!
//! Convenios que usaremos en TODO el motor:
//!
//! * **Mano derecha (right-handed)**: +X derecha, +Y arriba, -Z hacia delante.
//!   Por eso "mirar al frente" es la direccion `(0, 0, -1)`. Esto encaja con
//!   Minecraft y con la convencion de wgpu (que usa NDC con Z en `[0, 1]`).
//! * **Matrices column-major**: igual que wgpu, Vulkan y OpenGL. Internamente
//!   guardamos una matriz 4x4 como 4 columnas de 4 `f32`, para que al enviarla
//!   a la GPU sea `to_cols_array()` directo, sin transponer nada.
//!
//! De momento exponemos [`Vec3`] y [`Mat4`]; iremos anadiendo `Vec2`, `Quat`,
//! `Aabb`, etc. a medida que el motor los necesite.

mod frustum;
mod mat4;
mod vec3;

pub use frustum::Frustum;
pub use mat4::Mat4;
pub use vec3::Vec3;
