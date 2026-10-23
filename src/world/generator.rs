//! **Tipo de generador** de terreno: coexistencia entre el generador por etapas
//! actual (`Legacy16`), el grafo de densidad data-driven (`Graph`) y el pipeline
//! multi-capa de escala monumental (`Larion`).
//!
//! El tipo vive en los **metadatos** del mundo (`level.json`, campo
//! `generator_kind`), no en el binario: asi los mundos existentes (sin el campo)
//! siguen siendo `Legacy16` sin migrar el archivo, y no hay que tocar
//! `FORMAT_VERSION`/`GENERATOR_VERSION`.

/// Version del pipeline para los mundos **nuevos**. Los mundos existentes sin
/// `generator_config` en `level.json` se quedan en [`LEGACY_GENERATOR_CONFIG`].
///
/// * `2`: hidrologia P0.1 + acuiferos (v0.48-v0.51). Congelada: los mundos
///   creados entonces deben seguir generando los mismos chunks.
/// * `3`: arboles organicos, cuevas worm, ruido SIMD y `density_youth` (v0.52).
/// * `4`: biomas mas amplios/guiados por clima, relieve por bioma y agua
///   contenida (v0.53).
pub const CURRENT_GENERATOR_CONFIG: u32 = 4;

/// Version del pipeline historico (v0.48.0): conserva byte a byte la generacion
/// Larion anterior a P0.1.
pub const LEGACY_GENERATOR_CONFIG: u32 = 1;

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
    /// Desde el nombre guardado en `level.json`. Un nombre desconocido cae en
    /// `Legacy16` (compatibilidad con metadatos antiguos); para detectar
    /// metadatos invalidos sin degradar en silencio usa [`Self::try_from_name`].
    pub fn from_name(name: &str) -> Self {
        Self::try_from_name(name).unwrap_or_default()
    }

    /// Igual que [`Self::from_name`] pero distingue un nombre desconocido.
    pub fn try_from_name(name: &str) -> Option<Self> {
        match name {
            "legacy16" => Some(GeneratorKind::Legacy16),
            "graph" => Some(GeneratorKind::Graph),
            "larion" => Some(GeneratorKind::Larion),
            _ => None,
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
