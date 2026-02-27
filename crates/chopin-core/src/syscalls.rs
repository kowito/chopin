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
    use libc::{EPOLLET, EPOLLIN, EPOLLOUT, epoll_event};

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
                Ok(0) // Need to wait for more data
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

// ---- Accept-Distribute Pipe Operations ----

/// Create a non-blocking Unix pipe. Returns (read_fd, write_fd).
pub fn create_pipe() -> ChopinResult<(c_int, c_int)> {
    let mut fds = [0 as c_int; 2];
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) < 0 {
            return Err(io::Error::last_os_error().into());
        }
        // Make read end non-blocking
        let flags = libc::fcntl(fds[0], libc::F_GETFL, 0);
        if flags < 0 || libc::fcntl(fds[0], libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fds[0]);
            libc::close(fds[1]);
            return Err(err.into());
        }
    }
    Ok((fds[0], fds[1]))
}

/// Send a client FD over a pipe (blocking write of 4 bytes)
pub fn send_fd_over_pipe(pipe_write_fd: c_int, client_fd: c_int) -> ChopinResult<()> {
    let bytes = client_fd.to_ne_bytes();
    unsafe {
        let n = libc::write(pipe_write_fd, bytes.as_ptr() as *const c_void, 4);
        if n < 0 {
            Err(io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }
}

/// Receive a client FD from a pipe (non-blocking read of 4 bytes)
pub fn recv_fd_from_pipe(pipe_read_fd: c_int) -> ChopinResult<Option<c_int>> {
    let mut buf = [0u8; 4];
    unsafe {
        let n = libc::read(pipe_read_fd, buf.as_mut_ptr() as *mut c_void, 4);
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                Ok(None)
            } else {
                Err(err.into())
            }
        } else if n == 4 {
            Ok(Some(c_int::from_ne_bytes(buf)))
        } else {
            Ok(None) // Partial read, unlikely with 4 bytes
        }
    }
}
