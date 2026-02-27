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

        // ---- Per-Worker Metrics ----
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
                let mut total_bytes = 0;

                for m in &metrics_clones {
                    total_reqs += m.req_count.load(Ordering::Relaxed);
                    total_active += m.active_conns.load(Ordering::Relaxed);
                    total_bytes += m.bytes_sent.load(Ordering::Relaxed);
                }

                println!("[Metrics] Active Connections: {} | Total Requests: {} | Bytes Sent: {}", total_active, total_reqs, total_bytes);
            }
        }).ok();

        // ---- Create per-worker pipes ----
        let mut pipe_write_fds = Vec::with_capacity(self.workers);
        let mut pipe_read_fds = Vec::with_capacity(self.workers);

        for _ in 0..self.workers {
            let (read_fd, write_fd) = syscalls::create_pipe()?;
            pipe_read_fds.push(read_fd);
            pipe_write_fds.push(write_fd);
        }

        // ---- Spawn Worker Threads ----
        let mut handles = Vec::with_capacity(self.workers);
        println!("Starting {} workers on {}", self.workers, self.host_port);

        for i in 0..self.workers {
            let core_id = core_ids.get(i % core_ids.len()).copied();
            let router_clone = router.clone();
            let shutdown = shutdown_flag.clone();
            let metrics_worker = worker_metrics[i].clone();
            let pipe_fd = pipe_read_fds[i];

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

                    // Workers receive FDs via pipe — no listen socket needed
                    let mut worker = Worker::new(i, router_clone, metrics_worker, pipe_fd);
                    worker.run(shutdown);
                })?;

            handles.push(handle);
        }

        // ---- Spawn Acceptor Thread ----
        let parts: Vec<&str> = self.host_port.split(':').collect();
        let port: u16 = parts.get(1).unwrap_or(&"8080").parse().unwrap();
        let host = parts.first().unwrap_or(&"0.0.0.0").to_string();
        let shutdown_accept = shutdown_flag.clone();
        let num_workers = self.workers;

        let acceptor_handle = thread::Builder::new()
            .name("chopin-acceptor".to_string())
            .spawn(move || {
                let listen_fd = match syscalls::create_listen_socket(&host, port) {
                    Ok(fd) => fd,
                    Err(e) => {
                        eprintln!("Acceptor failed to bind: {}", e);
                        return;
                    }
                };

                println!("Acceptor listening on {}:{}", host, port);

                // Use kqueue/epoll for the listen socket
                let epoll = syscalls::Epoll::new().expect("Acceptor: failed to create epoll");
                epoll.add(listen_fd, 0, syscalls::EPOLLIN).expect("Acceptor: failed to register listen fd");

                let mut events = vec![syscalls::epoll_event { events: 0, u64: 0 }; 64];
                let mut next_worker: usize = 0;

                while !shutdown_accept.load(Ordering::Acquire) {
                    let n = match epoll.wait(&mut events, 500) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };

                    for _ev in 0..n {
                        // Drain the accept queue
                        loop {
                            match syscalls::accept_connection(listen_fd) {
                                Ok(Some(client_fd)) => {
                                    // Round-robin to workers
                                    let target = next_worker % num_workers;
                                    next_worker = next_worker.wrapping_add(1);

                                    if syscalls::send_fd_over_pipe(pipe_write_fds[target], client_fd).is_err() {
                                        unsafe { libc::close(client_fd); }
                                    }
                                }
                                Ok(None) => break,  // WouldBlock
                                Err(_) => break,
                            }
                        }
                    }
                }

                // Cleanup
                unsafe { libc::close(listen_fd); }
                for fd in &pipe_write_fds {
                    unsafe { libc::close(*fd); }
                }
                println!("Acceptor thread exiting.");
            })?;

        // Wait for all threads
        let _ = acceptor_handle.join();
        for handle in handles {
            let _ = handle.join();
        }

        println!("Chopin server shut down successfully.");
        Ok(())
    }
}
