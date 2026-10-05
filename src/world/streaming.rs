//! Generacion de columnas en **hilos de trabajo** (FASE 2 de la auditoria).
//!
//! El hilo principal no debe generar terreno: eso produce el tiron al cruzar de
//! chunk. `TerrainScheduler` reparte peticiones `(id, chunk)` a un pool de
//! workers que comparten el [`TerrainGenerator`] (`Arc`, ya `Send + Sync`) y
//! devuelven la `Column` generada.
//!
//! Cada peticion lleva un **id monotonico**: si el jugador se aleja y la columna
//! deja de interesar, su resultado (con id viejo) se **descarta** al llegar. Asi
//! ningun resultado obsoleto reemplaza datos nuevos.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::chunk::{CHUNK_SIZE, Column};
use super::save::ChunkPos;
use super::terrain::TerrainGenerator;

/// Peticion de generacion.
struct GenRequest {
    id: u64,
    pos: ChunkPos,
}

/// Resultado de generacion listo para el hilo principal.
///
/// La `Column` va **boxeada**: pesa ~98 KB y moverla por el canal/collect por
/// valor desbordaba la pila de 1 MB del hilo principal en `debug`.
pub struct GenResult {
    pub id: u64,
    pub pos: ChunkPos,
    pub column: Box<Column>,
}

/// Pool de generacion de terreno.
pub struct TerrainScheduler {
    /// `None` tras `join` (cerrar el canal termina los workers).
    tx: Option<Sender<GenRequest>>,
    rx: Receiver<GenResult>,
    handles: Vec<JoinHandle<()>>,
}

impl TerrainScheduler {
    /// Arranca `workers` hilos que comparten `generator`.
    pub fn new(generator: Arc<TerrainGenerator>, workers: usize) -> Self {
        let (tx, req_rx) = mpsc::channel::<GenRequest>();
        let (res_tx, rx) = mpsc::channel::<GenResult>();
        // Un solo `Receiver` compartido: cada worker toma una peticion, suelta el
        // cerrojo y genera; mientras, otro puede tomar la siguiente.
        let req_rx = Arc::new(Mutex::new(req_rx));
        let mut handles = Vec::new();
        for i in 0..workers.max(1) {
            let generator = Arc::clone(&generator);
            let req_rx = Arc::clone(&req_rx);
            let res_tx = res_tx.clone();
            let handle = thread::Builder::new()
                .name(format!("solaria-terrain-{i}"))
                .spawn(move || {
                    loop {
                        let request = {
                            let guard = req_rx.lock().unwrap();
                            guard.recv()
                        };
                        let Ok(request) = request else { break };
                        let column = generator.generate_column(
                            request.pos.x * CHUNK_SIZE as i32,
                            request.pos.z * CHUNK_SIZE as i32,
                        );
                        if res_tx
                            .send(GenResult {
                                id: request.id,
                                pos: request.pos,
                                column: Box::new(column),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .expect("no se pudo crear un hilo de terreno");
            handles.push(handle);
        }
        drop(res_tx);
        Self {
            tx: Some(tx),
            rx,
            handles,
        }
    }

    /// Encola una peticion (no bloquea).
    pub fn request(&self, id: u64, pos: ChunkPos) -> bool {
        match self.tx.as_ref() {
            Some(tx) => tx.send(GenRequest { id, pos }).is_ok(),
            None => false,
        }
    }

    /// Siguiente columna generada, si la hay.
    pub fn try_recv(&self) -> Option<GenResult> {
        self.rx.try_recv().ok()
    }

    /// Espera a que los workers terminen.
    pub fn join(&mut self) {
        self.tx = None;
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for TerrainScheduler {
    fn drop(&mut self) {
        self.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_scheduler_genera_y_etiqueta_los_resultados() {
        let generator = Arc::new(TerrainGenerator::new(13371));
        let mut scheduler = TerrainScheduler::new(generator, 2);
        scheduler.request(7, ChunkPos::new(0, 0));
        scheduler.request(8, ChunkPos::new(1, 0));

        // Recogemos los dos resultados (bloqueando con join: los workers vacian
        // la cola antes de salir).
        scheduler.join();
        let mut ids: Vec<u64> = Vec::new();
        while let Some(result) = scheduler.try_recv() {
            ids.push(result.id);
            // La columna tiene contenido (no es aire puro a 0..384).
            assert!(
                result.column.get(0, 0, 0).is_solid() || result.column.get(0, 64, 0).is_solid()
            );
        }
        ids.sort_unstable();
        assert_eq!(ids, vec![7, 8]);
    }
}
