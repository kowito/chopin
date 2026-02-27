// src/server.rs
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::worker::Worker;
use crate::router::Router;
use crate::syscalls;

pub struct Server {
    host_port: String,
    workers: usize,
}

impl Server {
    pub fn bind(host_port: &str) -> Self {
        Self {
            host_port: host_port.to_string(),
            workers: num_cpus::get(),
        }
    }

    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    pub fn serve(self, router: Router) -> Result<(), Box<dyn std::error::Error>> {
        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let mut worker_metrics = Vec::with_capacity(self.workers);
        for _ in 0..self.workers {
            worker_metrics.push(Arc::new(crate::metrics::WorkerMetrics::new()));
        }

        let metrics_clones = worker_metrics.clone();
        let shutdown_metrics = shutdown_flag.clone();
        
        thread::Builder::new().name("chopin-metrics".to_string()).spawn(move || {
            while !shutdown_metrics.load(Ordering::Acquire) {
                thread::sleep(std::time::Duration::from_secs(5));
                if shutdown_metrics.load(Ordering::Acquire) { break; }
                let mut total_reqs = 0;
                let mut total_active = 0;
                for m in &metrics_clones {
                    total_reqs += m.req_count.load(Ordering::Relaxed);
                    total_active += m.active_conns.load(Ordering::Relaxed);
                }
                println!("[Metrics] Active Connections: {} | Total Requests: {}", total_active, total_reqs);
            }
        }).ok();

        let host_port = self.host_port.clone();
        let parts: Vec<&str> = host_port.split(':').collect();
        let host = parts.first().unwrap_or(&"0.0.0.0").to_string();
        let port: u16 = parts.get(1).unwrap_or(&"8080").parse().unwrap();

        println!("Starting {} workers with SO_REUSEPORT (Independent Listeners)", self.workers);

        let mut handles = Vec::with_capacity(self.workers);
        for i in 0..self.workers {
            let core_id = core_ids.get(i % core_ids.len()).copied();
            let router_clone = router.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = worker_metrics[i].clone();
            let host_clone = host.clone();

            let handle = thread::Builder::new()
                .name(format!("chopin-worker-{}", i))
                .spawn(move || {
                    if let Some(id) = core_id {
                        core_affinity::set_for_current(id);
                    }

                    // Create its own listener with REUSEPORT
                    let listen_fd = syscalls::create_listen_socket_reuseport(&host_clone, port).expect("Failed to bind socket");
                    let mut worker = Worker::new(i, router_clone, metrics_worker, listen_fd);
                    worker.run(shutdown);
                    unsafe { libc::close(listen_fd); }
                })?;

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

        Ok(())
    }
}
