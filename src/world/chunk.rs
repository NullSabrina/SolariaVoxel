//! El mundo se organiza en **columnas** verticales.
//!
//! * Un [`Chunk`] es una seccion cubica de 16x16x16 bloques (la unidad de
//!   meshing y, en el futuro, la unidad de carga/descarga).
//! * Una [`Column`] apila `SECTION_COUNT` secciones hasta `WORLD_HEIGHT` (384)
//!   bloques de alto, como en Minecraft.
//!
//! La clave de rendimiento de v0.2.1: la mayoria de las 24 secciones de una
//! columna estan **vacias** (todo aire), asi que no generamos ni dibujamos su
//! geometria.

use std::array;

use super::block::Block;
use super::water::MAX_LEVEL;

/// Lado de una seccion, en bloques.
pub const CHUNK_SIZE: usize = 16;

/// Numero de bloques de una seccion (16^3 = 4096).
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Altura total del mundo, en bloques (16 x 24).
pub const WORLD_HEIGHT: usize = 384;

/// Cuantas secciones tiene una columna (384 / 16 = 24).
pub const SECTION_COUNT: usize = WORLD_HEIGHT / CHUNK_SIZE;

/// Bytes del almacen de **niveles de flujo** de una columna: un nibble (4 bits)
/// por celda (`MAX_LEVEL = 8` cabe de sobra). `16x16x384 / 2`.
pub const FLUID_NIBBLE_BYTES: usize = CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT / 2;

/// Una seccion de 16x16x16 bloques.
pub struct Chunk {
    blocks: [Block; CHUNK_VOLUME],
}

impl Chunk {
    /// Una seccion vacia (todo aire).
    pub fn empty() -> Self {
        Self {
            blocks: [Block::Air; CHUNK_VOLUME],
        }
    }

    /// Indice plano del bloque `(x, y, z)`. `x` varia mas rapido.
    #[inline]
    pub fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }

    /// Lee un bloque dentro de la seccion.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.blocks[Self::index(x, y, z)]
    }

    /// Escribe un bloque dentro de la seccion.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.blocks[Self::index(x, y, z)] = block;
    }

    /// ¿Esta seccion no tiene **nada dibujable** (ni solido ni visible)? Sirve
    /// para saltarnos secciones sin geometria al generar la malla. Contamos los
    /// visibles-no-solidos (la antorcha): una seccion con solo una antorcha no
    /// esta vacia.
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|b| !b.is_solid() && !b.is_visible())
    }
}

/// Niveles de luz, de 0 (oscuridad) a 15 (cielo despejado).
pub const MAX_LIGHT: u8 = 15;

/// Una columna del mundo: `SECTION_COUNT` secciones apiladas.
pub struct Column {
    sections: [Chunk; SECTION_COUNT],
    /// Luz de cielo por bloque (0..15), en el mismo orden que los bloques.
    /// Guardamos `u8` por celda: 16x16x384 = ~98 KB por columna. En v0.12.x
    /// pasaremos a un buffer de 4 bits por celda (mitad de memoria).
    light: Vec<u8>,
    /// Luz de bloque (antorchas, 0..15), propagada con un flood-fill.
    block_light: Vec<u8>,
    /// Altura del primer bloque solido de cada columna vertical `(x, z)`, para
    /// localizar rapido el aire en sombra (cuevas/voladizos) al propagar la luz
    /// de cielo lateralmente. Indice `z * CHUNK_SIZE + x`.
    surface: [u16; CHUNK_SIZE * CHUNK_SIZE],
    /// Nivel de flujo del agua por celda (`0` = sin flujo; `1..=MAX_LEVEL`),
    /// empaquetado en **nibbles**. Sustituye al antiguo `HashMap<[i32;3], Fluid>`
    /// global: el estado del fluido vive con la columna (localidad de cache) y
    /// va **disperso**: solo se reserva cuando hay agua que fluye. Un oceano
    /// (todo `Block::Water` a nivel de fuente) no reserva ni un byte. La
    /// condicion "fuente" no se guarda: se infiere (`Block::Water` con flujo 0).
    /// Indice `(y * CHUNK_SIZE + z) * CHUNK_SIZE + x`, como la luz.
    fluid: Option<Box<[u8]>>,
}

impl Column {
    /// Una columna vacia.
    pub fn empty() -> Self {
        Self {
            sections: array::from_fn(|_| Chunk::empty()),
            light: vec![0; CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT],
            block_light: vec![0; CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT],
            surface: [0; CHUNK_SIZE * CHUNK_SIZE],
            fluid: None,
        }
    }

    /// Indice plano de la luz.
    #[inline]
    fn light_index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }

    /// Luz de cielo en una posicion (0..15).
    #[inline]
    pub fn light_at(&self, x: usize, y: usize, z: usize) -> u8 {
        self.light[Self::light_index(x, y, z)]
    }

    /// Ajusta la luz de cielo en una posicion.
    #[inline]
    pub fn set_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        self.light[Self::light_index(x, y, z)] = level.min(MAX_LIGHT);
    }

    /// Calcula la **luz de cielo** de toda la columna.
    ///
    /// En v0.6.0 es una version simplificada (sin propagacion lateral aun):
    /// * Un bloque no solido **a cielo abierto** (por encima de la primera cosa
    ///   solida de su columna vertical) recibe luz 15.
    /// * Todo lo que queda bajo la superficie recibe 0.
    ///
    /// Esto ya da el efecto buscado (superficie iluminada, subsuelo a oscuras).
    /// La propagacion suave de la luz por las caras (flood fill 3D) y la luz de
    /// bloque (antorchas) llegan en v0.6.1/v0.6.2. Por eso el metodo esta
    /// separado: mas adelante lo sustituimos sin tocar el resto.
    pub fn compute_skylight(&mut self) {
        // Por defecto, todo a cielo abierto (15): es un `memset`. Solo hay que
        // **borrar** la parte bajo el primer solido, que son ~70 celdas por
        // columna en vez de marcar las ~312 abiertas. Mucho menos trabajo.
        self.light.fill(MAX_LIGHT);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                // Primer solido de arriba hacia abajo.
                let mut y = WORLD_HEIGHT;
                while y > 0 {
                    y -= 1;
                    if self.get(x, y, z).is_solid() {
                        break;
                    }
                }
                // `y` es el primer solido (o 0 si la columna esta vacia).
                if self.get(x, y, z).is_solid() {
                    for yy in 0..=y {
                        self.light[Self::light_index(x, yy, z)] = 0;
                    }
                    self.surface[z * CHUNK_SIZE + x] = y as u16;
                } else {
                    self.surface[z * CHUNK_SIZE + x] = 0;
                }
            }
        }
    }

    /// Altura del primer bloque solido de la columna vertical `(x, z)` (0 si la
    /// columna esta vacia). Es el borde de la sombra de cielo.
    #[inline]
    pub fn surface_y(&self, x: usize, z: usize) -> usize {
        self.surface[z * CHUNK_SIZE + x] as usize
    }

    /// Escribe el nibble bajo/alto correspondiente al indice plano `idx`.
    #[inline]
    fn write_nibble(buf: &mut [u8], idx: usize, level: u8) {
        let byte = &mut buf[idx >> 1];
        let level = level & 0x0F;
        if idx & 1 == 0 {
            *byte = (*byte & 0xF0) | level;
        } else {
            *byte = (*byte & 0x0F) | (level << 4);
        }
    }

    /// Nivel de **flujo** de agua en una celda (0 = sin flujo). No distingue si
    /// el bloque es agua: eso lo decide quien consulta (`World::water_at`).
    #[inline]
    pub fn flow_at(&self, x: usize, y: usize, z: usize) -> u8 {
        let Some(buf) = self.fluid.as_deref() else {
            return 0;
        };
        let idx = Self::light_index(x, y, z);
        let byte = buf[idx >> 1];
        if idx & 1 == 0 {
            byte & 0x0F
        } else {
            (byte >> 4) & 0x0F
        }
    }

    /// Fija el nivel de flujo de una celda (`0` lo borra). Reserva el almacen de
    /// nibbles **solo** si se escribe un nivel distinto de cero: un oceano de
    /// fuentes no reserva nada (su "fuente" se infiere del bloque).
    #[inline]
    pub fn set_flow(&mut self, x: usize, y: usize, z: usize, level: u8) {
        let level = level.min(MAX_LEVEL);
        let idx = Self::light_index(x, y, z);
        match &mut self.fluid {
            Some(buf) => Self::write_nibble(buf, idx, level),
            None if level != 0 => {
                let mut buf = vec![0u8; FLUID_NIBBLE_BYTES].into_boxed_slice();
                Self::write_nibble(&mut buf, idx, level);
                self.fluid = Some(buf);
            }
            None => {}
        }
    }

    /// ¿La columna reservo ya su almacen de flujo? Sirve para saber si merece la
    /// pena iterarlo al persistir/limpiar.
    #[inline]
    pub fn has_flow_storage(&self) -> bool {
        self.fluid.is_some()
    }

    /// Luz de bloque (antorchas) en una posicion (0..15).
    #[inline]
    pub fn block_light_at(&self, x: usize, y: usize, z: usize) -> u8 {
        self.block_light[Self::light_index(x, y, z)]
    }

    /// Ajusta la luz de bloque en una posicion (la usa el BFS del mundo).
    #[inline]
    pub fn set_block_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        self.block_light[Self::light_index(x, y, z)] = level.min(MAX_LIGHT);
    }

    /// Pone a cero la luz de bloque de toda la columna (antes de recomputarla).
    pub fn clear_block_light(&mut self) {
        self.block_light.fill(0);
    }

    /// Luz total que recibe una celda: el **maximo** de cielo y de bloque. Es lo
    /// que finalmente se dibuja (una antorcha ilumina una cueva a oscuras).
    #[inline]
    pub fn combined_light(&self, x: usize, y: usize, z: usize) -> u8 {
        self.light_at(x, y, z).max(self.block_light_at(x, y, z))
    }

    /// Calcula la **luz de bloque** con un flood-fill (BFS) desde cada bloque
    /// que emite luz. La luz pierde 1 por cada paso y no atraviesa bloques
    /// solidos.
    ///
    /// Usamos BFS (cola) en lugar de DFS para que la propagacion sea uniforme:
    /// cada celda se visita una sola vez con su nivel mas alto.
    pub fn compute_block_light(&mut self) {
        use std::collections::VecDeque;

        self.block_light.fill(0);
        let mut queue: VecDeque<(usize, usize, usize, u8)> = VecDeque::new();

        // Fuentes.
        for y in 0..WORLD_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let emission = self.get(x, y, z).light_emission();
                    if emission > 0 {
                        let idx = Self::light_index(x, y, z);
                        self.block_light[idx] = emission;
                        queue.push_back((x, y, z, emission));
                    }
                }
            }
        }

        // Propagacion a los 6 vecinos.
        while let Some((x, y, z, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            let next = level - 1;
            let neighbors: [(i32, i32, i32); 6] = [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ];
            for (dx, dy, dz) in neighbors {
                let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                if nx < 0 || ny < 0 || nz < 0 {
                    continue;
                }
                let (nx, ny, nz) = (nx as usize, ny as usize, nz as usize);
                if nx >= CHUNK_SIZE || nz >= CHUNK_SIZE || ny >= WORLD_HEIGHT {
                    continue;
                }
                // La luz no atraviesa bloques solidos.
                if self.get(nx, ny, nz).is_solid() {
                    continue;
                }
                let idx = Self::light_index(nx, ny, nz);
                if self.block_light[idx] < next {
                    self.block_light[idx] = next;
                    queue.push_back((nx, ny, nz, next));
                }
            }
        }
    }

    /// Luz de cielo en coordenadas que pueden salirse de la columna. Fuera
    /// devolvemos 0 (oscuridad).
    #[inline]
    pub fn light_or_zero(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 {
            return 0;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return 0;
        }
        self.light_at(x, y, z)
    }

    /// Luz de bloque (antorchas) en coordenadas que pueden salirse. Fuera, 0.
    #[inline]
    pub fn block_light_or_zero(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 {
            return 0;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return 0;
        }
        self.block_light_at(x, y, z)
    }

    /// Luz combinada (cielo vs bloque) en coordenadas que pueden salirse.
    #[inline]
    pub fn combined_or_zero(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0 {
            return 0;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return 0;
        }
        self.combined_light(x, y, z)
    }

    /// ¿La seccion `section` no tiene geometria que dibujar? Atajo para no
    /// meshear secciones vacias (la mayoria de las 24 de una columna).
    #[inline]
    pub fn section_is_empty(&self, section: usize) -> bool {
        self.sections[section].is_empty()
    }

    /// Lee un bloque con `y` global (0..WORLD_HEIGHT).
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.sections[y / CHUNK_SIZE].get(x, y % CHUNK_SIZE, z)
    }

    /// Escribe un bloque con `y` global.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.sections[y / CHUNK_SIZE].set(x, y % CHUNK_SIZE, z, block);
    }

    /// Lee un bloque en coordenadas que pueden salirse de la columna. Fuera
    /// (incluida la cara superior del mundo) devolvemos aire.
    #[inline]
    pub fn get_or_air(&self, x: i32, y: i32, z: i32) -> Block {
        if x < 0 || y < 0 || z < 0 {
            return Block::Air;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return Block::Air;
        }
        self.get(x, y, z)
    }

    /// Escribe un bloque ignorando si la posicion esta fuera.
    pub fn set_if_inside(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if x >= 0
            && y >= 0
            && z >= 0
            && (x as usize) < CHUNK_SIZE
            && (z as usize) < CHUNK_SIZE
            && (y as usize) < WORLD_HEIGHT
        {
            self.set(x as usize, y as usize, z as usize, block);
        }
    }

    /// Genera un terreno de ejemplo: colina a ~y=64 y un arbol.
    ///
    /// *Placeholder* deterministico. Los tests de meshing lo usan.
    pub fn generate_demo() -> Self {
        let mut column = Self::empty();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = demo_height(x, z);
                for y in 0..height {
                    let block = if y + 1 == height {
                        Block::Grass
                    } else if y + 4 >= height {
                        Block::Dirt
                    } else {
                        Block::Stone
                    };
                    column.set(x, y, z, block);
                }
            }
        }

        // Arbol de ejemplo.
        let (tx, tz) = (11usize, 4usize);
        let ground = demo_height(tx, tz);
        for i in 0..4 {
            column.set_if_inside(tx as i32, (ground + i) as i32, tz as i32, Block::Wood);
        }
        let top = (ground + 4) as i32;
        for dy in -2..=1i32 {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    if dx.abs() == 2 && dz.abs() == 2 {
                        continue;
                    }
                    let (lx, ly, lz) = (tx as i32 + dx, top + dy, tz as i32 + dz);
                    if column.get_or_air(lx, ly, lz) == Block::Air {
                        column.set_if_inside(lx, ly, lz, Block::Leaves);
                    }
                }
            }
        }

        column
    }
}

/// Altura del terreno de ejemplo en la columna `(x, z)`.
fn demo_height(x: usize, z: usize) -> usize {
    let fx = x as f32;
    let fz = z as f32;
    let h = 64.0 + 4.0 * (fx * 0.55).sin() + 3.0 * (fz * 0.50).cos();
    h.round().clamp(40.0, 80.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_es_unico_por_celda() {
        assert_eq!(Chunk::index(0, 0, 0), 0);
        assert_eq!(Chunk::index(15, 15, 15), CHUNK_VOLUME - 1);
        assert_eq!(Chunk::index(1, 0, 0), 1);
    }

    #[test]
    fn la_seccion_vacia_lo_esta() {
        assert!(Chunk::empty().is_empty());
        let mut chunk = Chunk::empty();
        chunk.set(3, 3, 3, Block::Stone);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn una_seccion_con_solo_antorcha_no_esta_vacia() {
        // La antorcha es visible-no-solida: si la contasemos como vacia, su cruz
        // no se meshearia nunca.
        let mut chunk = Chunk::empty();
        chunk.set(3, 3, 3, Block::Torch);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn get_or_air_fuera_de_la_columna() {
        let column = Column::empty();
        assert_eq!(column.get_or_air(-1, 0, 0), Block::Air);
        assert_eq!(column.get_or_air(16, 0, 0), Block::Air);
        assert_eq!(column.get_or_air(0, WORLD_HEIGHT as i32, 0), Block::Air);
    }

    #[test]
    fn escribir_y_leer_en_secciones_distintas() {
        let mut column = Column::empty();
        // y=5 esta en la seccion 0; y=20, en la 1.
        column.set(1, 5, 2, Block::Stone);
        column.set(1, 20, 2, Block::Sand);
        assert_eq!(column.get(1, 5, 2), Block::Stone);
        assert_eq!(column.get(1, 20, 2), Block::Sand);
        assert!(!column.sections[0].is_empty());
        assert!(!column.sections[1].is_empty());
        assert!(column.sections[2].is_empty());
    }

    #[test]
    fn la_skylight_ilumina_la_superficie_y_oscurece_el_subsuelo() {
        let mut column = Column::empty();
        // Suelo en y=0..4; aire por encima.
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                for y in 0..4 {
                    column.set(x, y, z, Block::Stone);
                }
            }
        }
        column.compute_skylight();
        // El aire sobre el suelo esta a cielo abierto -> luz 15.
        assert_eq!(column.light_at(0, 4, 0), MAX_LIGHT);
        assert_eq!(column.light_at(0, WORLD_HEIGHT - 1, 0), MAX_LIGHT);
        // Bajo el suelo no llega el cielo -> 0.
        assert_eq!(column.light_at(0, 3, 0), 0);
        // El propio bloque solido tambien queda a 0.
        assert_eq!(column.light_at(0, 0, 0), 0);
    }

    #[test]
    fn la_luz_de_bloque_se_propaga_desde_la_antorcha() {
        let mut column = Column::empty();
        // Antorcha en (8, 8, 8).
        column.set(8, 8, 8, Block::Torch);
        column.compute_block_light();
        // La fuente emite 14.
        assert_eq!(column.block_light_at(8, 8, 8), 14);
        // Un vecino inmediato recibe 13.
        assert_eq!(column.block_light_at(9, 8, 8), 13);
        // A 5 bloques baja a 9.
        assert_eq!(column.block_light_at(13, 8, 8), 9);
        // A 8 bloques (14 - 8) queda en 6.
        assert_eq!(column.block_light_at(0, 8, 8), 6);
    }

    #[test]
    fn la_luz_no_atraviesa_bloques_solidos() {
        let mut column = Column::empty();
        column.set(0, 4, 4, Block::Torch);
        // Pared solida COMPLETA (una cara entera del cubo 16x16) en x=1, para
        // que la luz no pueda rodearla.
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                column.set(1, y, z, Block::Stone);
            }
        }
        column.compute_block_light();
        // Al otro lado de la pared no llega luz.
        assert_eq!(column.block_light_at(2, 4, 4), 0);
    }

    #[test]
    fn la_luz_combinada_toma_el_maximo() {
        let mut column = Column::empty();
        column.set(8, 8, 8, Block::Torch);
        column.compute_skylight(); // todo aire a cielo abierto (pero hay antorcha)
        column.compute_block_light();
        // combined = max(sky, block) >= block.
        assert!(column.combined_light(8, 8, 8) >= 14);
    }

    #[test]
    fn el_terreno_demo_llena_el_subsuelo() {
        let column = Column::generate_demo();
        assert!(column.get(0, 0, 0).is_solid());
        assert_eq!(column.get(0, WORLD_HEIGHT - 1, 0), Block::Air);
    }

    #[test]
    fn el_flujo_se_empaqueta_en_nibbles_sin_pisar_al_vecino() {
        let mut column = Column::empty();
        // Sin agua que fluya no se reserva nada (un oceano no ocupa memoria).
        assert!(!column.has_flow_storage());
        assert_eq!(column.flow_at(0, 0, 0), 0);

        // Dos celdas que comparten byte (x par e impar): nibble bajo y alto.
        column.set_flow(2, 5, 7, 5);
        assert!(column.has_flow_storage());
        column.set_flow(3, 5, 7, MAX_LEVEL);
        assert_eq!(column.flow_at(2, 5, 7), 5, "nibble bajo");
        assert_eq!(column.flow_at(3, 5, 7), MAX_LEVEL, "nibble alto");
        // Borrar uno no toca al otro.
        column.set_flow(2, 5, 7, 0);
        assert_eq!(column.flow_at(2, 5, 7), 0);
        assert_eq!(column.flow_at(3, 5, 7), MAX_LEVEL);
    }

    #[test]
    fn el_nivel_de_flujo_se_recorta_al_maximo() {
        let mut column = Column::empty();
        column.set_flow(1, 1, 1, 200);
        assert_eq!(column.flow_at(1, 1, 1), MAX_LEVEL);
    }
}
