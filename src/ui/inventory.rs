//! Logica pura del **inventario creativo**: categorias, busqueda y filtrado.
//!
//! Sin GPU y sin estado de ventana: recibe el idioma y la consulta, y devuelve la
//! lista de bloques. `engine::app` y `render` solo lo consumen.

use crate::ui::lang::{Lang, block_name};
use crate::world::registry::{self, BlockRegistry};
use crate::world::{Block, CreativeCategory};

/// Pliega una cadena para comparar: minusculas y sin diacriticos
/// (`Café` -> `cafe`, `Ñ` -> `n`), de modo que "pie" encuentre "Piedra" y
/// "tablon" encuentre "Tablones".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        for lc in c.to_lowercase() {
            out.push(lc);
        }
    }
    out
}

/// ¿El nombre `name` casa con la `query`? Vacia = todo.
pub fn matches(query: &str, name: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    fold(name).contains(&fold(query))
}

/// Bloques colocables de una categoria (sin busqueda).
pub fn category_items(cat: CreativeCategory) -> Vec<Block> {
    BlockRegistry::items_in(cat)
}

/// Busca en **todos** los colocables por su nombre visible en `lang`.
pub fn search(lang: Lang, query: &str) -> Vec<Block> {
    registry::BlockRegistry::items()
        .iter()
        .copied()
        .filter(|b| matches(query, block_name(lang, *b)))
        .collect()
}

/// Vista del inventario: si `query` no esta vacia, busca en todo; si no, muestra
/// la categoria activa.
pub fn view(lang: Lang, cat: CreativeCategory, query: &str) -> Vec<Block> {
    if query.trim().is_empty() {
        category_items(cat)
    } else {
        search(lang, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_quita_tildes_y_minusculiza() {
        assert_eq!(fold("Café"), "cafe");
        assert_eq!(fold("Ñandú"), "nandu");
        assert_eq!(fold("PIEDRA"), "piedra");
    }

    #[test]
    fn matches_es_insensible_a_mayusculas_y_tildes() {
        assert!(matches("pie", "Piedra"));
        assert!(matches("PIE", "piedra"));
        assert!(matches("", "lo que sea"));
        assert!(!matches("xyz", "Piedra"));
    }

    #[test]
    fn la_busqueda_encuentra_por_nombre_visible() {
        let es = search(Lang::Es, "piedra");
        assert!(es.contains(&Block::Stone));
        // "tierra" casa con Tierra y Tierra agria.
        let tierras = search(Lang::Es, "tierra");
        assert!(tierras.contains(&Block::Dirt));
        assert!(tierras.contains(&Block::CoarseDirt));
        // Insensible a mayusculas.
        assert!(search(Lang::En, "STONE").contains(&Block::Stone));
    }

    #[test]
    fn la_vista_usa_categoria_o_busqueda() {
        let building = view(Lang::Es, CreativeCategory::Building, "");
        assert!(building.contains(&Block::Stone));
        assert!(!building.contains(&Block::Water));
        // Con busqueda, ignora la categoria.
        let found = view(Lang::Es, CreativeCategory::Building, "agua");
        assert!(found.contains(&Block::Water));
    }

    #[test]
    fn las_categorias_reparten_todos_los_items() {
        let mut total = 0;
        for cat in CreativeCategory::ALL {
            total += category_items(cat).len();
        }
        assert_eq!(total, BlockRegistry::items().len());
    }
}
