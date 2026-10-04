//! Punto de entrada del ejecutable.
//!
//! Toda la logica vive en la libreria (`lib.rs`); `main` solo la arranca. Tener
//! una libreria separada permite escribir tests de integracion y mantiene el
//! binario minimo.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    solaria_voxel::run()
}
