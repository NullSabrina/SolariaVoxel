//! **Tipo de generador** de terreno: coexistencia entre el generador por etapas
//! actual (`Legacy16`), el grafo de densidad data-driven (`Graph`) y el pipeline
//! multi-capa de escala monumental (`Larion`).
//!
//! El tipo vive en los **metadatos** del mundo (`level.json`, campo
//! `generator_kind`), no en el binario: asi los mundos existentes (sin el campo)
//! siguen siendo `Legacy16` sin migrar el archivo, y no hay que tocar
//! `FORMAT_VERSION`/`GENERATOR_VERSION`.

/// Generador de terreno de un mundo.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GeneratorKind {
    /// Generador por etapas actual (`world::worldgen`). Mundos existentes.
    #[default]
    Legacy16,
    /// Grafo de densidad data-driven (`world::worldgen::graph`).
    Graph,
    /// Pipeline multi-capa de escala monumental (`world::worldgen::larion`).
    Larion,
}

impl GeneratorKind {
    /// Desde el nombre guardado en `level.json` (desconocido -> `Legacy16`).
    pub fn from_name(name: &str) -> Self {
        match name {
            "graph" => GeneratorKind::Graph,
            "larion" => GeneratorKind::Larion,
            _ => GeneratorKind::Legacy16,
        }
    }

    /// Nombre estable (el que va a `level.json`).
    pub fn name(self) -> &'static str {
        match self {
            GeneratorKind::Legacy16 => "legacy16",
            GeneratorKind::Graph => "graph",
            GeneratorKind::Larion => "larion",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_va_y_vuelve() {
        assert_eq!(GeneratorKind::from_name("graph"), GeneratorKind::Graph);
        assert_eq!(
            GeneratorKind::from_name("legacy16"),
            GeneratorKind::Legacy16
        );
        assert_eq!(GeneratorKind::from_name("larion"), GeneratorKind::Larion);
        // Desconocido o vacio -> legacy (compatibilidad).
        assert_eq!(GeneratorKind::from_name(""), GeneratorKind::Legacy16);
        assert_eq!(GeneratorKind::from_name("otro"), GeneratorKind::Legacy16);
        assert_eq!(GeneratorKind::Graph.name(), "graph");
        assert_eq!(GeneratorKind::Legacy16.name(), "legacy16");
        assert_eq!(GeneratorKind::Larion.name(), "larion");
    }
}
