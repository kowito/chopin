// src/syscalls.rs
use crate::error::ChopinResult;
use libc::{c_int, c_void, socklen_t};
use std::io;
use std::mem;
use std::ptr;

// ---- Socket Operations ----

/// Create a non-blocking TCP server socket with SO_REUSEPORT (crucial for per-core binding)
pub fn create_listen_socket(host: &str, port: u16) -> ChopinResult<c_int> {
    let addr_str = format!("{}:{}", host, port);
    let addr: std::net::SocketAddr = addr_str
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let is_ipv6 = matches!(addr, std::net::SocketAddr::V6(_));
    let domain = if is_ipv6 {
        libc::AF_INET6
    } else {
        libc::AF_INET
    };

    #[cfg(target_os = "linux")]
    unsafe {
        // 1. Create socket
        let fd = libc::socket(domain, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }

        // 2. Set SO_REUSEPORT to allow multiple workers to bind to the same port
        let optval: c_int = 1;
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &optval as *const _ as *const c_void,
            mem::size_of_val(&optval) as socklen_t,
        ) < 0
        {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        // 3. Bind
        match addr {
            std::net::SocketAddr::V4(a) => {
                let sin = libc::sockaddr_in {
                    sin_family: libc::AF_INET as libc::sa_family_t,
                    sin_port: a.port().to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_ne_bytes(a.ip().octets()),
                    },
                    sin_zero: [0; 8],
                };
                if libc::bind(
                    fd,
                    &sin as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
            std::net::SocketAddr::V6(a) => {
                let sin6 = libc::sockaddr_in6 {
                    sin6_family: libc::AF_INET6 as libc::sa_family_t,
                    sin6_port: a.port().to_be(),
                    sin6_flowinfo: a.flowinfo(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: a.ip().octets(),
                    },
                    sin6_scope_id: a.scope_id(),
                };
                if libc::bind(
                    fd,
                    &sin6 as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin6) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
        }

        // 4. Listen
        // SOMAXCONN in linux is usually 4096. We queue aggressively.
        if libc::listen(fd, libc::SOMAXCONN) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        Ok(fd)
    }

    #[cfg(target_os = "macos")]
    unsafe {
        // 1. Create socket
        let fd = libc::socket(domain, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }

        // Set non-blocking manually
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        // 2. Set SO_REUSEPORT to allow multiple workers to bind to the same port
        let optval: c_int = 1;
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &optval as *const _ as *const c_void,
            mem::size_of_val(&optval) as socklen_t,
        ) < 0
        {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        // 3. Bind
        match addr {
            std::net::SocketAddr::V4(a) => {
                let sin = libc::sockaddr_in {
                    sin_len: mem::size_of::<libc::sockaddr_in>() as u8,
                    sin_family: libc::AF_INET as libc::sa_family_t,
                    sin_port: a.port().to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_ne_bytes(a.ip().octets()),
                    },
                    sin_zero: [0; 8],
                };
                if libc::bind(
                    fd,
                    &sin as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
            std::net::SocketAddr::V6(a) => {
                let sin6 = libc::sockaddr_in6 {
                    sin6_len: mem::size_of::<libc::sockaddr_in6>() as u8,
                    sin6_family: libc::AF_INET6 as libc::sa_family_t,
                    sin6_port: a.port().to_be(),
                    sin6_flowinfo: a.flowinfo(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: a.ip().octets(),
                    },
                    sin6_scope_id: a.scope_id(),
                };
                if libc::bind(
                    fd,
                    &sin6 as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin6) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
        }

        // 4. Listen
        if libc::listen(fd, libc::SOMAXCONN) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        Ok(fd)
    }
}

/// Create a maximally-optimized TCP listener with SO_REUSEPORT.
///
/// Platform optimizations:
/// - **Both**: SO_REUSEADDR, SO_REUSEPORT, TCP_NODELAY (inherited by accepted sockets)
/// - **Linux**: SOCK_NONBLOCK (atomic), TCP_DEFER_ACCEPT, TCP_FASTOPEN
/// - **macOS**: SO_NOSIGPIPE, TCP_FASTOPEN
pub fn create_listen_socket_reuseport(host: &str, port: u16) -> ChopinResult<c_int> {
    let addr_str = format!("{}:{}", host, port);
    let addr: std::net::SocketAddr = addr_str
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid address"))?;

    let is_ipv6 = matches!(addr, std::net::SocketAddr::V6(_));
    let domain = if is_ipv6 {
        libc::AF_INET6
    } else {
        libc::AF_INET
    };

    #[cfg(target_os = "linux")]
    unsafe {
        // 1. Atomic non-blocking socket (saves 2 fcntl syscalls vs macOS path)
        let fd = libc::socket(domain, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }

        let one: c_int = 1;

        // 2. SO_REUSEADDR + SO_REUSEPORT for per-worker binding
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        );
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        ) < 0
        {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        // 3. TCP_NODELAY on listener — inherited by all accepted sockets (eliminates per-accept setsockopt)
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NODELAY,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        );

        // 4. TCP_DEFER_ACCEPT — kernel holds connection until data arrives (reduces idle accept wakeups)
        let defer_secs: c_int = 1;
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_DEFER_ACCEPT,
            &defer_secs as *const _ as *const c_void,
            mem::size_of_val(&defer_secs) as socklen_t,
        );

        // 5. TCP_FASTOPEN — enable TFO with a queue of 256 pending connections
        let tfo_queue: c_int = 256;
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_FASTOPEN,
            &tfo_queue as *const _ as *const c_void,
            mem::size_of_val(&tfo_queue) as socklen_t,
        );

        // 6. Bind
        bind_addr(fd, &addr)?;

        // 7. Listen with aggressive backlog
        if libc::listen(fd, 8192) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        Ok(fd)
    }

    #[cfg(target_os = "macos")]
    unsafe {
        // 1. Create socket
        let fd = libc::socket(domain, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }

        // 2. Set non-blocking (macOS lacks SOCK_NONBLOCK)
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        let one: c_int = 1;

        // 3. SO_REUSEADDR + SO_REUSEPORT
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        );
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        ) < 0
        {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        // 4. SO_NOSIGPIPE — prevent SIGPIPE on broken connections (macOS has no MSG_NOSIGNAL)
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        );

        // 5. TCP_NODELAY on listener — inherited by accepted sockets
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NODELAY,
            &one as *const _ as *const c_void,
            mem::size_of_val(&one) as socklen_t,
        );

        // 6. TCP_FASTOPEN (macOS uses connectx-style TFO, value 0x105)
        const TCP_FASTOPEN_MACOS: c_int = 0x105;
        let tfo_enable: c_int = 1;
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            TCP_FASTOPEN_MACOS,
            &tfo_enable as *const _ as *const c_void,
            mem::size_of_val(&tfo_enable) as socklen_t,
        );

        // 7. Bind
        bind_addr(fd, &addr)?;

        // 8. Listen
        if libc::listen(fd, libc::SOMAXCONN) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err.into());
        }

        Ok(fd)
    }
}

/// Bind a socket to an address (shared between platforms).
fn bind_addr(fd: c_int, addr: &std::net::SocketAddr) -> ChopinResult<()> {
    unsafe {
        match addr {
            std::net::SocketAddr::V4(a) => {
                #[cfg(target_os = "macos")]
                let sin = libc::sockaddr_in {
                    sin_len: mem::size_of::<libc::sockaddr_in>() as u8,
                    sin_family: libc::AF_INET as libc::sa_family_t,
                    sin_port: a.port().to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_ne_bytes(a.ip().octets()),
                    },
                    sin_zero: [0; 8],
                };
                #[cfg(target_os = "linux")]
                let sin = libc::sockaddr_in {
                    sin_family: libc::AF_INET as libc::sa_family_t,
                    sin_port: a.port().to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_ne_bytes(a.ip().octets()),
                    },
                    sin_zero: [0; 8],
                };
                if libc::bind(
                    fd,
                    &sin as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
            std::net::SocketAddr::V6(a) => {
                #[cfg(target_os = "macos")]
                let sin6 = libc::sockaddr_in6 {
                    sin6_len: mem::size_of::<libc::sockaddr_in6>() as u8,
                    sin6_family: libc::AF_INET6 as libc::sa_family_t,
                    sin6_port: a.port().to_be(),
                    sin6_flowinfo: a.flowinfo(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: a.ip().octets(),
                    },
                    sin6_scope_id: a.scope_id(),
                };
                #[cfg(target_os = "linux")]
                let sin6 = libc::sockaddr_in6 {
                    sin6_family: libc::AF_INET6 as libc::sa_family_t,
                    sin6_port: a.port().to_be(),
                    sin6_flowinfo: a.flowinfo(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: a.ip().octets(),
                    },
                    sin6_scope_id: a.scope_id(),
                };
                if libc::bind(
                    fd,
                    &sin6 as *const _ as *const libc::sockaddr,
                    mem::size_of_val(&sin6) as socklen_t,
                ) < 0
                {
                    let err = io::Error::last_os_error();
                    libc::close(fd);
                    return Err(err.into());
                }
            }
        }
        Ok(())
    }
}

/// Accept a non-blocking connection
pub fn accept_connection(listen_fd: c_int) -> ChopinResult<Option<c_int>> {
    #[cfg(target_os = "linux")]
    unsafe {
        let fd = libc::accept4(
            listen_fd,
            ptr::null_mut(),
            ptr::null_mut(),
            libc::SOCK_NONBLOCK,
        );

        if fd < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                Ok(None)
            } else {
                Err(err.into())
            }
        } else {
            // TCP_NODELAY is inherited from the listener socket
            Ok(Some(fd))
        }
    }

    #[cfg(target_os = "macos")]
    unsafe {
        let fd = libc::accept(listen_fd, ptr::null_mut(), ptr::null_mut());

        if fd < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EWOULDBLOCK)
                || err.kind() == io::ErrorKind::WouldBlock
            {
                Ok(None)
            } else {
                Err(err.into())
            }
        } else {
            // Set O_NONBLOCK manually since macOS lacks accept4
            let flags = libc::fcntl(fd, libc::F_GETFL, 0);
            if flags < 0 {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err.into());
            }
            if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err.into());
            }

            // SO_NOSIGPIPE on accepted socket (macOS has no MSG_NOSIGNAL)
            let one: c_int = 1;
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                &one as *const _ as *const c_void,
                mem::size_of_val(&one) as socklen_t,
            );

            // TCP_NODELAY is inherited from the listener socket
            Ok(Some(fd))
        }
    }
}

// ---- Epoll Operations (Linux Only) ----

#[cfg(target_os = "linux")]
pub use linux_epoll::*;

#[cfg(target_os = "linux")]
mod linux_epoll {
    use super::*;
    pub use libc::{EPOLLET, EPOLLIN, EPOLLOUT, epoll_event};

    pub struct Epoll {
        pub fd: c_int,
    }

    impl Epoll {
        pub fn new() -> ChopinResult<Self> {
            unsafe {
                let fd = libc::epoll_create1(0);
                if fd < 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Ok(Self { fd })
            }
        }

        /// Add a file descriptor to epoll. We use Edge Triggered (EPOLLET) for high performance.
        pub fn add(&self, fd: c_int, token: u64, interests: i32) -> ChopinResult<()> {
            let mut event = epoll_event {
                events: (interests | EPOLLET) as u32,
                u64: token,
            };

            unsafe {
                if libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, fd, &mut event) < 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            Ok(())
        }

        pub fn modify(&self, fd: c_int, token: u64, interests: i32) -> ChopinResult<()> {
            let mut event = epoll_event {
                events: (interests | EPOLLET) as u32,
                u64: token,
            };

            unsafe {
                if libc::epoll_ctl(self.fd, libc::EPOLL_CTL_MOD, fd, &mut event) < 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            Ok(())
        }

        pub fn delete(&self, fd: c_int) -> ChopinResult<()> {
            unsafe {
                if libc::epoll_ctl(self.fd, libc::EPOLL_CTL_DEL, fd, ptr::null_mut()) < 0 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::ENOENT) {
                        return Err(err.into());
                    }
                }
            }
            Ok(())
        }

        pub fn wait(&self, events: &mut [epoll_event], timeout_ms: i32) -> ChopinResult<usize> {
            unsafe {
                let res = libc::epoll_wait(
                    self.fd,
                    events.as_mut_ptr(),
                    events.len() as c_int,
                    timeout_ms,
                );

                if res < 0 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::EINTR) {
                        return Ok(0);
                    }
                    return Err(err.into());
                }

                Ok(res as usize)
            }
        }
    }

    impl Drop for Epoll {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

// ---- Epoll/Kqueue Fallback for MacOS development ----
#[cfg(target_os = "macos")]
pub use macos_epoll::*;

#[cfg(target_os = "macos")]
mod macos_epoll {
    use super::*;
    use libc::{
        EV_ADD, EV_CLEAR, EV_DELETE, EV_ENABLE, EVFILT_READ, EVFILT_WRITE, kevent, kqueue, timespec,
    };
    use std::ptr;

    #[allow(non_camel_case_types)]
    #[derive(Clone, Copy)]
    pub struct epoll_event {
        pub events: u32,
        pub u64: u64,
    }

    pub const EPOLLIN: i32 = 1;
    pub const EPOLLOUT: i32 = 4;
    pub const EPOLLET: i32 = 1 << 31;

    pub struct Epoll {
        pub fd: c_int,
    }

    impl Epoll {
        pub fn new() -> ChopinResult<Self> {
            unsafe {
                let fd = kqueue();
                if fd < 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Ok(Self { fd })
            }
        }

        pub fn add(&self, fd: c_int, token: u64, interests: i32) -> ChopinResult<()> {
            self.modify_kqueue(fd, token, interests, EV_ADD | EV_ENABLE | EV_CLEAR)
        }

        pub fn modify(&self, fd: c_int, token: u64, interests: i32) -> ChopinResult<()> {
            self.modify_kqueue(fd, token, interests, EV_ADD | EV_ENABLE | EV_CLEAR)
        }

        pub fn delete(&self, fd: c_int) -> ChopinResult<()> {
            self.modify_kqueue(fd, 0, EPOLLIN | EPOLLOUT, EV_DELETE)
        }

        fn modify_kqueue(
            &self,
            fd: c_int,
            token: u64,
            interests: i32,
            action: u16,
        ) -> ChopinResult<()> {
            let mut changes = [unsafe { std::mem::zeroed::<kevent>() }; 2];
            let mut n = 0;

            if (interests & EPOLLIN) != 0 || action == EV_DELETE {
                changes[n] = kevent {
                    ident: fd as usize,
                    filter: EVFILT_READ,
                    flags: action,
                    fflags: 0,
                    data: 0,
                    udata: token as *mut c_void,
                };
                n += 1;
            }

            if (interests & EPOLLOUT) != 0 || action == EV_DELETE {
                changes[n] = kevent {
                    ident: fd as usize,
                    filter: EVFILT_WRITE,
                    flags: action,
                    fflags: 0,
                    data: 0,
                    udata: token as *mut c_void,
                };
                n += 1;
            }

            unsafe {
                // If action is DELETE, some filters might fail if they weren't added, ignore ENOENT equivalents
                let res = libc::kevent(
                    self.fd,
                    changes.as_ptr(),
                    n as c_int,
                    ptr::null_mut(),
                    0,
                    ptr::null(),
                );

                if res < 0 && action != EV_DELETE {
                    return Err(io::Error::last_os_error().into());
                }
            }
            Ok(())
        }

        pub fn wait(&self, events: &mut [epoll_event], timeout_ms: i32) -> ChopinResult<usize> {
            const MAX_BATCH: usize = 128; // Stack-allocated buffer for kevents
            let mut kevents = [unsafe { std::mem::zeroed::<kevent>() }; MAX_BATCH];
            let batch_size = events.len().min(MAX_BATCH);

            let ts = if timeout_ms >= 0 {
                Some(timespec {
                    tv_sec: (timeout_ms / 1000) as libc::time_t,
                    tv_nsec: ((timeout_ms % 1000) * 1_000_000) as libc::c_long,
                })
            } else {
                None
            };

            let ts_ptr = match &ts {
                Some(t) => t as *const timespec,
                None => ptr::null(),
            };

            unsafe {
                let res = libc::kevent(
                    self.fd,
                    ptr::null(),
                    0,
                    kevents.as_mut_ptr(),
                    batch_size as c_int,
                    ts_ptr,
                );

                if res < 0 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::EINTR) {
                        return Ok(0);
                    }
                    return Err(err.into());
                }

                let n = res as usize;
                for i in 0..n {
                    let mut ep_ev = 0;
                    if kevents[i].filter == EVFILT_READ {
                        ep_ev |= EPOLLIN;
                    }
                    if kevents[i].filter == EVFILT_WRITE {
                        ep_ev |= EPOLLOUT;
                    }
                    events[i] = epoll_event {
                        events: ep_ev as u32,
                        u64: kevents[i].udata as u64,
                    };
                }

                Ok(n)
            }
        }
    }

    impl Drop for Epoll {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

pub fn read_nonblocking(fd: c_int, buf: &mut [u8]) -> ChopinResult<usize> {
    unsafe {
        let res = libc::read(fd, buf.as_mut_ptr() as *mut c_void, buf.len());
        if res < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                // Propagate as error so caller can distinguish from EOF (Ok(0))
                Err(err.into())
            } else {
                Err(err.into())
            }
        } else {
            // 0 bytes read on non-blocking means EOF (connection closed by peer)
            Ok(res as usize)
        }
    }
}

pub fn write_nonblocking(fd: c_int, buf: &[u8]) -> ChopinResult<usize> {
    unsafe {
        let res = libc::write(fd, buf.as_ptr() as *const c_void, buf.len());
        if res < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                Ok(0)
            } else {
                Err(err.into())
            }
        } else {
            Ok(res as usize)
        }
    }
}

// ---- File Operations for Zero-Copy Serving ----

/// Open a file in read-only mode, returning its file descriptor.
pub fn open_file_readonly(path: &str) -> io::Result<c_int> {
    let c_path =
        std::ffi::CString::new(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    unsafe {
        let fd = libc::open(c_path.as_ptr(), libc::O_RDONLY);
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(fd)
        }
    }
}

/// Get the size in bytes of an open file descriptor using `fstat`.
pub fn file_size(fd: c_int) -> io::Result<u64> {
    unsafe {
        let mut stat: libc::stat = mem::zeroed();
        if libc::fstat(fd, &mut stat) < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(stat.st_size as u64)
        }
    }
}

/// Zero-copy sendfile: transfer data directly from a file descriptor to a socket
/// entirely within the kernel. Returns the number of bytes transferred, or 0 for
/// `EAGAIN`/`EWOULDBLOCK` (socket buffer full — wait for EPOLLOUT).
///
/// # Safety
/// Both `socket_fd` and `file_fd` must be valid open file descriptors.
#[cfg(target_os = "linux")]
pub fn sendfile_nonblocking(
    socket_fd: c_int,
    file_fd: c_int,
    offset: &mut u64,
    count: u64,
) -> ChopinResult<usize> {
    unsafe {
        let mut off = *offset as libc::off_t;
        let res = libc::sendfile(socket_fd, file_fd, &mut off, count as libc::size_t);
        if res < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                Ok(0)
            } else {
                Err(err.into())
            }
        } else {
            *offset = off as u64;
            Ok(res as usize)
        }
    }
}

/// Zero-copy sendfile for macOS. macOS `sendfile` has a different signature:
/// `sendfile(fd, s, offset, &mut len, hdtr, flags)` where `fd` is the file
/// and `s` is the socket. `len` is in/out: pass desired count, get actual sent.
#[cfg(target_os = "macos")]
pub fn sendfile_nonblocking(
    socket_fd: c_int,
    file_fd: c_int,
    offset: &mut u64,
    count: u64,
) -> ChopinResult<usize> {
    unsafe {
        let mut len = count as libc::off_t;
        let res = libc::sendfile(
            file_fd,
            socket_fd,
            *offset as libc::off_t,
            &mut len,
            ptr::null_mut(),
            0,
        );
        let sent = len as usize;
        if res < 0 {
            let err = io::Error::last_os_error();
            // macOS sendfile may return -1 with EAGAIN but still transfer some bytes
            if err.kind() == io::ErrorKind::WouldBlock {
                *offset += sent as u64;
                Ok(sent)
            } else {
                // Even on error, advance offset by whatever was sent
                *offset += sent as u64;
                if sent > 0 { Ok(sent) } else { Err(err.into()) }
            }
        } else {
            *offset += sent as u64;
            Ok(sent)
        }
    }
}

/// Vectored write: write multiple buffers in a single syscall (scatter-gather I/O)
pub fn writev_nonblocking(fd: c_int, bufs: &[&[u8]]) -> ChopinResult<usize> {
    if bufs.is_empty() {
        return Ok(0);
    }

    // Build iovec array on stack (max 8 segments)
    let mut iovecs: [libc::iovec; 8] = unsafe { std::mem::zeroed() };
    let iov_count = bufs.len().min(8);

    for i in 0..iov_count {
        iovecs[i] = libc::iovec {
            iov_base: bufs[i].as_ptr() as *mut c_void,
            iov_len: bufs[i].len(),
        };
    }

    unsafe {
        let res = libc::writev(fd, iovecs.as_ptr(), iov_count as c_int);
        if res < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                Ok(0)
            } else {
                Err(err.into())
            }
        } else {
            Ok(res as usize)
        }
    }
}

// ---- io_uring Backend (Linux only, feature-gated) ----

#[cfg(all(target_os = "linux", feature = "io-uring"))]
pub mod uring {
    use libc::{c_int, c_long, c_uint, c_void};
    use std::io;
    use std::ptr;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ---- io_uring syscall numbers (x86_64) ----
    const SYS_IO_URING_SETUP: c_long = 425;
    const SYS_IO_URING_ENTER: c_long = 426;
    const SYS_IO_URING_REGISTER: c_long = 427;

    // ---- io_uring setup flags ----
    pub const IORING_SETUP_SQPOLL: u32 = 1 << 1;
    pub const IORING_SETUP_COOP_TASKRUN: u32 = 1 << 8;
    pub const IORING_SETUP_SINGLE_ISSUER: u32 = 1 << 12;

    // ---- io_uring enter flags ----
    const IORING_ENTER_GETEVENTS: c_uint = 1;
    const IORING_ENTER_SQ_WAKEUP: c_uint = 1 << 1;

    // ---- io_uring op codes ----
    pub const IORING_OP_NOP: u8 = 0;
    pub const IORING_OP_READV: u8 = 1;
    pub const IORING_OP_WRITEV: u8 = 2;
    pub const IORING_OP_READ_FIXED: u8 = 4;
    pub const IORING_OP_WRITE_FIXED: u8 = 5;
    pub const IORING_OP_READ: u8 = 22;
    pub const IORING_OP_WRITE: u8 = 23;
    pub const IORING_OP_ACCEPT: u8 = 13;
    pub const IORING_OP_CLOSE: u8 = 19;
    pub const IORING_OP_SEND: u8 = 26;
    pub const IORING_OP_SPLICE: u8 = 30;
    pub const IORING_OP_SEND_ZC: u8 = 57;

    // ---- io_uring SQE flags ----
    pub const IOSQE_FIXED_FILE: u8 = 1;
    pub const IOSQE_IO_LINK: u8 = 1 << 2;

    // ---- io_uring accept flags ----
    pub const IORING_ACCEPT_MULTISHOT: u16 = 1;

    // ---- io_uring CQE flags ----
    pub const IORING_CQE_F_MORE: u32 = 1 << 1;

    // ---- io_uring SQ ring flags ----
    const IORING_SQ_NEED_WAKEUP: u32 = 1;

    // ---- io_uring register opcodes ----
    pub const IORING_REGISTER_BUFFERS: c_uint = 0;
    pub const IORING_UNREGISTER_BUFFERS: c_uint = 1;

    // ---- mmap offsets ----
    const IORING_OFF_SQ_RING: u64 = 0;
    const IORING_OFF_CQ_RING: u64 = 0x8000000;
    const IORING_OFF_SQES: u64 = 0x10000000;

    // ---- User-data encoding for operation type ----
    pub const OP_TYPE_ACCEPT: u8 = 0;
    pub const OP_TYPE_READ: u8 = 1;
    pub const OP_TYPE_WRITE: u8 = 2;
    pub const OP_TYPE_WRITEV: u8 = 3;
    pub const OP_TYPE_CLOSE: u8 = 4;
    pub const OP_TYPE_SENDFILE: u8 = 5;

    /// Sentinel connection index for accept operations
    pub const ACCEPT_CONN_IDX: u64 = 0x00FF_FFFF;

    #[inline(always)]
    pub fn encode_user_data(conn_idx: usize, op_type: u8) -> u64 {
        ((conn_idx as u64) << 8) | op_type as u64
    }

    #[inline(always)]
    pub fn decode_user_data(user_data: u64) -> (usize, u8) {
        let conn_idx = (user_data >> 8) as usize;
        let op_type = (user_data & 0xFF) as u8;
        (conn_idx, op_type)
    }

    // ---- Kernel ABI structures ----

    #[repr(C)]
    pub struct io_uring_sqe {
        pub opcode: u8,
        pub flags: u8,
        pub ioprio: u16,
        pub fd: i32,
        pub off_or_addr2: u64,  // union: off, addr2
        pub addr_or_splice: u64, // union: addr, splice_off_in
        pub len: u32,
        pub op_flags: u32,      // union: rw_flags, fsync_flags, poll_events, etc.
        pub user_data: u64,
        pub buf_index_or_group: u16, // union: buf_index, buf_group
        pub personality: u16,
        pub splice_fd_in_or_file_index: i32, // union
        pub addr3: u64,
        pub __pad2: [u64; 1],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct io_uring_cqe {
        pub user_data: u64,
        pub res: i32,
        pub flags: u32,
    }

    #[repr(C)]
    struct io_sqring_offsets {
        head: u32,
        tail: u32,
        ring_mask: u32,
        ring_entries: u32,
        flags: u32,
        dropped: u32,
        array: u32,
        resv1: u32,
        user_addr: u64,
    }

    #[repr(C)]
    struct io_cqring_offsets {
        head: u32,
        tail: u32,
        ring_mask: u32,
        ring_entries: u32,
        overflow: u32,
        cqes: u32,
        flags: u32,
        resv1: u32,
        user_addr: u64,
    }

    #[repr(C)]
    struct io_uring_params {
        sq_entries: u32,
        cq_entries: u32,
        flags: u32,
        sq_thread_cpu: u32,
        sq_thread_idle: u32,
        features: u32,
        wq_fd: u32,
        resv: [u32; 3],
        sq_off: io_sqring_offsets,
        cq_off: io_cqring_offsets,
    }

    /// Raw io_uring ring managing mmap'd SQ and CQ ring buffers.
    /// All operations are zero-allocation and operate on shared memory with the kernel.
    pub struct UringRing {
        ring_fd: c_int,

        // SQ ring pointers (mmap'd, shared with kernel)
        sq_ring_ptr: *mut u8,
        sq_ring_size: usize,
        sq_head: *const AtomicU32,
        sq_tail: *mut AtomicU32,
        sq_mask: u32,
        sq_flags: *const AtomicU32,
        sq_array: *mut u32,

        // SQE array (separate mmap)
        sqes_ptr: *mut io_uring_sqe,
        sqes_size: usize,
        sq_entries: u32,

        // CQ ring pointers (mmap'd alongside SQ ring or separate)
        cq_ring_ptr: *mut u8,
        cq_ring_size: usize,
        cq_head: *mut AtomicU32,
        cq_tail: *const AtomicU32,
        cq_mask: u32,
        cq_cqes: *const io_uring_cqe,
        cq_overflow: *const AtomicU32,
    }

    // SAFETY: UringRing is used single-threaded per worker (shared-nothing model).
    // The mmap'd memory is only accessed by the owning thread and the kernel.
    unsafe impl Send for UringRing {}

    impl UringRing {
        /// Create a new io_uring instance with the specified number of entries and flags.
        ///
        /// Common flags:
        /// - `IORING_SETUP_SQPOLL`: Kernel thread polls SQ ring (zero-syscall submission)
        /// - `IORING_SETUP_SINGLE_ISSUER`: Hint that only one thread submits (performance gain)
        /// - `IORING_SETUP_COOP_TASKRUN`: Cooperative task running (reduces IPIs)
        pub fn new(entries: u32, flags: u32) -> io::Result<Self> {
            let mut params: io_uring_params = unsafe { std::mem::zeroed() };
            params.flags = flags;

            // For SQPOLL: kernel thread sleeps after 1000ms idle, then needs wakeup
            if (flags & IORING_SETUP_SQPOLL) != 0 {
                params.sq_thread_idle = 1000;
            }

            let ring_fd = unsafe {
                libc::syscall(SYS_IO_URING_SETUP, entries, &mut params as *mut _) as c_int
            };
            if ring_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let sq_entries = params.sq_entries;
            let cq_entries = params.cq_entries;

            // mmap SQ ring
            let sq_ring_size = params.sq_off.array as usize
                + (sq_entries as usize) * std::mem::size_of::<u32>();
            let sq_ring_ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    sq_ring_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED | libc::MAP_POPULATE,
                    ring_fd,
                    IORING_OFF_SQ_RING as libc::off_t,
                ) as *mut u8
            };
            if sq_ring_ptr == libc::MAP_FAILED as *mut u8 {
                unsafe { libc::close(ring_fd); }
                return Err(io::Error::last_os_error());
            }

            // mmap CQ ring
            let cq_ring_size = params.cq_off.cqes as usize
                + (cq_entries as usize) * std::mem::size_of::<io_uring_cqe>();
            let cq_ring_ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    cq_ring_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED | libc::MAP_POPULATE,
                    ring_fd,
                    IORING_OFF_CQ_RING as libc::off_t,
                ) as *mut u8
            };
            if cq_ring_ptr == libc::MAP_FAILED as *mut u8 {
                unsafe {
                    libc::munmap(sq_ring_ptr as *mut c_void, sq_ring_size);
                    libc::close(ring_fd);
                }
                return Err(io::Error::last_os_error());
            }

            // mmap SQE array
            let sqes_size = (sq_entries as usize) * std::mem::size_of::<io_uring_sqe>();
            let sqes_ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    sqes_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED | libc::MAP_POPULATE,
                    ring_fd,
                    IORING_OFF_SQES as libc::off_t,
                ) as *mut io_uring_sqe
            };
            if sqes_ptr == libc::MAP_FAILED as *mut io_uring_sqe {
                unsafe {
                    libc::munmap(cq_ring_ptr as *mut c_void, cq_ring_size);
                    libc::munmap(sq_ring_ptr as *mut c_void, sq_ring_size);
                    libc::close(ring_fd);
                }
                return Err(io::Error::last_os_error());
            }

            // Extract ring pointers from offsets
            let sq_head = unsafe { sq_ring_ptr.add(params.sq_off.head as usize) as *const AtomicU32 };
            let sq_tail = unsafe { sq_ring_ptr.add(params.sq_off.tail as usize) as *mut AtomicU32 };
            let sq_mask_val = unsafe { *(sq_ring_ptr.add(params.sq_off.ring_mask as usize) as *const u32) };
            let sq_flags = unsafe { sq_ring_ptr.add(params.sq_off.flags as usize) as *const AtomicU32 };
            let sq_array = unsafe { sq_ring_ptr.add(params.sq_off.array as usize) as *mut u32 };

            let cq_head = unsafe { cq_ring_ptr.add(params.cq_off.head as usize) as *mut AtomicU32 };
            let cq_tail = unsafe { cq_ring_ptr.add(params.cq_off.tail as usize) as *const AtomicU32 };
            let cq_mask_val = unsafe { *(cq_ring_ptr.add(params.cq_off.ring_mask as usize) as *const u32) };
            let cq_cqes = unsafe { cq_ring_ptr.add(params.cq_off.cqes as usize) as *const io_uring_cqe };
            let cq_overflow = unsafe { cq_ring_ptr.add(params.cq_off.overflow as usize) as *const AtomicU32 };

            Ok(Self {
                ring_fd,
                sq_ring_ptr,
                sq_ring_size,
                sq_head,
                sq_tail,
                sq_mask: sq_mask_val,
                sq_flags,
                sq_array,
                sqes_ptr,
                sqes_size,
                sq_entries,
                cq_ring_ptr,
                cq_ring_size,
                cq_head,
                cq_tail,
                cq_mask: cq_mask_val,
                cq_cqes,
                cq_overflow,
            })
        }

        /// Get the next available SQE slot. Returns None if the SQ ring is full.
        #[inline(always)]
        pub fn get_sqe(&mut self) -> Option<&mut io_uring_sqe> {
            let head = unsafe { (*self.sq_head).load(Ordering::Acquire) };
            let tail = unsafe { (*self.sq_tail).load(Ordering::Relaxed) };
            let next = tail.wrapping_add(1);

            if next.wrapping_sub(head) > self.sq_entries {
                return None; // SQ ring full
            }

            let idx = tail & self.sq_mask;
            // Write the SQE index into the SQ array
            unsafe { *self.sq_array.add(idx as usize) = idx; }
            let sqe = unsafe { &mut *self.sqes_ptr.add(idx as usize) };
            // Zero-init the SQE
            unsafe { ptr::write_bytes(sqe as *mut io_uring_sqe, 0, 1); }
            // Advance SQ tail
            unsafe { (*self.sq_tail).store(next, Ordering::Release); }
            Some(sqe)
        }

        /// Submit all pending SQEs to the kernel. Returns number of SQEs submitted.
        /// For SQPOLL mode, this may not need to enter the kernel if the polling thread
        /// is still active.
        #[inline]
        pub fn submit(&self) -> io::Result<usize> {
            // With SQPOLL, check if kernel thread needs wakeup
            let flags = unsafe { (*self.sq_flags).load(Ordering::Acquire) };
            let enter_flags = if (flags & IORING_SQ_NEED_WAKEUP) != 0 {
                IORING_ENTER_SQ_WAKEUP
            } else {
                0
            };

            // If SQPOLL is active and kernel thread is running, no syscall needed
            if enter_flags == 0 {
                // Check if there's anything to submit
                let head = unsafe { (*self.sq_head).load(Ordering::Acquire) };
                let tail = unsafe { (*self.sq_tail).load(Ordering::Relaxed) };
                return Ok(tail.wrapping_sub(head) as usize);
            }

            let to_submit = {
                let head = unsafe { (*self.sq_head).load(Ordering::Acquire) };
                let tail = unsafe { (*self.sq_tail).load(Ordering::Relaxed) };
                tail.wrapping_sub(head)
            };

            let ret = unsafe {
                libc::syscall(
                    SYS_IO_URING_ENTER,
                    self.ring_fd,
                    to_submit,
                    0u32,          // min_complete
                    enter_flags,
                    ptr::null::<c_void>(),
                    0usize,        // sigset size
                ) as c_int
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINTR) {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(ret as usize)
        }

        /// Submit all pending SQEs and wait for at least `min_complete` CQEs.
        /// This is the primary blocking call in the event loop.
        #[inline]
        pub fn submit_and_wait(&self, min_complete: u32) -> io::Result<usize> {
            let to_submit = {
                let head = unsafe { (*self.sq_head).load(Ordering::Acquire) };
                let tail = unsafe { (*self.sq_tail).load(Ordering::Relaxed) };
                tail.wrapping_sub(head)
            };

            let mut enter_flags = IORING_ENTER_GETEVENTS;

            // Check SQPOLL wakeup
            let sq_flags = unsafe { (*self.sq_flags).load(Ordering::Acquire) };
            if (sq_flags & IORING_SQ_NEED_WAKEUP) != 0 {
                enter_flags |= IORING_ENTER_SQ_WAKEUP;
            }

            let ret = unsafe {
                libc::syscall(
                    SYS_IO_URING_ENTER,
                    self.ring_fd,
                    to_submit,
                    min_complete,
                    enter_flags,
                    ptr::null::<c_void>(),
                    0usize,
                ) as c_int
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINTR) {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(ret as usize)
        }

        /// Non-blocking peek at the next CQE. Returns None if no completions available.
        #[inline(always)]
        pub fn peek_cqe(&self) -> Option<io_uring_cqe> {
            let head = unsafe { (*self.cq_head).load(Ordering::Acquire) };
            let tail = unsafe { (*self.cq_tail).load(Ordering::Acquire) };
            if head == tail {
                return None;
            }
            let idx = head & self.cq_mask;
            let cqe = unsafe { *self.cq_cqes.add(idx as usize) };
            Some(cqe)
        }

        /// Advance the CQ head by `count` entries after processing CQEs.
        #[inline(always)]
        pub fn advance_cq(&self, count: u32) {
            let head = unsafe { (*self.cq_head).load(Ordering::Relaxed) };
            unsafe { (*self.cq_head).store(head.wrapping_add(count), Ordering::Release) };
        }

        /// Check for CQ overflow (kernel dropped CQEs).
        #[inline]
        pub fn cq_overflow(&self) -> u32 {
            unsafe { (*self.cq_overflow).load(Ordering::Relaxed) }
        }

        /// Register pre-allocated buffers with the ring for OP_READ_FIXED/OP_WRITE_FIXED.
        pub fn register_buffers(&self, iovecs: &[libc::iovec]) -> io::Result<()> {
            let ret = unsafe {
                libc::syscall(
                    SYS_IO_URING_REGISTER,
                    self.ring_fd,
                    IORING_REGISTER_BUFFERS,
                    iovecs.as_ptr(),
                    iovecs.len() as c_uint,
                ) as c_int
            };
            if ret < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        /// Get the ring file descriptor (for probing features).
        pub fn fd(&self) -> c_int {
            self.ring_fd
        }
    }

    impl Drop for UringRing {
        fn drop(&mut self) {
            unsafe {
                libc::munmap(self.sqes_ptr as *mut c_void, self.sqes_size);
                libc::munmap(self.cq_ring_ptr as *mut c_void, self.cq_ring_size);
                libc::munmap(self.sq_ring_ptr as *mut c_void, self.sq_ring_size);
                libc::close(self.ring_fd);
            }
        }
    }

    // ---- SQE Preparation Helpers ----
    // All inline, zero-allocation, write directly into SQE memory.

    /// Prepare an accept SQE. The accepted fd is returned in the CQE res field.
    #[inline(always)]
    pub fn prep_accept(sqe: &mut io_uring_sqe, listen_fd: i32, user_data: u64) {
        sqe.opcode = IORING_OP_ACCEPT;
        sqe.fd = listen_fd;
        sqe.user_data = user_data;
        // addr=NULL, addrlen=NULL → don't care about peer address
    }

    /// Prepare a multi-shot accept SQE (kernel ≥5.19).
    /// Generates one CQE per accepted connection without re-submission.
    /// CQE will have IORING_CQE_F_MORE flag set if more accepts will follow.
    #[inline(always)]
    pub fn prep_accept_multishot(sqe: &mut io_uring_sqe, listen_fd: i32, user_data: u64) {
        sqe.opcode = IORING_OP_ACCEPT;
        sqe.fd = listen_fd;
        sqe.user_data = user_data;
        sqe.ioprio = IORING_ACCEPT_MULTISHOT;
    }

    /// Prepare a read SQE. Reads into the buffer at `buf_ptr` up to `len` bytes.
    #[inline(always)]
    pub fn prep_read(sqe: &mut io_uring_sqe, fd: i32, buf_ptr: *mut u8, len: u32, user_data: u64) {
        sqe.opcode = IORING_OP_READ;
        sqe.fd = fd;
        sqe.addr_or_splice = buf_ptr as u64;
        sqe.len = len;
        sqe.user_data = user_data;
    }

    /// Prepare a read from a pre-registered buffer (A.4: fixed buffers).
    /// `buf_index` is the index into the array passed to `register_buffers()`.
    #[inline(always)]
    pub fn prep_read_fixed(
        sqe: &mut io_uring_sqe, fd: i32, buf_ptr: *mut u8, len: u32,
        offset: u64, buf_index: u16, user_data: u64,
    ) {
        sqe.opcode = IORING_OP_READ_FIXED;
        sqe.fd = fd;
        sqe.addr_or_splice = buf_ptr as u64;
        sqe.len = len;
        sqe.off_or_addr2 = offset;
        sqe.buf_index_or_group = buf_index;
        sqe.user_data = user_data;
    }

    /// Prepare a write SQE.
    #[inline(always)]
    pub fn prep_write(sqe: &mut io_uring_sqe, fd: i32, buf_ptr: *const u8, len: u32, user_data: u64) {
        sqe.opcode = IORING_OP_WRITE;
        sqe.fd = fd;
        sqe.addr_or_splice = buf_ptr as u64;
        sqe.len = len;
        sqe.user_data = user_data;
    }

    /// Prepare a write from a pre-registered buffer (A.4: fixed buffers).
    #[inline(always)]
    pub fn prep_write_fixed(
        sqe: &mut io_uring_sqe, fd: i32, buf_ptr: *const u8, len: u32,
        offset: u64, buf_index: u16, user_data: u64,
    ) {
        sqe.opcode = IORING_OP_WRITE_FIXED;
        sqe.fd = fd;
        sqe.addr_or_splice = buf_ptr as u64;
        sqe.len = len;
        sqe.off_or_addr2 = offset;
        sqe.buf_index_or_group = buf_index;
        sqe.user_data = user_data;
    }

    /// Prepare a writev SQE (scatter-gather write).
    #[inline(always)]
    pub fn prep_writev(
        sqe: &mut io_uring_sqe,
        fd: i32,
        iovecs: *const libc::iovec,
        iov_count: u32,
        user_data: u64,
    ) {
        sqe.opcode = IORING_OP_WRITEV;
        sqe.fd = fd;
        sqe.addr_or_splice = iovecs as u64;
        sqe.len = iov_count;
        sqe.user_data = user_data;
    }

    /// Prepare a close SQE (async close — avoids blocking).
    #[inline(always)]
    pub fn prep_close(sqe: &mut io_uring_sqe, fd: i32, user_data: u64) {
        sqe.opcode = IORING_OP_CLOSE;
        sqe.fd = fd;
        sqe.user_data = user_data;
    }

    /// Prepare a NOP SQE (useful for testing / wakeup).
    #[inline(always)]
    pub fn prep_nop(sqe: &mut io_uring_sqe, user_data: u64) {
        sqe.opcode = IORING_OP_NOP;
        sqe.user_data = user_data;
    }

    /// Prepare a splice SQE (A.5: zero-copy sendfile via io_uring).
    /// Splices `len` bytes from `fd_in` (at `off_in`) into `fd_out`.
    /// Use `-1` for `off_in` to read from the current file position.
    #[inline(always)]
    pub fn prep_splice(
        sqe: &mut io_uring_sqe,
        fd_in: i32,
        off_in: i64,
        fd_out: i32,
        off_out: i64,
        len: u32,
        user_data: u64,
    ) {
        sqe.opcode = IORING_OP_SPLICE;
        sqe.fd = fd_out;
        sqe.addr_or_splice = off_in as u64; // splice_off_in
        sqe.len = len;
        sqe.off_or_addr2 = off_out as u64;
        sqe.splice_fd_in_or_file_index = fd_in;
        sqe.user_data = user_data;
    }
}
