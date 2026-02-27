// src/server.rs
use crate::error::ChopinError;
use crate::router::Router;
use crate::syscalls;
use crate::worker::Worker;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

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

    pub fn serve(self, router: Router) -> crate::error::ChopinResult<()> {
        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let mut worker_metrics = Vec::with_capacity(self.workers);
        for _ in 0..self.workers {
            worker_metrics.push(Arc::new(crate::metrics::WorkerMetrics::new()));
        }

        let metrics_clones = worker_metrics.clone();
        let shutdown_metrics = shutdown_flag.clone();

        thread::Builder::new()
            .name("chopin-metrics".to_string())
            .spawn(move || {
                while !shutdown_metrics.load(Ordering::Acquire) {
                    thread::sleep(std::time::Duration::from_secs(5));
                    if shutdown_metrics.load(Ordering::Acquire) {
                        break;
                    }
                    let mut total_reqs = 0;
                    let mut total_active = 0;
                    for m in &metrics_clones {
                        total_reqs += m.req_count.load(Ordering::Relaxed);
                        total_active += m.active_conns.load(Ordering::Relaxed);
                    }
                    println!(
                        "[Metrics] Active Connections: {} | Total Requests: {}",
                        total_active, total_reqs
                    );
                }
            })
            .ok();

        let Parts { host, port } = parse_host_port(&self.host_port)?;

        println!(
            "Starting {} workers with SO_REUSEPORT (Independent Listeners)",
            self.workers
        );

        let mut handles = Vec::with_capacity(self.workers);
        for (i, metrics_worker) in worker_metrics.iter().enumerate().take(self.workers) {
            let core_id = core_ids.get(i % core_ids.len()).copied();
            let router_clone = router.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = metrics_worker.clone();
            let host_clone = host.clone();

            let handle = thread::Builder::new()
                .name(format!("chopin-worker-{}", i))
                .spawn(move || {
                    if let Some(id) = core_id {
                        core_affinity::set_for_current(id);
                    }

                    // Create its own listener with REUSEPORT
                    let listen_fd =
                        match syscalls::create_listen_socket_reuseport(&host_clone, port) {
                            Ok(fd) => fd,
                            Err(e) => {
                                eprintln!("Worker {} failed to bind socket: {}", i, e);
                                return;
                            }
                        };
                    let mut worker = Worker::new(i, router_clone, metrics_worker, listen_fd);
                    if let Err(e) = worker.run(shutdown) {
                        eprintln!("Worker {} exited with error: {}", i, e);
                    }
                    unsafe {
                        libc::close(listen_fd);
                    }
                })
                .map_err(ChopinError::from)?;

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

        Ok(())
    }
}

struct Parts {
    host: String,
    port: u16,
}

fn parse_host_port(hp: &str) -> crate::error::ChopinResult<Parts> {
    let parts: Vec<&str> = hp.split(':').collect();
    let host = parts.first().unwrap_or(&"0.0.0.0").to_string();
    let port = parts
        .get(1)
        .ok_or_else(|| crate::error::ChopinError::Other("Missing port in address".to_string()))?
        .parse::<u16>()
        .map_err(|_| crate::error::ChopinError::Other("Invalid port number".to_string()))?;

    Ok(Parts { host, port })
}
