//! Recetas de la mesa de crafteo (etapa 2).
//!
//! Modelo **creativo** (sin conteos de items): la rejilla 3x3 contiene
//! `Option<Block>` por celda y una receta casa cuando el conjunto de celdas
//! ocupadas coincide **exactamente** con su forma, tras normalizar (recortar
//! filas/columnas vacias, como en Minecraft). La posicion dentro de la rejilla
//! no importa; los bloques de mas invalidan la receta.
//!
//! Tomar el resultado asigna el bloque a la ranura activa de la hotbar y limpia
//! la rejilla (los materiales "se consumen").

use super::block::Block;

/// Una receta: celdas ocupadas en coordenadas relativas + bloque de salida.
pub struct Recipe {
    /// `(x, y, bloque)` con origen en la celda ocupada minima.
    pub cells: &'static [(i32, i32, Block)],
    /// Lo que se obtiene al tomar el resultado.
    pub out: Block,
}

/// Todas las recetas conocidas, en orden de prioridad.
pub const RECIPES: [Recipe; 2] = [
    // 1 madera (donde sea) -> tablones.
    Recipe {
        cells: &[(0, 0, Block::Wood)],
        out: Block::Planks,
    },
    // 2x2 de tablones (donde sea) -> mesa de crafteo.
    Recipe {
        cells: &[
            (0, 0, Block::Planks),
            (1, 0, Block::Planks),
            (0, 1, Block::Planks),
            (1, 1, Block::Planks),
        ],
        out: Block::CraftingTable,
    },
];

/// Normaliza una rejilla: celdas ocupadas relativas a su minimo, ordenadas por
/// `(y, x)` para poder comparar.
fn normalize(grid: &[Option<Block>; 9]) -> Vec<(i32, i32, Block)> {
    let mut cells: Vec<(i32, i32, Block)> = Vec::new();
    for (i, cell) in grid.iter().enumerate() {
        if let Some(b) = cell {
            cells.push(((i % 3) as i32, (i / 3) as i32, *b));
        }
    }
    if cells.is_empty() {
        return cells;
    }
    let min_x = cells.iter().map(|c| c.0).min().unwrap_or(0);
    let min_y = cells.iter().map(|c| c.1).min().unwrap_or(0);
    let mut out: Vec<(i32, i32, Block)> = cells
        .into_iter()
        .map(|(x, y, b)| (x - min_x, y - min_y, b))
        .collect();
    out.sort();
    out
}

/// Busca la receta que casa con la rejilla. `None` si esta vacia o no casa.
pub fn match_recipe(grid: &[Option<Block>; 9]) -> Option<Block> {
    let norm = normalize(grid);
    if norm.is_empty() {
        return None;
    }
    for recipe in RECIPES.iter() {
        let mut want: Vec<(i32, i32, Block)> = recipe.cells.to_vec();
        want.sort();
        if norm == want {
            return Some(recipe.out);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_with(cells: &[(usize, Block)]) -> [Option<Block>; 9] {
        let mut g = [None; 9];
        for (i, b) in cells {
            g[*i] = Some(*b);
        }
        g
    }

    #[test]
    fn una_madera_da_tablones_donde_este() {
        for i in 0..9 {
            let g = grid_with(&[(i, Block::Wood)]);
            assert_eq!(match_recipe(&g), Some(Block::Planks), "celda {i}");
        }
    }

    #[test]
    fn dos_por_dos_de_tablones_da_mesa_donde_este() {
        // Arriba-izquierda y abajo-derecha: la posicion no importa.
        for cells in [
            [
                (0, Block::Planks),
                (1, Block::Planks),
                (3, Block::Planks),
                (4, Block::Planks),
            ],
            [
                (4, Block::Planks),
                (5, Block::Planks),
                (7, Block::Planks),
                (8, Block::Planks),
            ],
        ] {
            let g = grid_with(&cells);
            assert_eq!(match_recipe(&g), Some(Block::CraftingTable));
        }
    }

    #[test]
    fn un_bloque_de_mas_invalida_la_receta() {
        let g = grid_with(&[(4, Block::Wood), (0, Block::Stone)]);
        assert_eq!(match_recipe(&g), None);
    }

    #[test]
    fn la_rejilla_vacia_no_da_nada() {
        assert_eq!(match_recipe(&[None; 9]), None);
    }

    #[test]
    fn tres_tablones_en_fila_no_dan_mesa() {
        let g = grid_with(&[(0, Block::Planks), (1, Block::Planks), (2, Block::Planks)]);
        assert_eq!(match_recipe(&g), None);
    }
}
