//! Meshing CPU en **hilos de trabajo** (FASE 6 de la auditoria).
//!
//! El hilo principal construye un [`SectionSnapshot`] (lecturas baratas) y lo
//! envia a un worker, que ejecuta el greedy/fluido **sin tocar `wgpu`** y
//! devuelve los vertices/indices. La subida a GPU la hace el hilo principal.
//!
//! Cada trabajo lleva una **revision**; si la seccion volvio a cambiar mientras
//! se mesheaba, su resultado (revision vieja) se descarta al llegar.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::render::mesh::Vertex;
use crate::world::mesh_snapshot::{SectionSnapshot, mesh_snapshot};
use crate::world::save::ChunkPos;

/// Trabajo de meshing: una seccion + su snapshot.
pub struct MeshJob {
    pub pos: ChunkPos,
    pub section: usize,
    pub revision: u64,
    pub origin: [f32; 3],
    pub snapshot: SectionSnapshot,
}

/// Geometria CPU terminada, lista para subir a la GPU.
pub struct MeshOutput {
    pub pos: ChunkPos,
    pub section: usize,
    pub revision: u64,
    pub opaque: (Vec<Vertex>, Vec<u32>),
    pub water: (Vec<Vertex>, Vec<u32>),
}

/// Pool de meshing.
pub struct MeshScheduler {
    tx: Option<Sender<MeshJob>>,
    rx: Receiver<MeshOutput>,
    handles: Vec<JoinHandle<()>>,
}

impl MeshScheduler {
    pub fn new(workers: usize) -> Self {
        let (tx, job_rx) = mpsc::channel::<MeshJob>();
        let (out_tx, rx) = mpsc::channel::<MeshOutput>();
        let job_rx = Arc::new(StdMutex::new(job_rx));
        let mut handles = Vec::new();
        for i in 0..workers.max(1) {
            let job_rx = Arc::clone(&job_rx);
            let out_tx = out_tx.clone();
            let handle = thread::Builder::new()
                .name(format!("solaria-mesh-{i}"))
                .spawn(move || {
                    loop {
                        // Se suelta el cerrojo antes de meshear: mientras un worker
                        // meshea, otro toma la siguiente peticion.
                        let job = {
                            let guard = job_rx.lock().unwrap();
                            guard.recv()
                        };
                        let Ok(job) = job else { break };
                        let (ov, oi, wv, wi) =
                            mesh_snapshot(&job.snapshot, job.section, job.origin);
                        if out_tx
                            .send(MeshOutput {
                                pos: job.pos,
                                section: job.section,
                                revision: job.revision,
                                opaque: (ov, oi),
                                water: (wv, wi),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .expect("no se pudo crear un hilo de meshing");
            handles.push(handle);
        }
        drop(out_tx);
        Self {
            tx: Some(tx),
            rx,
            handles,
        }
    }

    pub fn request(&self, job: MeshJob) -> bool {
        match self.tx.as_ref() {
            Some(tx) => tx.send(job).is_ok(),
            None => false,
        }
    }

    pub fn try_recv(&self) -> Option<MeshOutput> {
        self.rx.try_recv().ok()
    }

    pub fn join(&mut self) {
        self.tx = None;
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for MeshScheduler {
    fn drop(&mut self) {
        self.join();
    }
}
