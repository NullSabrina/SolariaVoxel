//! **Modelos de bloque JSON** (P1.5).
//!
//! Formato compatible con el subconjunto de Blockbench/Java Block Model que usa
//! Solaria: `parent`, `textures` (`#side` -> tile), `elements.from/to` y `faces`
//! con `uv` en pixels de la textura. Resuelve referencias `#` y hereda del padre
//! `cube` cuando el hijo no trae elementos.
//!
//! El mesh builder solo usa modelos para bloques **no cubicos**; los cubicos
//! siguen el camino rapido de atlas.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::atlas;
use super::block::Face;

/// Cara de un elemento de modelo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelFace {
    /// Tile del atlas.
    pub tile: u16,
    /// UV normalizada `[u0, v0, u1, v1]` dentro del tile.
    pub uv: [f32; 4],
}

/// Elemento cuboide del modelo, en unidades de bloque (0..16).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: [Option<ModelFace>; 6],
}

impl ModelElement {
    pub fn face(&self, face: Face) -> Option<ModelFace> {
        self.faces[face.index()]
    }
}

/// Modelo resuelto listo para el mesh builder.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockModel {
    pub elements: Vec<ModelElement>,
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    texture_size: Option<f32>,
    #[serde(default)]
    textures: BTreeMap<String, String>,
    #[serde(default)]
    elements: Vec<RawElement>,
}

#[derive(Deserialize)]
struct RawElement {
    from: [f32; 3],
    to: [f32; 3],
    #[serde(default)]
    faces: BTreeMap<String, RawFace>,
}

#[derive(Deserialize)]
struct RawFace {
    #[serde(default)]
    texture: Option<String>,
    #[serde(default)]
    uv: Option<[f32; 4]>,
}

impl BlockModel {
    /// Parsea un modelo y resuelve sus referencias de textura.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let raw: RawModel = serde_json::from_str(json)?;
        let size = raw.texture_size.unwrap_or(atlas::TILE as f32).max(1.0);
        let textures = raw.textures.clone();
        // El padre `cube` hereda su geometria si el hijo no define elementos.
        if raw.elements.is_empty() && raw.parent.as_deref() == Some("cube") {
            let all = textures
                .get("all")
                .cloned()
                .or_else(|| textures.get("particle").cloned())
                .unwrap_or_else(|| "0".to_string());
            let faces = std::array::from_fn(|_| {
                Some(ModelFace {
                    tile: resolve_texture(&all, &textures),
                    uv: [0.0, 0.0, 1.0, 1.0],
                })
            });
            return Ok(Self {
                elements: vec![ModelElement {
                    from: [0.0, 0.0, 0.0],
                    to: [16.0, 16.0, 16.0],
                    faces,
                }],
            });
        }
        let elements = raw
            .elements
            .iter()
            .map(|el| {
                let mut faces: [Option<ModelFace>; 6] = [None; 6];
                for (name, face) in &el.faces {
                    let Some(idx) = face_index(name) else {
                        continue;
                    };
                    let raw_tex = face
                        .texture
                        .clone()
                        .or_else(|| textures.get("all").cloned())
                        .unwrap_or_else(|| "0".to_string());
                    let tile = resolve_texture(&raw_tex, &textures);
                    let uv = face.uv.unwrap_or([0.0, 0.0, size, size]);
                    faces[idx] = Some(ModelFace {
                        tile,
                        uv: [
                            (uv[0] / size).clamp(0.0, 1.0),
                            (uv[1] / size).clamp(0.0, 1.0),
                            (uv[2] / size).clamp(0.0, 1.0),
                            (uv[3] / size).clamp(0.0, 1.0),
                        ],
                    });
                }
                ModelElement {
                    from: el.from,
                    to: el.to,
                    faces,
                }
            })
            .collect();
        Ok(Self { elements })
    }

    /// Modelo 3D de la antorcha (dos cubos: palo y llama), cargado una vez.
    pub fn torch() -> &'static Self {
        static TORCH: OnceLock<BlockModel> = OnceLock::new();
        TORCH.get_or_init(|| {
            Self::parse(include_str!("../../assets/models/solaria_torch.json"))
                .expect("el modelo JSON de la antorcha es valido")
        })
    }
}

fn face_index(name: &str) -> Option<usize> {
    Some(match name {
        "north" => Face::NegZ.index(),
        "south" => Face::PosZ.index(),
        "east" => Face::PosX.index(),
        "west" => Face::NegX.index(),
        "up" => Face::PosY.index(),
        "down" => Face::NegY.index(),
        _ => return None,
    })
}

fn resolve_texture(raw: &str, textures: &BTreeMap<String, String>) -> u16 {
    let mut key = raw.trim();
    let mut guard = 0;
    while let Some(next) = key.strip_prefix('#') {
        guard += 1;
        if guard > 8 {
            return 0;
        }
        match textures.get(next) {
            Some(v) => key = v,
            None => return 0,
        }
    }
    if let Ok(v) = key.parse::<u16>() {
        return v;
    }
    atlas::TILE_NAMES
        .iter()
        .position(|n| *n == key)
        .map(|i| i as u16)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_antorcha_es_un_modelo_3d_no_una_cruz() {
        let m = BlockModel::torch();
        assert_eq!(m.elements.len(), 2, "palo + llama");
        assert!(
            m.elements
                .iter()
                .all(|e| e.faces.iter().any(|f| f.is_some()))
        );
        assert_eq!(
            m.elements[0].face(Face::NegZ).unwrap().tile,
            8,
            "la antorcha usa el tile 8"
        );
    }

    #[test]
    fn resuelve_referencias_de_textura_y_parent_cube() {
        let cube = BlockModel::parse(
            r#"{
                "parent": "cube",
                "textures": {"all": "stone"},
                "elements": []
            }"#,
        )
        .unwrap();
        assert_eq!(cube.elements.len(), 1);
        assert_eq!(cube.elements[0].face(Face::PosY).unwrap().tile, 3);
    }

    #[test]
    fn las_uv_se_normalizan_al_tamano_de_textura() {
        let m = BlockModel::torch();
        let uv = m.elements[0].face(Face::NegZ).unwrap().uv;
        assert!(uv.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(uv[1] < uv[3], "v crece hacia abajo en la imagen");
    }
}
