//! Guardado en **segundo plano**.
//!
//! Serializar el mundo (potencialmente miles de chunks) y escribirlo a disco no
//! debe bloquear el hilo de render. `SaveWorker` mantiene un hilo que recibe
//! instantaneas de [`WorldSave`] y se encarga de codificar y escribir de forma
//! atomica; el hilo principal solo pide el guardado y consulta el resultado.
//!
//! El hilo termina al soltar el canal (o con [`SaveWorker::join`]), momento en
//! el que suelta el trabajo pendiente: por eso el cierre del juego hace `join`
//! para no perder el ultimo guardado.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::world::WorldSave;

/// Trabajo de guardado: una instantanea + destino.
struct SaveJob {
    save: WorldSave,
    path: PathBuf,
}

/// Resultado de un guardado, para informar al hilo principal.
pub struct SaveOutcome {
    pub chunks: usize,
    pub bytes: u64,
    pub result: Result<(), String>,
}

/// Hilo de guardado + canales.
pub struct SaveWorker {
    /// `None` tras `join` (soltarlo cierra el canal y termina el hilo).
    tx: Option<Sender<SaveJob>>,
    rx: Receiver<SaveOutcome>,
    handle: Option<JoinHandle<()>>,
}

impl SaveWorker {
    /// Arranca el hilo de guardado.
    pub fn spawn() -> Self {
        let (tx, job_rx) = mpsc::channel::<SaveJob>();
        let (out_tx, rx) = mpsc::channel::<SaveOutcome>();
        let handle = thread::Builder::new()
            .name("solaria-save".to_string())
            .spawn(move || {
                // Termina cuando se suelta el `Sender` (recv -> Err), tras
                // procesar el ultimo trabajo encolado.
                while let Ok(job) = job_rx.recv() {
                    let chunks = job.save.chunks.len();
                    let result = job.save.save_to(&job.path).map_err(|e| e.to_string());
                    let bytes = std::fs::metadata(&job.path).map(|m| m.len()).unwrap_or(0);
                    if out_tx
                        .send(SaveOutcome {
                            chunks,
                            bytes,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("no se pudo crear el hilo de guardado");
        Self {
            tx: Some(tx),
            rx,
            handle: Some(handle),
        }
    }

    /// Pide un guardado (no bloquea). `false` si el worker ya no acepta trabajos.
    pub fn request(&self, save: WorldSave, path: PathBuf) -> bool {
        match self.tx.as_ref() {
            Some(tx) => tx.send(SaveJob { save, path }).is_ok(),
            None => false,
        }
    }

    /// Siguiente resultado terminado, si lo hay.
    pub fn try_recv(&self) -> Option<SaveOutcome> {
        self.rx.try_recv().ok()
    }

    /// Espera a que el hilo termine (tras soltar el canal). Se usa al cerrar.
    pub fn join(&mut self) {
        self.tx = None;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SaveWorker {
    fn drop(&mut self) {
        self.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Block, ChunkPos, ChunkRecord, Column, WorldSave};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solaria_worker_{name}.vf"))
    }

    #[test]
    fn el_worker_guarda_y_reporta_el_resultado() {
        let mut column = Column::empty();
        column.set(1, 2, 3, Block::Stone);
        let mut save = WorldSave::new(1, 0);
        save.set_chunk(ChunkPos::new(0, 0), ChunkRecord::from_column(&column));

        let path = temp_path("ok");
        let _ = std::fs::remove_file(&path);

        let mut worker = SaveWorker::spawn();
        assert!(worker.request(save, path.clone()));
        worker.join();

        let mut ok = false;
        while let Some(outcome) = worker.try_recv() {
            assert!(outcome.result.is_ok(), "{:?}", outcome.result);
            assert_eq!(outcome.chunks, 1);
            assert!(outcome.bytes > 0);
            ok = true;
        }
        assert!(ok, "no llego el resultado del guardado");
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn el_worker_reporta_error_de_ruta() {
        // Guardar dentro de un archivo (el "directorio" es un fichero) falla.
        let blocker = temp_path("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let path = blocker.join("world.vf");
        let mut worker = SaveWorker::spawn();
        assert!(worker.request(WorldSave::new(1, 0), path));
        worker.join();
        let mut fallo = false;
        while let Some(outcome) = worker.try_recv() {
            if outcome.result.is_err() {
                fallo = true;
            }
        }
        let _ = std::fs::remove_file(&blocker);
        assert!(fallo, "deberia reportar el error de escritura");
    }
}
