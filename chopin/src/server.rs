// src/server.rs
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::worker::Worker;
use crate::router::Router;

pub struct Server {
    host_port: String,
    workers: usize,
}

impl Server {
    pub fn bind(host_port: &str) -> Self {
        Self {
            host_port: host_port.to_string(),
            workers: num_cpus::get(), // Default to all cores
        }
    }

    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    pub fn serve(self, router: Router) -> Result<(), Box<dyn std::error::Error>> {
        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        // Setup signal handling - ctrl-c graceful shutdown
        let shutdown_clone = shutdown_flag.clone();
        ctrlc::set_handler(move || {
            println!("\\nReceived SIGINT. Initiating graceful shutdown...");
            shutdown_clone.store(true, Ordering::SeqCst);
        }).expect("Error setting Ctrl-C handler");


        let metrics = Arc::new(crate::metrics::WorkerMetrics::new());
        let metrics_clone = metrics.clone();
        let shutdown_metrics = shutdown_flag.clone();
        
        thread::Builder::new().name("chopin-metrics".to_string()).spawn(move || {
            while !shutdown_metrics.load(Ordering::Acquire) {
                thread::sleep(std::time::Duration::from_secs(5));
                if shutdown_metrics.load(Ordering::Acquire) { break; }
                let reqs = metrics_clone.req_count.load(Ordering::Relaxed);
                let active = metrics_clone.active_conns.load(Ordering::Relaxed);
                let bytes = metrics_clone.bytes_sent.load(Ordering::Relaxed);
                println!("[Metrics] Active Connections: {} | Total Requests: {} | Bytes Sent: {}", active, reqs, bytes);
            }
        }).ok();

        let mut handles = Vec::with_capacity(self.workers);
        println!("Starting {} workers on {}", self.workers, self.host_port);

        for i in 0..self.workers {
            let core_id = core_ids.get(i % core_ids.len()).copied(); // Pin to core or wrap around
            let router_clone = router.clone();

            let host_port = self.host_port.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = metrics.clone();

            let handle = thread::Builder::new()
                .name(format!("chopin-worker-{}", i))
                .spawn(move || {
                    if let Some(id) = core_id {
                        if core_affinity::set_for_current(id) {
                            println!("Worker {} pinned to CPU {}", i, id.id);
                        } else {
                            eprintln!("Worker {} failed to pin to CPU {}", i, id.id);
                        }
                    } else {
                        println!("Worker {} started (no pinning available)", i);
                    }

                    // Create and run the per-core worker loop
                    let mut worker = Worker::new(i, host_port, router_clone, metrics_worker);
                    worker.run(shutdown);
                })?;

            handles.push(handle);
        }

        // Wait for all workers to finish
        for handle in handles {
            let _ = handle.join();
        }

        println!("Chopin server shut down successfully.");
        Ok(())
    }
}
