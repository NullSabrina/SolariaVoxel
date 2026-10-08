//! i18n minimo (espanol por defecto, ingles). Tablas en codigo: un datapack
//! futuro puede moverlas a `assets/lang/*.json` sin cambiar a los consumidores.
//!
//! Las claves son estables (el `name` del registro de bloques, `cat.*`, `ui.*`).
//! Si falta una clave se devuelve la **propia clave**, visible en las pruebas.

use crate::world::{Block, registry};

/// Idioma de la interfaz.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    /// Espanol (por defecto).
    #[default]
    Es,
    /// Ingles.
    En,
}

impl Lang {
    /// Todos los idiomas, en orden de menu.
    pub const ALL: [Lang; 2] = [Lang::Es, Lang::En];

    /// Codigo corto (`es`/`en`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }

    /// El siguiente idioma (para un boton "ciclar idioma").
    pub fn next(self) -> Self {
        match self {
            Lang::Es => Lang::En,
            Lang::En => Lang::Es,
        }
    }
}

/// Traducciones al espanol.
const ES: &[(&str, &str)] = &[
    ("air", "Aire"),
    ("grass", "Hierba"),
    ("dirt", "Tierra"),
    ("stone", "Piedra"),
    ("sand", "Arena"),
    ("wood", "Madera de roble"),
    ("leaves", "Hojas"),
    ("torch", "Antorcha"),
    ("snow", "Nieve"),
    ("water", "Agua"),
    ("planks", "Tablones"),
    ("crafting_table", "Mesa de crafteo"),
    ("coarse_dirt", "Tierra agria"),
    ("gravel", "Grava"),
    ("podzol", "Podzol"),
    ("lava", "Lava"),
    ("obsidian", "Obsidiana"),
    ("cat.building", "Construccion"),
    ("cat.nature", "Naturaleza"),
    ("cat.decoration", "Decoracion"),
    ("cat.special", "Especiales"),
    ("ui.search", "Buscar"),
    ("ui.inventory", "Inventario creativo"),
    ("act.forward", "Adelante"),
    ("act.back", "Atrás"),
    ("act.left", "Izquierda"),
    ("act.right", "Derecha"),
    ("act.jump", "Saltar"),
    ("act.fly", "Volar"),
    ("act.inventory", "Inventario"),
];

/// Traducciones al ingles.
const EN: &[(&str, &str)] = &[
    ("air", "Air"),
    ("grass", "Grass"),
    ("dirt", "Dirt"),
    ("stone", "Stone"),
    ("sand", "Sand"),
    ("wood", "Oak wood"),
    ("leaves", "Leaves"),
    ("torch", "Torch"),
    ("snow", "Snow"),
    ("water", "Water"),
    ("planks", "Planks"),
    ("crafting_table", "Crafting table"),
    ("coarse_dirt", "Coarse dirt"),
    ("gravel", "Gravel"),
    ("podzol", "Podzol"),
    ("lava", "Lava"),
    ("obsidian", "Obsidian"),
    ("cat.building", "Building"),
    ("cat.nature", "Nature"),
    ("cat.decoration", "Decoration"),
    ("cat.special", "Special"),
    ("ui.search", "Search"),
    ("ui.inventory", "Creative inventory"),
    ("act.forward", "Forward"),
    ("act.back", "Back"),
    ("act.left", "Left"),
    ("act.right", "Right"),
    ("act.jump", "Jump"),
    ("act.fly", "Fly"),
    ("act.inventory", "Inventory"),
];

fn table(lang: Lang) -> &'static [(&'static str, &'static str)] {
    match lang {
        Lang::Es => ES,
        Lang::En => EN,
    }
}

/// Traduce una clave. Si falta, devuelve la propia clave (nunca panico).
pub fn translate(lang: Lang, key: &'static str) -> &'static str {
    for (k, v) in table(lang) {
        if *k == key {
            return v;
        }
    }
    key
}

/// Nombre visible de un bloque en el idioma dado.
pub fn block_name(lang: Lang, block: Block) -> &'static str {
    translate(lang, registry::definition(block).name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_bloque_colocable_tiene_nombre_en_ambos_idiomas() {
        for &b in registry::BlockRegistry::items() {
            for lang in Lang::ALL {
                let key = registry::definition(b).name;
                let name = translate(lang, key);
                assert_ne!(name, key, "falta traduccion de '{key}' en {lang:?}");
                assert!(!name.is_empty());
            }
        }
    }

    #[test]
    fn todas_las_categorias_tienen_nombre() {
        for cat in crate::world::CreativeCategory::ALL {
            for lang in Lang::ALL {
                assert_ne!(
                    translate(lang, cat.key()),
                    cat.key(),
                    "falta {} en {lang:?}",
                    cat.key()
                );
            }
        }
    }

    #[test]
    fn una_clave_desconocida_devuelve_la_clave() {
        assert_eq!(translate(Lang::Es, "no.existe"), "no.existe");
    }

    #[test]
    fn el_nombre_de_bloque_sigue_el_idioma() {
        assert_eq!(block_name(Lang::Es, Block::Stone), "Piedra");
        assert_eq!(block_name(Lang::En, Block::Stone), "Stone");
    }
}
