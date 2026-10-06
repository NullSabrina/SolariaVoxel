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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_VOLUME, Column, SECTION_COUNT, WORLD_HEIGHT};
use super::save::{ChunkPos, ChunkRecord};
use super::streaming::{GenResult, TerrainScheduler};
use super::terrain::TerrainGenerator;
use super::water::{self, Fluid, FluidBudget, FluidGrid, MAX_LEVEL};

/// Un mundo vivo: columnas cargadas + cache + generador.
pub struct World {
    /// Generador de terreno (deterministico por semilla), compartido con los
    /// workers de generacion (`Arc`, `Send + Sync`).
    generator: Arc<TerrainGenerator>,
    /// Columnas cargadas, por posicion de chunk. Van **boxeadas**: una `Column`
    /// pesa ~98 KB y moverla por valor (canal/collect/insert) desborda la pila
    /// del hilo principal en `debug`.
    columns: HashMap<ChunkPos, Box<Column>>,
    /// Posiciones de las columnas que el jugador ha **modificado** (para
    /// guardarlas y porque no hay que regenerarlas). El registro se reconstruye
    /// de forma **perezosa** (al guardar/descargar), no en cada `set_block`.
    modified: HashMap<ChunkPos, ChunkRecord>,
    /// Columnas cargadas con ediciones aun no volcadas a `modified`.
    dirty: HashSet<ChunkPos>,
    /// Distancia de carga en chunks (radio, no diametro).
    view_radius: i32,
    /// Ultimo centro de carga (para no recalcular si no cambio).
    last_center: Option<ChunkPos>,
    /// **Active set** del agua: celdas pendientes de simular, con deduplicacion.
    /// Solo entran celdas no cargadas no; y una celda en equilibrio (un oceano
    /// quieto) sale al procesarse, asi que no vuelve a encolarse. Los *niveles*
    /// de flujo ya no viven aqui: van por columna (`Column::flow_at`/`set_flow`),
    /// empaquetados en nibbles, lo que elimina el `HashMap<[i32;3], Fluid>`
    /// global (localidad de cache y memoria proporcional al agua que fluye).
    water_queue: water::DirtyQueue,
    /// Pool de generacion en hilos (se crea al primer streaming asincrono).
    scheduler: Option<TerrainScheduler>,
    /// Peticiones de generacion pendientes: `ChunkPos -> id`.
    pending: HashMap<ChunkPos, u64>,
    /// Siguiente id de peticion (monotonico).
    next_request: u64,
}

impl World {
    /// Crea un mundo para una semilla, restaurando los chunks editados que se
    /// hayan cargado de disco.
    pub fn new(seed: u32, view_radius: i32, restored: Vec<(ChunkPos, ChunkRecord)>) -> Self {
        let mut world = Self {
            generator: Arc::new(TerrainGenerator::new(seed)),
            columns: HashMap::new(),
            modified: HashMap::new(),
            dirty: HashSet::new(),
            view_radius,
            last_center: None,
            water_queue: water::DirtyQueue::new(),
            scheduler: None,
            pending: HashMap::new(),
            next_request: 0,
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

    /// ¿La columna de esta celda esta **cargada**? (modelo `Loaded`/`Unloaded`.)
    pub fn is_column_loaded(&self, world: [i32; 3]) -> bool {
        let (pos, _) = Self::world_to_local(world);
        self.columns.contains_key(&pos)
    }

    /// Estado explicito de una celda: cargada (con su bloque), no cargada o
    /// fuera del mundo. Es la consulta base del modelo `Loaded`/`Unloaded`.
    pub fn availability(&self, world: [i32; 3]) -> VoxelAvailability {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return VoxelAvailability::OutOfBounds;
        }
        let (pos, local) = Self::world_to_local(world);
        match self.columns.get(&pos) {
            Some(column) => VoxelAvailability::Loaded(column.get(local[0], local[1], local[2])),
            None => VoxelAvailability::Unloaded,
        }
    }

    /// Consulta para **fisica**: a diferencia de [`World::is_solid`], una columna
    /// aun no cargada se trata como **solida** (muro), para que el jugador no
    /// caiga al vacio mientras llega la generacion asincrona. Por debajo del
    /// mundo tambien es muro; por encima, aire.
    pub fn is_solid_or_unloaded(&self, world: [i32; 3]) -> bool {
        match self.availability(world) {
            VoxelAvailability::Loaded(block) => block.is_solid(),
            VoxelAvailability::Unloaded => true,
            VoxelAvailability::OutOfBounds => world[1] < 0,
        }
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
        // Luz de BLOQUE incremental: recalcula solo lo afectado por esta celda
        // (antes se hacia un `recompute_block_light` de todo el mundo cargado en
        // cada edicion -> pico de CPU).
        self.relight_block(world, block);
        // La columna queda sucia: el registro persistente se reconstruye al
        // guardar o al descargar (no aqui: comprimir 98 KB por bloque seria
        // carisimo). Asi el guardado captura tambien las ediciones por encima y
        // por debajo de la antigua seccion fija.
        self.dirty.insert(pos);
        // El agua: un bloque `Water` nuevo es fuente (flujo 0); cualquier otro
        // bloque borra el flujo previo de la celda. Ademas, los vecinos pueden
        // reaccionar (agua que cae a un hueco, etc.).
        if let Some(column) = self.columns.get_mut(&pos) {
            column.set_flow(local[0], local[1], local[2], 0);
        }
        self.enqueue_water(world);
        for d in NEIGHBORS6 {
            self.enqueue_water([world[0] + d[0], world[1] + d[1], world[2] + d[2]]);
        }
        // La luz de bloque ya se recalculo de forma incremental en
        // `relight_block`; el renderer re-meshea el area.
        true
    }

    /// Escribe la luz de bloque de una celda de mundo (0..15). No-op si la
    /// columna no esta cargada.
    fn put_block_light(&mut self, world: [i32; 3], level: u8) {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return;
        }
        let (pos, local) = Self::world_to_local(world);
        if let Some(column) = self.columns.get_mut(&pos) {
            column.set_block_light(local[0], local[1], local[2], level);
        }
    }

    /// Recalcula la **luz de bloque** de forma **incremental** alrededor de la
    /// edicion `(p -> new_block)`.
    ///
    /// 1. Apaga la luz que partia de `p` (cola de **remocion**, BFS): las celdas
    ///    que dependian de ella se oscurecen; las que tienen otra fuente se
    ///    re-siembran.
    /// 2. **Re-propagacion** (cola de adicion, solo sube): rellena desde las
    ///    celdas-frontera con luz y desde la fuente nueva si `new_block` emite.
    ///
    /// Es equivalente al recalculo global pero acotado al alcance de la luz
    /// (< 16 bloques), sin recorrer el mundo entero.
    fn relight_block(&mut self, p: [i32; 3], new_block: Block) {
        use std::collections::VecDeque;
        let old_light = self.block_light_at(p);
        let new_emission = new_block.light_emission();

        let mut remove: VecDeque<([i32; 3], u8)> = VecDeque::new();
        let mut add: VecDeque<([i32; 3], u8)> = VecDeque::new();

        self.put_block_light(p, 0);
        remove.push_back((p, old_light));

        // 1. Remocion.
        while let Some((c, level)) = remove.pop_front() {
            for d in NEIGHBORS6 {
                let n = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
                if n[1] < 0 || n[1] >= WORLD_HEIGHT as i32 {
                    continue;
                }
                let nl = self.block_light_at(n);
                if nl != 0 && nl < level {
                    self.put_block_light(n, 0);
                    remove.push_back((n, nl));
                } else if nl >= level {
                    add.push_back((n, nl));
                }
            }
        }

        // 2. Re-siembra desde los vecinos con luz (por si quedaron a oscuras
        //    celdas que otra fuente deberia iluminar de nuevo).
        for d in NEIGHBORS6 {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            let nl = self.block_light_at(n);
            if nl > 1 {
                add.push_back((n, nl));
            }
        }
        // Fuente nueva (antorcha/lava colocada).
        if new_emission > 0 {
            self.put_block_light(p, new_emission);
            add.push_back((p, new_emission));
        }

        // 3. Propagacion (solo sube).
        while let Some((c, level)) = add.pop_front() {
            if level <= 1 {
                continue;
            }
            for d in NEIGHBORS6 {
                let n = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
                if n[1] < 0 || n[1] >= WORLD_HEIGHT as i32 {
                    continue;
                }
                if self.get_block(n).is_solid() {
                    continue;
                }
                let next = level - 1;
                if self.block_light_at(n) < next {
                    self.put_block_light(n, next);
                    add.push_back((n, next));
                }
            }
        }
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

    /// ¿Esta el chunk en `pos` modificado por el jugador? (Incluye ediciones
    /// aun no volcadas a `modified`.)
    pub fn is_modified(&self, pos: ChunkPos) -> bool {
        self.modified.contains_key(&pos) || self.dirty.contains(&pos)
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
        self.columns.insert(pos, Box::new(column));
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

        // 1. Recolectar los emisores usando la **cache por columna** (evita
        //    escanear las 24 secciones de cada columna en cada cruce de chunk).
        let mut sources: Vec<(i32, i32, i32, u8)> = Vec::new();
        for (pos, column) in self.columns.iter_mut() {
            let bx = pos.x * CHUNK_SIZE as i32;
            let bz = pos.z * CHUNK_SIZE as i32;
            for &(idx, e) in column.emitters() {
                let x = (idx & 0x0F) as i32;
                let z = ((idx >> 4) & 0x0F) as i32;
                let y = (idx >> 8) as i32;
                sources.push((bx + x, y, bz + z, e));
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
    /// Centro de streaming (chunk del jugador).
    fn stream_center(player_pos: [f32; 3]) -> ChunkPos {
        ChunkPos::new(
            (player_pos[0] / CHUNK_SIZE as f32).floor() as i32,
            (player_pos[2] / CHUNK_SIZE as f32).floor() as i32,
        )
    }

    /// Descarga lo que sale del radio (volcando antes sus ediciones), cancela
    /// peticiones que ya no interesan y devuelve `(faltantes, descargadas)`.
    fn plan_center(&mut self, center: ChunkPos) -> (Vec<ChunkPos>, Vec<ChunkPos>) {
        // Guardar las ediciones pendientes ANTES de descargar.
        self.sync_modified();
        let r = self.view_radius;

        let mut unloaded = Vec::new();
        self.columns.retain(|pos, _| {
            let inside = (pos.x - center.x).abs() <= r && (pos.z - center.z).abs() <= r;
            if !inside {
                unloaded.push(*pos);
            }
            inside
        });
        // Descarta peticiones fuera del radio (su resultado se ignorara igual).
        self.pending
            .retain(|pos, _| (pos.x - center.x).abs() <= r && (pos.z - center.z).abs() <= r);

        let mut missing = Vec::new();
        for dz in -r..=r {
            for dx in -r..=r {
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                if !self.columns.contains_key(&pos) && !self.pending.contains_key(&pos) {
                    missing.push(pos);
                }
            }
        }
        (missing, unloaded)
    }

    /// Streaming **sincrono** (tests y usos que necesitan carga inmediata):
    /// genera las columnas faltantes en el propio hilo.
    pub fn update_streaming(&mut self, player_pos: [f32; 3]) -> StreamChange {
        let center = Self::stream_center(player_pos);
        if self.last_center == Some(center) {
            return StreamChange::default();
        }
        self.last_center = Some(center);
        let (missing, unloaded) = self.plan_center(center);
        let mut loaded = Vec::with_capacity(missing.len());
        for pos in missing {
            self.load_column(pos);
            loaded.push(pos);
        }
        StreamChange { loaded, unloaded }
    }

    /// Carga **sincrona forzada** de un area (arranque): cancela las peticiones
    /// async pendientes y genera todo el area en el hilo actual. Garantiza que el
    /// area del jugador esta completa antes del primer frame (con streaming async,
    /// si no, el suelo aun no existe y el jugador cae).
    pub fn warm_streaming(&mut self, player_pos: [f32; 3]) -> StreamChange {
        // Descarta lo que haya pedido el streaming async: sus resultados se
        // ignoraran (ya no estan en `pending`) y aqui lo cargamos todo en sync.
        self.pending.clear();
        self.last_center = None;
        self.update_streaming(player_pos)
    }

    /// Streaming **asincrono**: planifica y encola la generacion en los workers.
    /// Las columnas entran en frames posteriores via [`World::poll_generation`].
    /// El chunk del jugador se genera **ya** para no caer mientras llega el resto.
    pub fn plan_streaming(&mut self, player_pos: [f32; 3]) -> StreamChange {
        let center = Self::stream_center(player_pos);
        if self.last_center == Some(center) {
            return StreamChange::default();
        }
        self.last_center = Some(center);
        let (missing, unloaded) = self.plan_center(center);
        self.ensure_scheduler();
        let mut loaded = Vec::new();
        for pos in missing {
            if pos == center {
                self.load_column(pos);
                loaded.push(pos);
            } else {
                self.request_column(pos);
            }
        }
        StreamChange { loaded, unloaded }
    }

    /// Recoge columnas generadas por los workers. Valida que la peticion siga
    /// siendo la vigente (`id`) y que la columna siga dentro del radio; si no,
    /// descarta el resultado (revisiones / resultados obsoletos).
    pub fn poll_generation(&mut self) -> Vec<ChunkPos> {
        let results: Vec<GenResult> = match self.scheduler.as_ref() {
            Some(scheduler) => std::iter::from_fn(|| scheduler.try_recv()).collect(),
            None => Vec::new(),
        };
        let mut loaded = Vec::new();
        for result in results {
            if self.pending.get(&result.pos) != Some(&result.id) {
                continue; // obsoleto (se pidio otra vez o ya no interesa)
            }
            self.pending.remove(&result.pos);
            let inside = self.last_center.is_none_or(|c| {
                (result.pos.x - c.x).abs() <= self.view_radius
                    && (result.pos.z - c.z).abs() <= self.view_radius
            });
            if !inside || self.columns.contains_key(&result.pos) {
                continue;
            }
            let mut column = result.column;
            if let Some(record) = self.modified.get(&result.pos) {
                apply_record(&mut column, record);
            }
            column.compute_skylight();
            self.columns.insert(result.pos, column);
            loaded.push(result.pos);
        }
        loaded
    }

    /// Crea el pool de generacion la primera vez que se pide streaming async.
    fn ensure_scheduler(&mut self) {
        if self.scheduler.is_none() {
            self.scheduler = Some(TerrainScheduler::new(Arc::clone(&self.generator), 2));
        }
    }

    /// Encola la generacion de una columna con un id nuevo.
    fn request_column(&mut self, pos: ChunkPos) {
        let id = self.next_request;
        self.next_request += 1;
        if let Some(scheduler) = self.scheduler.as_ref()
            && scheduler.request(id, pos)
        {
            self.pending.insert(pos, id);
        }
    }

    /// Devuelve la columna (para meshearla sin prestar `self`).
    pub fn column(&self, pos: ChunkPos) -> Option<&Column> {
        self.columns.get(&pos).map(std::convert::AsRef::as_ref)
    }

    /// Reconstruye los registros persistentes de las columnas cargadas
    /// pendientes (dirty) y limpia la marca. Se llama antes de guardar y antes
    /// de descargar columnas.
    pub fn sync_modified(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let pending: Vec<ChunkPos> = self.dirty.iter().copied().collect();
        for pos in pending {
            if let Some(column) = self.columns.get(&pos) {
                let record = ChunkRecord::from_column(column);
                self.modified.insert(pos, record);
            }
        }
        self.dirty.clear();
    }

    /// Todos los registros modificados, para guardar el mundo.
    ///
    /// Llama antes a [`World::sync_modified`] si puede haber ediciones propias de
    /// este frame sin volcar.
    pub fn modified_records(&self) -> &HashMap<ChunkPos, ChunkRecord> {
        &self.modified
    }

    /// Informe de **memoria** del mundo por categorias (FASE 10). Recorre las
    /// columnas cargadas una vez. Sirve para decidir con datos si merece la pena
    /// bit-packing/cache en frio, y alimentara el overlay de diagnostico.
    pub fn memory_report(&self) -> super::memory::WorldMemory {
        let columns = self.columns.len();
        let block_per_column = SECTION_COUNT * CHUNK_VOLUME;
        let struct_per_column = std::mem::size_of::<Column>();
        let mut memory = super::memory::WorldMemory {
            columns,
            blocks_bytes: columns * block_per_column,
            struct_overhead_bytes: columns * struct_per_column.saturating_sub(block_per_column),
            ..Default::default()
        };
        for column in self.columns.values() {
            memory.skylight_bytes += column.skylight_bytes();
            memory.blocklight_bytes += column.blocklight_bytes();
            memory.fluid_bytes += column.fluid_bytes();
        }
        memory.modified_chunks = self.modified.len();
        memory.modified_bytes = self
            .modified
            .values()
            .map(super::memory::record_bytes)
            .sum();
        memory.water_queue_cells = self.water_queue.len();
        memory
    }

    /// Estado de agua de una celda del mundo (fuente, flujo con nivel, o nada).
    ///
    /// El nivel de flujo se lee de la columna (nibble empaquetado). Un bloque
    /// `Water` con flujo 0 es una **fuente**: no hay que almacenar el flag, la
    /// condicion "fuente" es exactamente "agua sin flujo".
    pub fn water_at(&self, world: [i32; 3]) -> Fluid {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return Fluid::None;
        }
        let (pos, local) = Self::world_to_local(world);
        let Some(column) = self.columns.get(&pos) else {
            return Fluid::None;
        };
        if column.get(local[0], local[1], local[2]) != Block::Water {
            return Fluid::None;
        }
        let level = column.flow_at(local[0], local[1], local[2]);
        if level == 0 {
            Fluid::Source
        } else {
            Fluid::Flow(level)
        }
    }

    /// Nivel de agua de una celda (0 si no hay).
    pub fn water_level(&self, world: [i32; 3]) -> u8 {
        self.water_at(world).level()
    }

    /// ¿Es una **fuente ya en equilibrio** que no hace falta simular? Ocurre
    /// cuando el fondo esta bloqueado o lleno y los 4 vecinos horizontales estan
    /// a tope. Asi los oceanos generados no cuestan CPU en el tick.
    fn water_in_equilibrium(&self, p: [i32; 3]) -> bool {
        if !self.water_at(p).is_source() {
            return false;
        }
        let below = [p[0], p[1] - 1, p[2]];
        let below_open = self.in_bounds(below)
            && !self.get_block(below).blocks_fluid()
            && self.water_level(below) < MAX_LEVEL;
        if below_open {
            return false;
        }
        for d in [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]] {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if !self.in_bounds(n) || self.get_block(n).blocks_fluid() {
                continue;
            }
            if self.water_level(n) < MAX_LEVEL - 1 {
                return false;
            }
        }
        true
    }

    /// Encela una celda de agua pendiente (si la columna esta cargada).
    fn enqueue_water(&mut self, world: [i32; 3]) {
        if !(0..WORLD_HEIGHT as i32).contains(&world[1]) {
            return;
        }
        let (pos, _) = Self::world_to_local(world);
        if self.columns.contains_key(&pos) {
            self.water_queue.push(world);
        }
    }

    /// Escribe el estado de agua de una celda **sin** marcarla como editada
    /// (la simulacion reescribe el bloque `Water`/`Air` a su gusto). El bloque y
    /// el nibble de flujo de la columna quedan coherentes: `Source` es flujo 0.
    fn set_water_raw(&mut self, world: [i32; 3], f: Fluid) {
        if world[1] < 0 || world[1] >= WORLD_HEIGHT as i32 {
            return;
        }
        let (pos, local) = Self::world_to_local(world);
        let Some(column) = self.columns.get_mut(&pos) else {
            return;
        };
        let (x, y, z) = (local[0], local[1], local[2]);
        match f {
            Fluid::None => {
                column.set_flow(x, y, z, 0);
                if column.get(x, y, z) == Block::Water {
                    column.set(x, y, z, Block::Air);
                }
            }
            Fluid::Source => {
                column.set_flow(x, y, z, 0);
                if column.get(x, y, z) != Block::Water {
                    column.set(x, y, z, Block::Water);
                }
            }
            Fluid::Flow(level) => {
                column.set_flow(x, y, z, level);
                if column.get(x, y, z) != Block::Water {
                    column.set(x, y, z, Block::Water);
                }
            }
        }
    }

    /// Cuantas celdas de agua hay pendientes en el **active set**. Es 0 cuando
    /// todo el agua esta en equilibrio (un oceano quieto): ese es el objetivo de
    /// coste cero por frame.
    pub fn pending_water_cells(&self) -> usize {
        self.water_queue.len()
    }

    /// Avanza la simulacion de agua hasta `budget` celdas (sin cota de tiempo).
    /// Devuelve las **secciones** que cambiaron (para re-meshearlas de forma
    /// incremental, no la columna entera).
    pub fn tick_water(&mut self, budget: usize) -> Vec<FluidDirty> {
        self.tick_water_with(FluidBudget {
            cells: budget,
            ms: f32::INFINITY,
        })
    }

    /// Avanza la simulacion de agua con un presupuesto de celdas **y** de tiempo.
    /// Devuelve las secciones sucias (con marcas de borde si el cambio toco un
    /// borde de chunk, para re-meshear tambien la columna vecina).
    pub fn tick_water_with(&mut self, budget: FluidBudget) -> Vec<FluidDirty> {
        let start = std::time::Instant::now();
        let mut dirty: Vec<FluidDirty> = Vec::new();
        let mut processed = 0usize;
        while processed < budget.cells {
            if start.elapsed().as_secs_f32() * 1000.0 >= budget.ms {
                break;
            }
            let Some(p) = self.water_queue.pop() else {
                break;
            };
            processed += 1;
            if !(0..WORLD_HEIGHT as i32).contains(&p[1]) {
                continue;
            }
            let (pos, local) = Self::world_to_local(p);
            if !self.columns.contains_key(&pos) {
                continue;
            }
            // Oceanos/fuentes en equilibrio: coste cero.
            if self.water_in_equilibrium(p) {
                continue;
            }
            if water::step_cell(self, p) {
                add_fluid_dirty(&mut dirty, pos, local);
                for n in water::neighborhood(p) {
                    self.enqueue_water(n);
                }
            }
        }
        dirty
    }
}

/// Registro de una **seccion** cuyo fluido cambio, con las marcas de borde
/// necesarias para re-meshear tambien la columna vecina. Se fusiona con la
/// entrada de la misma seccion (los bordes se acumulan con OR).
fn add_fluid_dirty(dirty: &mut Vec<FluidDirty>, pos: ChunkPos, local: [usize; 3]) {
    let section = local[1] / CHUNK_SIZE;
    let edge_x = local[0] == 0 || local[0] == CHUNK_SIZE - 1;
    let edge_z = local[2] == 0 || local[2] == CHUNK_SIZE - 1;
    for d in dirty.iter_mut() {
        if d.pos == pos && d.section == section {
            d.edge_x |= edge_x;
            d.edge_z |= edge_z;
            return;
        }
    }
    dirty.push(FluidDirty {
        pos,
        section,
        edge_x,
        edge_z,
    });
}

/// Los 6 vecinos ortogonales.
const NEIGHBORS6: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// El mundo actua como rejilla para la simulacion de agua.
impl FluidGrid for World {
    fn in_bounds(&self, p: [i32; 3]) -> bool {
        if !(0..WORLD_HEIGHT as i32).contains(&p[1]) {
            return false;
        }
        let (pos, _) = Self::world_to_local(p);
        self.columns.contains_key(&pos)
    }

    fn is_solid(&self, p: [i32; 3]) -> bool {
        // Para el agua, la lava cuenta como obstaculo (no fluye dentro).
        self.get_block(p).blocks_fluid()
    }

    fn fluid(&self, p: [i32; 3]) -> Fluid {
        self.water_at(p)
    }

    fn set_fluid(&mut self, p: [i32; 3], f: Fluid) {
        self.set_water_raw(p, f);
    }
}

/// Seccion que contiene la superficie del terreno (y 64..80).
pub const TERRAIN_SECTION: usize = 4;

/// Aplica los bloques (y los niveles de flujo) de un `ChunkRecord` a una columna
/// (descomprime si hace falta). Escribe `record.height` capas a partir de
/// `record.y0`, de modo que los registros migrados de v3 (una seccion en `y=64`)
/// y los nuevos (columna completa) conviven.
///
/// El campo `fluid` es opcional: si el registro no lo trae (v4 migrado o columna
/// sin agua que fluya), los niveles quedan a 0 = todo `Water` es **fuente**, que
/// es el comportamiento anterior.
pub fn apply_record(column: &mut Column, record: &ChunkRecord) {
    let blocks = record.decompressed_blocks();
    let fluid = record.decompressed_fluid();
    let y0 = record.y0 as usize;
    let y_end = (y0 + record.height as usize).min(WORLD_HEIGHT);
    let mut i = 0usize;
    for y in y0..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if let Some(&id) = blocks.get(i) {
                    column.set(x, y, z, Block::from_u8(id));
                }
                if let Some(&level) = fluid.get(i)
                    && level > 0
                {
                    column.set_flow(x, y, z, level);
                }
                i += 1;
            }
        }
    }
}

/// Estado de una celda consultada: **cargada** (con su bloque), **no cargada**, o
/// **fuera** de los limites verticales del mundo. Sustituye al ambiguo "no
/// cargado = aire": la fisica puede decidir (un chunk sin cargar es un muro
/// temporal) sin confundirlo con vacio real.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelAvailability {
    Loaded(Block),
    Unloaded,
    OutOfBounds,
}

/// Una **seccion** cuyo fluido cambio en un tick, con marcas de borde para
/// re-meshear la columna vecina solo cuando hace falta (remeshing incremental).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidDirty {
    pub pos: ChunkPos,
    pub section: usize,
    /// El cambio toca un borde X del chunk (afecta a la columna vecina en X).
    pub edge_x: bool,
    /// El cambio toca un borde Z del chunk (afecta a la columna vecina en Z).
    pub edge_z: bool,
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
    fn la_luz_de_bloque_incremental_coincide_con_el_global() {
        // La luz incremental debe dar el MISMO resultado que recalcular todo.
        let mut world = World::new(7, 2, vec![]);
        world.warm_streaming([8.0, 100.0, 8.0]); // chunk (0,0), 5x5 columnas
        let edits = [
            ([1, 100, 1], Block::Torch),
            ([14, 100, 14], Block::Torch),
            ([7, 100, 7], Block::Stone),
            ([7, 101, 7], Block::Stone),
            ([8, 100, 8], Block::Torch),
            ([8, 100, 8], Block::Air),
            ([1, 100, 1], Block::Air),
        ];
        for (p, b) in edits {
            assert!(world.set_block(p, b), "no se pudo editar {p:?}");
        }
        // Zona amplia (cruza bordes de chunk) donde la luz puede cambiar.
        let mut coords = Vec::new();
        for y in 96..106 {
            for z in -8..24 {
                for x in -8..24 {
                    coords.push([x, y, z]);
                }
            }
        }
        let incremental: Vec<u8> = coords.iter().map(|&c| world.block_light_at(c)).collect();
        world.recompute_block_light();
        let global: Vec<u8> = coords.iter().map(|&c| world.block_light_at(c)).collect();
        assert_eq!(
            incremental, global,
            "la luz incremental difiere del recalculo global"
        );
    }

    #[test]
    fn el_warm_streaming_carga_el_radio_completo_en_sync() {
        let mut world = World::new(7, 4, vec![]);
        let change = world.warm_streaming([0.0, 64.0, 0.0]);
        assert_eq!(change.loaded.len(), 81, "deberia cargar 9x9 columnas");
        assert_eq!(world.loaded_positions().count(), 81);
        // El suelo del spawn es solido y esta posado en un y razonable.
        assert!(world.is_solid([0, 60, 0]) || world.is_solid([0, 70, 0]));
    }

    #[test]
    fn la_fisica_trata_lo_no_cargado_como_solido() {
        let mut world = World::new(7, 1, vec![]);
        world.warm_streaming([0.0, 64.0, 0.0]);
        // Coordenada muy lejana: columna no cargada.
        let lejos = [9999, 32, 9999];
        assert!(!world.is_column_loaded(lejos));
        // Para fisica es un muro (no se cae); para posar/meshing es aire.
        assert!(world.is_solid_or_unloaded(lejos));
        assert!(!world.is_solid(lejos));
    }

    #[test]
    fn el_streaming_asincrono_carga_columnas() {
        let mut world = World::new(7, 4, vec![]);
        world.plan_streaming([0.0, 64.0, 0.0]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while world.loaded_positions().count() < 81 && std::time::Instant::now() < deadline {
            world.poll_generation();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            world.loaded_positions().count(),
            81,
            "no se cargaron las 81 columnas por streaming asincrono"
        );
    }

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

    #[test]
    fn el_agua_de_un_hueco_se_extiende_y_reporta_chunks_sucios() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        // Suelo de piedra a y=100 y una fuente de agua encima.
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        world.set_block([8, 101, 8], Block::Water);
        // El bloque de agua colocado es fuente (inagotable).
        assert!(world.water_at([8, 101, 8]).is_source());

        let mut dirty_total = 0usize;
        for _ in 0..120 {
            dirty_total += world.tick_water(100_000).len();
        }
        assert!(dirty_total > 0, "tick_water deberia reportar chunks sucios");
        // Se ha extendido por el suelo.
        assert!(
            world.water_at([9, 101, 8]).is_water(),
            "el agua no se extendio"
        );
        assert!(world.water_at([8, 101, 8]).is_source(), "la fuente sigue");
        // El nivel decrece al alejarse de la fuente.
        assert!(world.water_level([8, 101, 8]) > world.water_level([9, 101, 8]));
    }

    #[test]
    fn un_oceano_quieto_no_consume_cpu() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        // Oceano: suelo solido en y=100 y una capa de agua-fuente en y=101.
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
                world.set_block([x, 101, z], Block::Water);
            }
        }
        // Dejamos que el active set se vacie (las celdas en equilibrio salen al
        // procesarse y no se re-encolan).
        let mut ticks = 0;
        while world.pending_water_cells() > 0 && ticks < 500 {
            world.tick_water(100_000);
            ticks += 1;
        }
        assert_eq!(
            world.pending_water_cells(),
            0,
            "un oceano quieto no deberia tener celdas activas"
        );
        // Un tick mas no reporta ningun chunk sucio ni procesa nada visible.
        assert!(world.tick_water(100_000).is_empty());
        assert!(world.water_at([8, 101, 8]).is_source());
    }

    #[test]
    fn el_flujo_sobrevive_a_descargar_y_recargar_la_columna() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        world.set_block([8, 101, 8], Block::Water);
        for _ in 0..40 {
            world.tick_water(100_000);
        }
        // Una celda que fluye (no la fuente), con nivel > 0.
        let flow_cell = [9, 101, 8];
        let level = world.water_level(flow_cell);
        assert!(world.water_at(flow_cell).is_water());

        // Nos alejamos 6 chunks: la columna (0,0) se descarga (volcando su
        // registro, con fluido). Volvemos: debe restaurarse igual.
        world.update_streaming([(CHUNK_SIZE * 6) as f32, 64.0, 0.0]);
        assert!(!world.is_loaded(ChunkPos::new(0, 0)));
        world.update_streaming([8.0, 120.0, 8.0]);

        assert_eq!(
            world.water_level(flow_cell),
            level,
            "el nivel de flujo se perdio al descargar/recargar"
        );
        assert_eq!(world.water_at([8, 101, 8]), Fluid::Source);
    }

    #[test]
    fn el_tick_reporta_la_seccion_y_no_el_borde_si_es_interior() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        // Anillo solido que confina el agua lejos de los bordes del chunk, para
        // que toda celda sucia sea interior (x/z dentro de 5..11).
        for i in 4..=12i32 {
            world.set_block([i, 101, 4], Block::Stone);
            world.set_block([i, 101, 12], Block::Stone);
            world.set_block([4, 101, i], Block::Stone);
            world.set_block([12, 101, i], Block::Stone);
        }
        world.set_block([8, 101, 8], Block::Water);
        let dirty = world.tick_water(100_000);
        assert!(!dirty.is_empty(), "deberia haber secciones sucias");
        assert!(dirty.iter().all(|d| d.section == 101 / CHUNK_SIZE));
        assert_eq!(dirty[0].pos, ChunkPos::new(0, 0));
        assert!(
            dirty.iter().all(|d| !d.edge_x && !d.edge_z),
            "el agua confinada no deberia tocar bordes de chunk"
        );
    }

    #[test]
    fn el_tick_marca_el_borde_cuando_el_cambio_toca_un_borde_de_chunk() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        // Fuente pegada al borde x=0 del chunk (0,0).
        world.set_block([0, 101, 8], Block::Water);
        let dirty = world.tick_water(100_000);
        assert!(dirty.iter().any(|d| d.edge_x), "deberia marcar el borde X");
    }

    #[test]
    fn el_presupuesto_de_celdas_limita_el_tick() {
        use super::super::block::Block;
        use super::super::water::FluidBudget;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        world.set_block([8, 101, 8], Block::Water);
        let before = world.pending_water_cells();
        assert!(before > 0, "deberia haber celdas activas tras editar");
        // 0 celdas: el tick no procesa nada y la cola queda intacta.
        let dirty = world.tick_water_with(FluidBudget {
            cells: 0,
            ms: f32::INFINITY,
        });
        assert!(dirty.is_empty());
        assert_eq!(world.pending_water_cells(), before, "no se consumio nada");
    }

    #[test]
    fn la_disponibilidad_distingue_cargado_no_cargado_y_fuera() {
        let mut world = World::new(7, 1, vec![]);
        world.update_streaming([0.0, 64.0, 0.0]);
        assert!(matches!(
            world.availability([0, 60, 0]),
            VoxelAvailability::Loaded(_)
        ));
        assert_eq!(
            world.availability([9999, 60, 9999]),
            VoxelAvailability::Unloaded
        );
        assert_eq!(
            world.availability([0, -1, 0]),
            VoxelAvailability::OutOfBounds
        );
        assert_eq!(
            world.availability([0, WORLD_HEIGHT as i32, 0]),
            VoxelAvailability::OutOfBounds
        );
        // Fisica: no cargado y debajo del mundo son muro; por encima, aire.
        assert!(world.is_solid_or_unloaded([9999, 60, 9999]));
        assert!(world.is_solid_or_unloaded([0, -1, 0]));
        assert!(!world.is_solid_or_unloaded([0, WORLD_HEIGHT as i32, 0]));
    }

    #[test]
    fn el_reporte_de_memoria_cuadra_por_categorias() {
        use super::super::block::Block;
        let mut world = World::new(7, 1, vec![]);
        world.warm_streaming([0.0, 100.0, 0.0]);
        let r = world.memory_report();
        assert_eq!(r.columns, world.loaded_positions().count());
        assert_eq!(
            r.blocks_bytes,
            r.columns * SECTION_COUNT * crate::world::CHUNK_VOLUME
        );
        assert!(r.total_bytes() >= r.blocks_bytes);
        // La luz de cielo siempre esta reservada para las columnas cargadas.
        assert_eq!(
            r.skylight_bytes,
            r.columns * CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT
        );
        // Editar crea un registro modificado con payload.
        world.set_block([1, 100, 1], Block::Stone);
        world.sync_modified();
        let r2 = world.memory_report();
        assert_eq!(r2.modified_chunks, 1);
        assert!(r2.modified_bytes > 0);
    }

    #[test]
    fn el_agua_no_fluye_dentro_de_la_lava() {
        use super::super::block::Block;
        let mut world = World::new(7, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        // Suelo de piedra y un canal: fuente de agua en x=8, lava en x=5.
        for z in 7..=9i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        world.set_block([5, 101, 8], Block::Lava);
        world.set_block([8, 101, 8], Block::Water);
        for _ in 0..60 {
            world.tick_water(100_000);
        }
        // El agua llega hasta al lado, pero la celda de lava sigue intacta
        // (bloque y sin fluido de agua dentro).
        assert_eq!(world.get_block([5, 101, 8]), Block::Lava);
        assert_eq!(world.water_at([5, 101, 8]), Fluid::None);
        assert!(
            world.water_at([6, 101, 8]).is_water() || world.water_at([4, 101, 8]).is_water(),
            "el agua deberia acercarse a la lava"
        );
    }

    #[test]
    fn bench_tick_agua() {
        use super::super::block::Block;
        let mut world = World::new(1, 0, vec![]);
        world.update_streaming([8.0, 120.0, 8.0]);
        for z in 0..CHUNK_SIZE as i32 {
            for x in 0..CHUNK_SIZE as i32 {
                world.set_block([x, 100, z], Block::Stone);
            }
        }
        world.set_block([8, 101, 8], Block::Water);
        let t = std::time::Instant::now();
        let mut cells = 0usize;
        for _ in 0..100 {
            cells += world.tick_water(100_000).len();
        }
        println!(
            "[agua] 100 ticks de una charca 16x16: {:?} ({} chunk-updates)",
            t.elapsed(),
            cells
        );
        assert!(world.water_at([9, 101, 8]).is_water());
    }
}
