//! # `World` (en el modulo `store`) — el mundo en memoria
//!
//! En v0.3.x el "mundo" no existia como tal: el `Renderer` guardaba **una**
//! columna (la central) y las vecinas eran mallas generadas y tiradas. Eso
//! tenia dos consecuencias que se veian feo:
//!
//! 1. Al cruzar de chunk se **regeneraba** todo desde la semilla.
//! 2. Los bordes entre columnas se mesheaban como si fueran aire, asi que
//!    aparecian **muros internos** en los saltos de altura.
//!
//! Aqui esta la solucion: un [`World`] que mantiene cargadas las columnas
//! alrededor del jugador, con **cache** (una columna ya generada no se vuelve a
//! generar) y consulta de vecinos a traves de fronteras de chunk. El mesher
//! ahora puede preguntar "¿que hay al otro lado?" y no dibuja muros internos.
//!
//! Nota: de momento la generacion es **sincrona** (en el hilo principal). La
//! generacion en hilos con `rayon` es el objetivo de v0.5.1 completo; aqui
//! dejamos la estructura lista (cola de peticiones) para anadirla encima.

use std::collections::HashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, Column, SECTION_COUNT, WORLD_HEIGHT};
use super::save::{ChunkPos, ChunkRecord};
use super::terrain::TerrainGenerator;

/// Un mundo vivo: columnas cargadas + cache + generador.
pub struct World {
    /// Generador de terreno (deterministico por semilla).
    generator: TerrainGenerator,
    /// Columnas cargadas, por posicion de chunk.
    columns: HashMap<ChunkPos, Column>,
    /// Posiciones de las columnas que el jugador ha **modificado** (para
    /// guardarlas y porque no hay que regenerarlas).
    modified: HashMap<ChunkPos, ChunkRecord>,
    /// Distancia de carga en chunks (radio, no diametro).
    view_radius: i32,
    /// Ultimo centro de carga (para no recalcular si no cambio).
    last_center: Option<ChunkPos>,
}

impl World {
    /// Crea un mundo para una semilla, restaurando los chunks editados que se
    /// hayan cargado de disco.
    pub fn new(seed: u32, view_radius: i32, restored: Vec<(ChunkPos, ChunkRecord)>) -> Self {
        let mut world = Self {
            generator: TerrainGenerator::new(seed),
            columns: HashMap::new(),
            modified: HashMap::new(),
            view_radius,
            last_center: None,
        };
        // Las columnas restauradas se marcan como modificadas y se aplican
        // encima del terreno generado cuando se carguen.
        for (pos, record) in restored {
            world.modified.insert(pos, record);
        }
        world
    }

    /// La semilla del mundo.
    pub fn seed(&self) -> u32 {
        self.generator.seed()
    }

    /// Radio de carga actual.
    pub fn view_radius(&self) -> i32 {
        self.view_radius
    }

    /// Coordenadas de mundo (en bloques) del origen de un chunk.
    pub fn chunk_origin(pos: ChunkPos) -> [f32; 3] {
        [
            (pos.x * CHUNK_SIZE as i32) as f32,
            0.0,
            (pos.z * CHUNK_SIZE as i32) as f32,
        ]
    }

    /// Convierte una posicion de mundo (bloques) a su chunk + coordenadas
    /// locales.
    pub fn world_to_local(world: [i32; 3]) -> (ChunkPos, [usize; 3]) {
        let pos = ChunkPos::new(
            world[0].div_euclid(CHUNK_SIZE as i32),
            world[2].div_euclid(CHUNK_SIZE as i32),
        );
        let local = [
            world[0].rem_euclid(CHUNK_SIZE as i32) as usize,
            world[1] as usize,
            world[2].rem_euclid(CHUNK_SIZE as i32) as usize,
        ];
        (pos, local)
    }

    /// ¿Esta cargada la columna en `pos`?
    pub fn is_loaded(&self, pos: ChunkPos) -> bool {
        self.columns.contains_key(&pos)
    }

    /// ¿La seccion `section` de la columna `pos` no tiene geometria que dibujar?
    /// (Si la columna no esta cargada, la tratamos como vacia.)
    pub fn section_is_empty(&self, pos: ChunkPos, section: usize) -> bool {
        self.columns
            .get(&pos)
            .is_none_or(|c| c.section_is_empty(section))
    }

    /// Lee un bloque en coordenadas de mundo. Devuelve `Air` si la columna no
    /// esta cargada o `y` esta fuera del mundo.
    pub fn get_block(&self, world: [i32; 3]) -> Block {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return Block::Air;
        }
        let (pos, local) = Self::world_to_local(world);
        match self.columns.get(&pos) {
            Some(column) => column.get(local[0], local[1], local[2]),
            // Fuera de lo cargado: tratamos como aire (no hay bloque).
            None => Block::Air,
        }
    }

    /// ¿Hay bloque solido en estas coordenadas de mundo?
    pub fn is_solid(&self, world: [i32; 3]) -> bool {
        self.get_block(world).is_solid()
    }

    /// Escribe un bloque en coordenadas de mundo. Marca el chunk como
    /// modificado. Devuelve `true` si se pudo (la columna debe estar cargada).
    pub fn set_block(&mut self, world: [i32; 3], block: Block) -> bool {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return false;
        }
        let (pos, local) = Self::world_to_local(world);
        let Some(column) = self.columns.get_mut(&pos) else {
            return false;
        };
        column.set(local[0], local[1], local[2], block);
        // Toda columna editada se guarda; registramos su chunk.
        self.modified
            .insert(pos, ChunkRecord::from_column(column, TERRAIN_SECTION));
        // La luz la recalcula el mundo entero justo despues (cruza chunks).
        true
    }

    /// Luz de **cielo** (0..15) de una celda, en coords de mundo.
    pub fn sky_light_at(&self, world: [i32; 3]) -> u8 {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return 0;
        }
        let (pos, local) = Self::world_to_local(world);
        match self.columns.get(&pos) {
            Some(column) => column.light_at(local[0], local[1], local[2]),
            None => 0,
        }
    }

    /// Luz de **bloque** (antorchas, 0..15) de una celda, en coords de mundo.
    pub fn block_light_at(&self, world: [i32; 3]) -> u8 {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return 0;
        }
        let (pos, local) = Self::world_to_local(world);
        match self.columns.get(&pos) {
            Some(column) => column.block_light_at(local[0], local[1], local[2]),
            None => 0,
        }
    }

    /// Luz total (cielo vs bloque) de una celda (0..15), en coords de mundo.
    pub fn light_at(&self, world: [i32; 3]) -> u8 {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return 0;
        }
        let (pos, local) = Self::world_to_local(world);
        match self.columns.get(&pos) {
            Some(column) => column.combined_light(local[0], local[1], local[2]),
            None => 0,
        }
    }

    /// ¿Esta el chunk en `pos` modificado por el jugador?
    pub fn is_modified(&self, pos: ChunkPos) -> bool {
        self.modified.contains_key(&pos)
    }

    /// Itera las posiciones de las columnas cargadas.
    pub fn loaded_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.columns.keys().copied()
    }

    /// Carga una columna: usa la version modificada si existe, si no, la genera.
    fn load_column(&mut self, pos: ChunkPos) {
        let origin = Self::chunk_origin(pos);
        let mut column = self
            .generator
            .generate_column(origin[0] as i32, origin[2] as i32);
        if let Some(record) = self.modified.get(&pos) {
            apply_record(&mut column, record);
        }
        // Calculamos la luz de cielo tras generar/restaurar la columna. La luz
        // de bloque se calcula a nivel de **mundo** (cruza chunks), no aqui.
        column.compute_skylight();
        self.columns.insert(pos, column);
    }

    /// Recalcula la **luz de cielo** con propagacion **lateral** (BFS a nivel de
    /// mundo, cruza chunks), pero solo en la **region** afectada.
    ///
    /// `dirty` son las columnas cuyos bloques cambiaron (o que entran/salen). Se
    /// reinicia su base columnar (15 hasta el primer solido) y se propaga la luz
    /// lateral desde las celdas de aire en sombra de la region (dirty + su anillo
    /// 3x3): con **cuevas y voladizos** hay aire *bajo un techo* que no ve el cielo
    /// y se ilumina de lado. Como la luz viaja **15 bloques** (< 1 chunk), la
    /// region cubre todo lo que puede cambiar; recalcularla entera costaba ~60 ms
    /// y se notaba al editar o al cruzar de chunk.
    pub fn recompute_skylight(&mut self, dirty: &[ChunkPos]) {
        use std::collections::VecDeque;

        const NEIGHBORS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];

        // 1. Base columnar de las columnas sucias (borra su luz lateral antigua).
        for pos in dirty {
            if let Some(column) = self.columns.get_mut(pos) {
                column.compute_skylight();
            }
        }

        // 2. Region a escanear: dirty + anillo 3x3 (las celdas en sombra que
        //    limitan con la luz suelen estar en las columnas vecinas).
        let mut region: Vec<ChunkPos> = Vec::new();
        for pos in dirty {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = ChunkPos::new(pos.x + dx, pos.z + dz);
                    if self.is_loaded(n) && !region.contains(&n) {
                        region.push(n);
                    }
                }
            }
        }

        // 3. Sembrar desde las celdas de aire en sombra que ya tocan luz.
        let mut seeds: Vec<(i32, i32, i32, u8)> = Vec::new();
        for pos in &region {
            let Some(column) = self.columns.get(pos) else {
                continue;
            };
            let bx = pos.x * CHUNK_SIZE as i32;
            let bz = pos.z * CHUNK_SIZE as i32;
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let surface = column.surface_y(x, z);
                    for y in 0..surface {
                        if column.get(x, y, z).is_solid() {
                            continue;
                        }
                        let (wx, wy, wz) = (bx + x as i32, y as i32, bz + z as i32);
                        let mut best = 0u8;
                        for (dx, dy, dz) in NEIGHBORS {
                            let ly = wy + dy;
                            if ly < 0 || ly >= WORLD_HEIGHT as i32 {
                                continue;
                            }
                            let lx = x as i32 + dx;
                            let lz = z as i32 + dz;
                            // Fuera de la columna se cruza al chunk vecino (lento);
                            // dentro se lee directo (rapido).
                            let l = if (0..CHUNK_SIZE as i32).contains(&lx)
                                && (0..CHUNK_SIZE as i32).contains(&lz)
                            {
                                column.light_at(lx as usize, ly as usize, lz as usize)
                            } else {
                                self.sky_light_at([bx + lx, ly, bz + lz])
                            };
                            let cand = if dy == -1 { l } else { l.saturating_sub(1) };
                            if l > 0 && cand > best {
                                best = cand;
                            }
                        }
                        if best > 0 {
                            seeds.push((wx, wy, wz, best));
                        }
                    }
                }
            }
        }

        // 3. Sembrar y propagar.
        let mut queue: VecDeque<(i32, i32, i32, u8)> = VecDeque::new();
        for (x, y, z, level) in seeds {
            let (pos, local) = Self::world_to_local([x, y, z]);
            if let Some(column) = self.columns.get_mut(&pos)
                && column.light_at(local[0], local[1], local[2]) < level
            {
                column.set_light(local[0], local[1], local[2], level);
                queue.push_back((x, y, z, level));
            }
        }
        while let Some((x, y, z, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            for (dx, dy, dz) in NEIGHBORS {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                if ny < 0 || ny >= WORLD_HEIGHT as i32 {
                    continue;
                }
                let cand = if dy == -1 { level } else { level - 1 };
                let (pos, local) = Self::world_to_local([nx, ny, nz]);
                let Some(column) = self.columns.get_mut(&pos) else {
                    continue;
                };
                if column.get(local[0], local[1], local[2]).is_solid() {
                    continue;
                }
                if column.light_at(local[0], local[1], local[2]) < cand {
                    column.set_light(local[0], local[1], local[2], cand);
                    queue.push_back((nx, ny, nz, cand));
                }
            }
        }
    }

    /// Recalcula la **luz de bloque** (antorchas) de todo el mundo cargado.
    ///
    /// A diferencia de la version por columna, este BFS **cruza chunks**: la luz
    /// de una antorcha cerca de un borde ilumina tambien la columna vecina, asi
    /// que no aparece un corte de luz en la frontera.
    pub fn recompute_block_light(&mut self) {
        use std::collections::VecDeque;

        // 1. Recolectar los emisores de las secciones no vacias (la mayoria de
        //    las 24 de una columna no tienen nada que emitir).
        let mut sources: Vec<(i32, i32, i32, u8)> = Vec::new();
        for (pos, column) in &self.columns {
            let bx = pos.x * CHUNK_SIZE as i32;
            let bz = pos.z * CHUNK_SIZE as i32;
            for section in 0..SECTION_COUNT {
                if column.section_is_empty(section) {
                    continue;
                }
                let y0 = section * CHUNK_SIZE;
                for y in y0..(y0 + CHUNK_SIZE).min(WORLD_HEIGHT) {
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            let e = column.get(x, y, z).light_emission();
                            if e > 0 {
                                sources.push((bx + x as i32, y as i32, bz + z as i32, e));
                            }
                        }
                    }
                }
            }
        }

        // 2. Limpiar y sembrar las fuentes.
        let mut queue: VecDeque<(i32, i32, i32, u8)> = VecDeque::new();
        for column in self.columns.values_mut() {
            column.clear_block_light();
        }
        for (x, y, z, e) in sources {
            let (pos, local) = Self::world_to_local([x, y, z]);
            if let Some(column) = self.columns.get_mut(&pos) {
                column.set_block_light(local[0], local[1], local[2], e);
                queue.push_back((x, y, z, e));
            }
        }

        // 3. Propagar a los 6 vecinos (en coordenadas de mundo, cruzando chunks).
        const NEIGHBORS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        while let Some((x, y, z, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            let next = level - 1;
            for (dx, dy, dz) in NEIGHBORS {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                if ny < 0 || ny >= WORLD_HEIGHT as i32 {
                    continue;
                }
                let (pos, local) = Self::world_to_local([nx, ny, nz]);
                let Some(column) = self.columns.get_mut(&pos) else {
                    continue;
                };
                if column.get(local[0], local[1], local[2]).is_solid() {
                    continue;
                }
                if column.block_light_at(local[0], local[1], local[2]) < next {
                    column.set_block_light(local[0], local[1], local[2], next);
                    queue.push_back((nx, ny, nz, next));
                }
            }
        }
    }

    /// Actualiza el conjunto de columnas cargadas alrededor del jugador:
    /// carga las que faltan y **descarga** las que quedan fuera del radio.
    ///
    /// Devuelve `(cargadas, descargadas)` para que el renderer sepa que mallas
    /// hay que regenerar. No hace nada si el centro no cambio.
    pub fn update_streaming(&mut self, player_pos: [f32; 3]) -> StreamChange {
        let center = ChunkPos::new(
            (player_pos[0] / CHUNK_SIZE as f32).floor() as i32,
            (player_pos[2] / CHUNK_SIZE as f32).floor() as i32,
        );
        if self.last_center == Some(center) {
            return StreamChange::default();
        }
        self.last_center = Some(center);

        // 1. Descargar lo que quede fuera del radio.
        let mut unloaded = Vec::new();
        self.columns.retain(|pos, _| {
            let inside = (pos.x - center.x).abs() <= self.view_radius
                && (pos.z - center.z).abs() <= self.view_radius;
            if !inside {
                unloaded.push(*pos);
            }
            inside
        });

        // 2. Cargar lo que falte.
        let mut loaded = Vec::new();
        for dz in -self.view_radius..=self.view_radius {
            for dx in -self.view_radius..=self.view_radius {
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                if !self.columns.contains_key(&pos) {
                    self.load_column(pos);
                    loaded.push(pos);
                }
            }
        }

        StreamChange { loaded, unloaded }
    }

    /// Devuelve un clon de la columna (para meshearla sin prestar `self`).
    pub fn column(&self, pos: ChunkPos) -> Option<&Column> {
        self.columns.get(&pos)
    }

    /// Todos los registros modificados, para guardar el mundo.
    pub fn modified_records(&self) -> &HashMap<ChunkPos, ChunkRecord> {
        &self.modified
    }
}

/// Seccion que contiene la superficie del terreno (y 64..80).
pub const TERRAIN_SECTION: usize = 4;

/// Aplica los bloques de un `ChunkRecord` a una columna (descomprime si hace
/// falta).
pub fn apply_record(column: &mut Column, record: &ChunkRecord) {
    let blocks = record.decompressed_blocks();
    let y0 = TERRAIN_SECTION * CHUNK_SIZE;
    let mut i = 0usize;
    for y in y0..(y0 + CHUNK_SIZE).min(WORLD_HEIGHT) {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if let Some(&id) = blocks.get(i) {
                    column.set(x, y, z, Block::from_u8(id));
                }
                i += 1;
            }
        }
    }
}

/// Cambios producidos por un tick de streaming.
#[derive(Default)]
pub struct StreamChange {
    pub loaded: Vec<ChunkPos>,
    pub unloaded: Vec<ChunkPos>,
}

impl StreamChange {
    /// ¿Hubo algun cambio?
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty() && self.unloaded.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargar_y_editar_en_cualquier_columna() {
        let mut world = World::new(7, 1, vec![]);
        world.update_streaming([0.0, 64.0, 0.0]);
        // 3x3 = 9 columnas cargadas.
        assert_eq!(world.loaded_positions().count(), 9);

        // Editamos un bloque en una columna vecina (chunk 1,0).
        let target = [CHUNK_SIZE as i32 + 2, 70, 3];
        assert!(!world.is_solid(target));
        assert!(world.set_block(target, Block::Stone));
        assert!(world.is_solid(target));
        assert!(world.is_modified(ChunkPos::new(1, 0)));
    }

    #[test]
    fn descarga_las_columnas_lejanas() {
        let mut world = World::new(7, 1, vec![]);
        world.update_streaming([0.0, 64.0, 0.0]);
        assert!(world.is_loaded(ChunkPos::new(0, 0)));
        // Nos movemos 5 chunks en X; (0,0) queda muy lejos.
        world.update_streaming([(CHUNK_SIZE * 5) as f32, 64.0, 0.0]);
        assert!(!world.is_loaded(ChunkPos::new(0, 0)));
        assert!(world.is_loaded(ChunkPos::new(5, 0)));
    }

    #[test]
    fn las_ediciones_sobreviven_a_descargar_y_recargar() {
        let mut world = World::new(7, 1, vec![]);
        world.update_streaming([0.0, 64.0, 0.0]);
        let target = [2, 70, 3];
        world.set_block(target, Block::Stone);
        // Nos alejamos (se descarga) y volvemos.
        world.update_streaming([(CHUNK_SIZE * 5) as f32, 64.0, 0.0]);
        world.update_streaming([0.0, 64.0, 0.0]);
        assert!(
            world.is_solid(target),
            "la edicion deberia persistir en memoria"
        );
    }

    #[test]
    fn world_to_local_maneja_coordenadas_negativas() {
        let (pos, local) = World::world_to_local([-1, 5, -17]);
        assert_eq!(pos, ChunkPos::new(-1, -2));
        assert_eq!(local[0], CHUNK_SIZE - 1); // -1 mod 16
        assert_eq!(local[2], CHUNK_SIZE - 1); // -17 mod 16
    }

    #[test]
    fn la_luz_de_antorcha_cruza_el_borde_de_chunk() {
        use super::super::block::Block;
        let mut world = World::new(7, 1, vec![]);
        world.update_streaming([0.0, 100.0, 0.0]); // chunks -1..1 en x y z

        // Antorcha en el ultimo bloque del chunk 0 (mundo x=15), bien alto para
        // que sea aire. El bloque vecino (mundo x=16) esta en el chunk 1.
        assert!(world.set_block([15, 100, 0], Block::Torch));
        world.recompute_block_light();
        assert_eq!(world.block_light_at([16, 100, 0]), 13, "no cruzo el borde");
        assert_eq!(world.block_light_at([20, 100, 0]), 9, "a 5 bloques");

        // Al quitar la antorcha, la luz se apaga tambien al otro lado.
        assert!(world.set_block([15, 100, 0], Block::Air));
        world.recompute_block_light();
        assert_eq!(world.block_light_at([16, 100, 0]), 0);
    }

    #[test]
    fn la_luz_de_cielo_se_propaga_bajo_un_techo() {
        use super::super::block::Block;
        let mut world = World::new(1, 0, vec![]);
        world.update_streaming([8.0, 200.0, 8.0]); // una columna (16x16)

        // A y=200 todo es aire (por encima del terreno). Ponemos:
        // - un techo solido en y=202 sobre x = 0..11,
        // - dejando x = 11..15 a cielo abierto.
        for z in 0..CHUNK_SIZE {
            for x in 0..11i32 {
                world.set_block([x, 202, z as i32], Block::Stone);
            }
        }
        let all: Vec<ChunkPos> = world.loaded_positions().collect();
        world.recompute_skylight(&all);

        // Bajo el techo, la luz solo puede entrar de lado desde las columnas
        // abiertas: mas cerca del borde x=11 hay mas luz.
        let cerca = world.sky_light_at([10, 200, 8]);
        let lejos = world.sky_light_at([0, 200, 8]);
        assert!(cerca > 0, "no llega luz de cielo bajo el techo");
        assert!(
            cerca > lejos,
            "la luz no se atenua con la distancia: cerca={cerca}, lejos={lejos}"
        );
        // Justo al lado de la columna abierta, casi luz plena.
        assert!(world.sky_light_at([11, 200, 8]) == 15);
        // Bajo el techo a cielo abierto (columna sin techo) la luz es 15.
        assert_eq!(world.sky_light_at([3, 203, 8]), 15);
    }

    #[test]
    fn bench_light_recompute() {
        let mut world = World::new(13_371, 4, vec![]);
        let t = std::time::Instant::now();
        world.update_streaming([8.0, 74.0, 20.0]);
        println!("carga 81 columnas: {:?}", t.elapsed());
        let all: Vec<ChunkPos> = world.loaded_positions().collect();
        let t = std::time::Instant::now();
        world.recompute_skylight(&all);
        println!("recompute_skylight (81 col): {:?}", t.elapsed());
        let mut dirty = Vec::new();
        for dz in -1..=1 {
            for dx in -1..=1 {
                dirty.push(ChunkPos::new(dx, dz));
            }
        }
        let t = std::time::Instant::now();
        world.recompute_skylight(&dirty);
        println!("recompute_skylight (3x3): {:?}", t.elapsed());
        let t = std::time::Instant::now();
        world.recompute_block_light();
        println!("recompute_block_light: {:?}", t.elapsed());
    }

    #[test]
    fn bench_cruce_de_chunk() {
        use super::super::greedy::greedy_section;
        let mut world = World::new(13_371, 4, vec![]);
        world.update_streaming([8.0, 74.0, 20.0]);
        let all: Vec<ChunkPos> = world.loaded_positions().collect();
        world.recompute_skylight(&all);
        world.recompute_block_light();

        // Cruce de chunk: el centro pasa a (1,1) -> entran/salen columnas.
        let t = std::time::Instant::now();
        let change = world.update_streaming([24.0, 74.0, 20.0]);
        println!(
            "update_streaming (gen): {:?} (+{} -{})",
            t.elapsed(),
            change.loaded.len(),
            change.unloaded.len()
        );

        // Columnas a re-meshear (dirty para luz): loaded + anillo 3x3.
        let mut dirty: Vec<ChunkPos> = change.loaded.clone();
        for pos in change.loaded.iter().chain(change.unloaded.iter()) {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = ChunkPos::new(pos.x + dx, pos.z + dz);
                    if world.is_loaded(n) && !dirty.contains(&n) {
                        dirty.push(n);
                    }
                }
            }
        }
        let t = std::time::Instant::now();
        world.recompute_skylight(&dirty);
        println!("skylight (region {} col): {:?}", dirty.len(), t.elapsed());
        let t = std::time::Instant::now();
        world.recompute_block_light();
        println!("block_light: {:?}", t.elapsed());

        // Meshing aproximado: greedy de las secciones no vacias de las columnas
        // a re-meshear (sin GPU).
        let t = std::time::Instant::now();
        let mut passes = 0;
        for pos in &dirty {
            let Some(column) = world.column(*pos) else {
                continue;
            };
            for section in 0..super::super::chunk::SECTION_COUNT {
                if column.section_is_empty(section) {
                    continue;
                }
                let _ = greedy_section(column, section, [0.0; 3]);
                passes += 1;
            }
        }
        println!(
            "greedy ({} secciones, {} col): {:?}",
            passes,
            dirty.len(),
            t.elapsed()
        );
    }
}
