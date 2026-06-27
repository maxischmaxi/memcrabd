use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream, UnixListener, UnixStream},
    sync::Notify,
};

pub struct ConnectionLimiter {
    max: u64,
    maxconns_fast: bool,
    curr: AtomicU64,
    total: AtomicU64,
    rejected: AtomicU64,
    listen_disabled_num: AtomicU64,
    accepting: AtomicBool,
    notify: Notify,
}

pub struct ConnectionGuard {
    limiter: Arc<ConnectionLimiter>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

pub trait Accept {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;
    type Addr;

    fn accept(&self) -> impl std::future::Future<Output = std::io::Result<(Self::Stream, Self::Addr)>>;
}

impl Accept for TcpListener {
    type Stream = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&self) -> std::io::Result<(Self::Stream, Self::Addr)> {
        TcpListener::accept(self).await
    }
}

impl Accept for UnixListener {
    type Stream = UnixStream;
    type Addr = tokio::net::unix::SocketAddr;

    async fn accept(&self) -> std::io::Result<(Self::Stream, Self::Addr)> {
        UnixListener::accept(self).await
    }
}

impl ConnectionLimiter {
    pub fn new(max: u64, maxconns_fast: bool) -> Self {
        Self {
            max,
            maxconns_fast,
            curr: AtomicU64::new(0),
            total: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            listen_disabled_num: AtomicU64::new(0),
            accepting: AtomicBool::new(true),
            notify: Notify::new(),
        }
    }

    pub async fn wait_until_accepting(&self) {
        if !self.accepting.load(Ordering::Acquire) {
            self.notify.notified().await;
        }
    }

    pub async fn try_acquire(self: &Arc<Self>) -> Option<ConnectionGuard> {
        self.total.fetch_add(1, Ordering::Relaxed);

        if self.max == 0 {
            self.curr.fetch_add(1, Ordering::Relaxed);
            return Some(ConnectionGuard {
                limiter: self.clone(),
            });
        }

        let curr = self.curr.fetch_add(1, Ordering::Relaxed) + 1;

        if curr > self.max {
            // Limit überschritten
            self.curr.fetch_sub(1, Ordering::Relaxed);

            if self.maxconns_fast {
                // sofort abweisen
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return None;
            } else {
                // Default-Modus: Verbindung wird bedient,
                // aber Listener wird pausiert
                self.accepting.store(false, Ordering::Release);
                self.listen_disabled_num.fetch_add(1, Ordering::Relaxed);
                // curr wurde oben schon wieder dekrementiert –
                // aber wir behalten die Verbindung! Also wieder inkrementieren.
                self.curr.fetch_add(1, Ordering::Relaxed);
            }
        }

        Some(ConnectionGuard {
            limiter: self.clone(),
        })
    }

    pub fn release(&self) {
        let prev = self.curr.fetch_sub(1, Ordering::Relaxed);
        let now = prev - 1;

        if self.max != 0 && now < self.max && !self.accepting.load(Ordering::Acquire) {
            self.accepting.store(true, Ordering::Release);
            self.notify.notify_one();
        }
    }

    pub fn curr_connections(&self) -> u64 {
        self.curr.load(Ordering::Relaxed)
    }
    pub fn total_connections(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
    pub fn rejected_connections(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }
    pub fn listen_disabled_num(&self) -> u64 {
        self.listen_disabled_num.load(Ordering::Relaxed)
    }
    pub fn is_accepting(&self) -> bool {
        self.accepting.load(Ordering::Relaxed)
    }
    pub fn max_connections(&self) -> u64 {
        self.max
    }
}
