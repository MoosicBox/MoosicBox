use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{self},
    marker::PhantomData,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    str::FromStr,
    sync::{
        RwLock,
        atomic::{AtomicU16, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use scoped_tls::scoped_thread_local;
use switchy_async::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc::{Receiver, Sender, error::TrySendError},
    time,
    util::CancellationToken,
};

use crate::{
    Error, GenericTcpListener, GenericTcpStream, GenericTcpStreamReadHalf,
    GenericTcpStreamWriteHalf,
};

thread_local! {
    #[allow(clippy::type_complexity)]
    static TCP_LISTENERS: RefCell<RwLock<BTreeMap<SocketAddr, flume::Sender<(TcpStream, SocketAddr)>>>> =
        const { RefCell::new(RwLock::new(BTreeMap::new())) };

    static EPHEMERAL_PORT_START: RefCell<AtomicU16> =
        const { RefCell::new(AtomicU16::new(40000)) };
    static NEXT_PORT: RefCell<AtomicU16> = RefCell::new(AtomicU16::new(ephemeral_port_start()));


    static IP_START: RefCell<Ipv4Addr> =
        const { RefCell::new(Ipv4Addr::new(192, 168, 1, 1)) };
    static NEXT_IP: RefCell<Ipv4Addr> = RefCell::new(ip_start());

    static DNS: RefCell<BTreeMap<String, Ipv4Addr>> = const { RefCell::new(BTreeMap::new()) };
}

#[must_use]
pub fn ephemeral_port_start() -> u16 {
    EPHEMERAL_PORT_START.with_borrow(|x| x.load(Ordering::SeqCst))
}

pub fn reset_next_port() {
    NEXT_PORT.with_borrow(|x| {
        x.store(ephemeral_port_start(), Ordering::SeqCst);
    });
}

#[must_use]
pub fn next_port() -> u16 {
    NEXT_PORT.with_borrow(|x| {
        let mut port = x.fetch_add(1, Ordering::SeqCst);

        if port == u16::MAX {
            port = ephemeral_port_start();
            x.store(port, Ordering::SeqCst);
        }

        port
    })
}

/// # Panics
///
/// * If ran out of 3rd octet available IPs
#[must_use]
pub fn next_ip() -> Ipv4Addr {
    NEXT_IP.with_borrow_mut(|x| {
        let mut octets = x.octets();

        if octets[3] == u8::MAX {
            assert!(octets[2] < u8::MAX, "ran out of available IPs");

            octets[2] += 1;
            octets[3] = 1;
        } else {
            octets[3] += 1;
        }

        let current = *x;

        *x = Ipv4Addr::from(octets);

        current
    })
}

#[must_use]
pub fn ip_start() -> Ipv4Addr {
    IP_START.with_borrow(|x| *x)
}

pub fn reset_next_ip() {
    NEXT_IP.with_borrow_mut(|x| {
        *x = ip_start();
    });
}

pub fn reset_dns() {
    DNS.with_borrow_mut(BTreeMap::clear);
}

pub fn reset() {
    reset_next_port();
    reset_next_ip();
    reset_dns();
}

struct Host {
    addr: String,
}

scoped_thread_local! {
    static HOST: Host
}

#[must_use]
pub fn current_host() -> Option<String> {
    if HOST.is_set() {
        Some(HOST.with(|x| x.addr.to_string()))
    } else {
        None
    }
}

pub fn with_host<T>(addr: String, f: impl FnOnce(&str) -> T) -> T {
    let host = Host { addr };
    HOST.set(&host, || f(&host.addr))
}

fn is_local_host_name(addr: &str) -> bool {
    matches!(addr, "0.0.0.0" | "127.0.0.1")
}

fn parse_addr(mut addr: String, host: bool) -> Result<(SocketAddr, Option<String>), crate::Error> {
    Ok(if let Some(index) = addr.rfind(':') {
        let port: u16 = addr.split_off(index)[1..].parse()?;
        let mut host_name = addr;

        if host {
            if HOST.is_set() {
                if is_local_host_name(&host_name) {
                    host_name = HOST.with(|x| x.addr.clone());
                } else {
                    unimplemented!("host-local networking not implemented yet")
                }
            }

            let ip = DNS.with_borrow_mut(|x| {
                if x.contains_key(&host_name) {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        format!(
                            "Address in use: {host_name} {}",
                            std::backtrace::Backtrace::force_capture()
                        ),
                    ));
                }

                let ip = Ipv4Addr::from_str(&host_name).ok().unwrap_or_else(next_ip);

                log::debug!("inserting ip={ip} for host_name={host_name}");
                x.insert(host_name.clone(), ip);

                Ok(ip)
            })?;

            return Ok((SocketAddr::new(ip.into(), port), Some(host_name)));
        }

        let ip = DNS.with_borrow(|x| x.get(&host_name).copied());
        log::debug!("ip={ip:?} from host_name={host_name}");

        if let Some(ip) = ip {
            return Ok((SocketAddr::new(ip.into(), port), Some(host_name)));
        }

        SocketAddr::from_str(&host_name)
            .map(|x| (x, Some(host_name)))
            .map_err(|_| io::Error::new(io::ErrorKind::HostUnreachable, "Host unreachable"))?
    } else {
        (SocketAddr::from_str(&addr)?, None)
    })
}

pub struct TcpListener {
    token: CancellationToken,
    addr: SocketAddr,
    // Handles listening for incoming connections
    rx: flume::Receiver<(TcpStream, SocketAddr)>,
}

impl TcpListener {
    /// # Errors
    ///
    /// * If the `TcpListener` fails to bind the address
    ///
    /// # Panics
    ///
    /// * If fails to bind the new listener address to the `TCP_LISTENERS` `RwLock`
    pub async fn bind(addr: impl Into<String>) -> Result<Self, crate::Error> {
        async {
            let (tx, rx) = flume::bounded(64);
            let token = CancellationToken::new();
            let addr = addr.into();
            log::debug!("Binding TCP listener to addr={addr}");

            let (addr, _host_name) = parse_addr(addr, true)?;

            TCP_LISTENERS.with_borrow_mut(|x| x.write().unwrap().insert(addr, tx));

            let listener = Self { token, addr, rx };

            assert!(listener.rx.is_empty());
            assert!(TCP_LISTENERS.with_borrow(|x| x.read().unwrap().contains_key(&listener.addr)));
            assert!(!listener.token.is_cancelled());

            Ok(listener)
        }
        .await
    }

    /// # Panics
    ///
    /// * If the `CancellationToken` is already cancelled
    /// * If the `CancellationToken` fails to cancel
    /// * If the `TCP_LISTENERS` `RwLock` fails to remove the listener
    pub fn shutdown(self) {
        self.shutdown_inner();
    }

    /// # Panics
    ///
    /// * If the `CancellationToken` is already cancelled
    /// * If the `CancellationToken` fails to cancel
    /// * If the `TCP_LISTENERS` `RwLock` fails to remove the listener
    fn shutdown_inner(&self) {
        log::debug!("Shutting down TCP listener at addr={}", self.addr);

        assert!(!self.token.is_cancelled());

        self.token.cancel();
        assert!(self.token.is_cancelled());

        TCP_LISTENERS.with_borrow_mut(|x| x.write().unwrap().remove(&self.addr));
        assert!(TCP_LISTENERS.with_borrow(|x| !x.read().unwrap().contains_key(&self.addr)));
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        if self.token.is_cancelled() {
            return;
        }
        self.shutdown_inner();
    }
}

impl crate::SimulatorTcpListener {
    /// # Errors
    ///
    /// * If the `TcpListener` fails to bind the address
    pub async fn bind(addr: impl Into<String>) -> Result<Self, Error> {
        Ok(Self(
            TcpListener::bind(addr).await?,
            PhantomData,
            PhantomData,
            PhantomData,
        ))
    }
}

#[async_trait]
impl GenericTcpListener<crate::SimulatorTcpStream> for TcpListener {
    async fn accept(&self) -> Result<(crate::SimulatorTcpStream, SocketAddr), crate::Error> {
        log::debug!("Accepting connection on TCP listener at addr={}", self.addr);
        self.rx
            .recv_async()
            .await
            .map_err(|e| {
                crate::Error::IO(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("Connection refused: {e:?}"),
                ))
            })
            .map(|(stream, addr)| {
                log::debug!(
                    "Accepted connection from addr={addr} on TCP listener at addr={}",
                    self.addr
                );
                (
                    crate::TcpStreamWrapper(stream, PhantomData, PhantomData),
                    addr,
                )
            })
    }
}

pub struct TcpStream {
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    read_half: TcpStreamReadHalf,
    write_half: TcpStreamWriteHalf,
}

impl TcpStream {
    /// # Errors
    ///
    /// * If the underlying `TcpStream` fails to connect
    ///
    /// # Panics
    ///
    /// * If the `TCP_LISTENERS` `RwLock` fails to read
    pub async fn connect(server_addr: impl Into<String>) -> io::Result<Self> {
        let server_addr = server_addr.into();
        log::debug!("Connecting to server at server_addr={server_addr}");

        let client_port = next_port();
        let client_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), client_port);
        let (peer_addr, _host_name) = parse_addr(server_addr, false).map_err(|e| match e {
            Error::IO(e) => e,
            Error::AddrParse(..) | Error::ParseInt(..) | Error::Send => io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to connect: {e:?}"),
            ),
        })?;

        let (tx1, rx1) = tokio::sync::mpsc::channel(16);
        let (tx2, rx2) = tokio::sync::mpsc::channel(16);

        let stream_for_client = Self {
            local_addr: client_addr,
            peer_addr,
            read_half: TcpStreamReadHalf {
                rx: rx2,
                read_buf: BytesMut::new(),
            },
            write_half: TcpStreamWriteHalf { tx: tx1 },
        };

        let stream_for_server = Self {
            local_addr: peer_addr,
            peer_addr: client_addr,
            read_half: TcpStreamReadHalf {
                rx: rx1,
                read_buf: BytesMut::new(),
            },
            write_half: TcpStreamWriteHalf { tx: tx2 },
        };

        let connect_tx = TCP_LISTENERS
            .with_borrow(|x| x.read().unwrap().get(&peer_addr).cloned())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("Failed to connect to {peer_addr}"),
                )
            })?;

        // Allow the async runtime to switch to a different Task before sending the connection
        time::sleep(Duration::from_nanos(0)).await;

        connect_tx
            .try_send((stream_for_server, client_addr))
            .map_err(|e| match e {
                flume::TrySendError::Full(..) => {
                    io::Error::new(io::ErrorKind::ConnectionRefused, "Connection queue is full")
                }
                flume::TrySendError::Disconnected(..) => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "Receiver dropped")
                }
            })?;

        Ok(stream_for_client)
    }
}

impl GenericTcpStream<TcpStreamReadHalf, TcpStreamWriteHalf> for TcpStream {
    fn into_split(self) -> (TcpStreamReadHalf, TcpStreamWriteHalf) {
        (self.read_half, self.write_half)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}

pub struct TcpStreamReadHalf {
    /// Receiver for receiving data from the peer
    rx: Receiver<Bytes>,
    read_buf: BytesMut,
}
impl GenericTcpStreamReadHalf for TcpStreamReadHalf {}

pub struct TcpStreamWriteHalf {
    /// Sender for sending data to the peer
    tx: Sender<Bytes>,
}
impl GenericTcpStreamWriteHalf for TcpStreamWriteHalf {}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.get_mut().read_half), cx, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().write_half), cx, data)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().write_half), cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.get_mut().write_half), cx)
    }
}

impl AsyncRead for TcpStreamReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        if !this.read_buf.is_empty() {
            let to_copy = std::cmp::min(buf.remaining(), this.read_buf.len());
            buf.put_slice(&this.read_buf.split_to(to_copy));
            return Poll::Ready(Ok(()));
        }

        match Pin::new(&mut this.rx).poll_recv(cx) {
            Poll::Ready(Some(bytes)) => {
                if bytes.len() < 100 {
                    log::trace!("Received {} bytes ({bytes:?})", bytes.len());
                } else {
                    log::trace!("Received {} bytes", bytes.len());
                }
                this.read_buf.extend_from_slice(&bytes);
                let to_copy = std::cmp::min(buf.remaining(), this.read_buf.len());
                let data = this.read_buf.split_to(to_copy);
                if data.len() < 100 {
                    log::trace!("put_slice ({data:?})");
                }
                buf.put_slice(&data);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                log::trace!("Received empty response");
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for TcpStreamWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let tx = &self.tx;
        let bytes = Bytes::copy_from_slice(data);
        let len = bytes.len();

        log::trace!("Sending bytes={bytes:?}");
        match tx.try_send(bytes) {
            Ok(()) => {
                log::trace!("Sent {len} bytes");
                Poll::Ready(Ok(data.len()))
            }
            Err(TrySendError::Full(..)) => {
                log::trace!("Sender full, cannot send {len} bytes");
                Poll::Pending
            }
            Err(TrySendError::Closed(..)) => {
                log::trace!("Sender closed, cannot send {len} bytes");
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "receiver dropped",
                )))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        log::trace!("poll_flush");
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        log::trace!("poll_shutdown");
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod test {
    use std::{net::Ipv4Addr, sync::LazyLock};

    use pretty_assertions::{assert_eq, assert_ne};
    use serial_test::serial;
    use switchy_async::{runtime, task};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_can_bind() {
        let addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await;
        assert!(
            listener.is_ok(),
            "Failed to bind TcpListener: {:?}",
            listener.err()
        );
    }

    #[tokio::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_after_bind_exists_in_tcp_listener() {
        let addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);

        let _listener = TcpListener::bind(addr.to_string()).await.unwrap();
        TCP_LISTENERS.with_borrow_mut(|x| {
            assert!(
                x.read().unwrap().contains_key(&addr),
                "TcpListener should exist in TCP_LISTENERS"
            );
        });
    }

    #[tokio::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_addr_matches_bind_addr() {
        let addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await.unwrap();
        assert_eq!(listener.addr, addr, "TcpListener address mismatch");
    }

    #[tokio::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_rx_is_empty_initially() {
        let addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await.unwrap();
        assert!(
            listener.rx.is_empty(),
            "TcpListener receiver should be empty initially"
        );
    }

    #[tokio::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_shutdown_removes_from_tcp_listeners() {
        let addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await.unwrap();
        listener.shutdown();
        TCP_LISTENERS.with_borrow_mut(|x| {
            assert!(
                !x.read().unwrap().contains_key(&addr),
                "TcpListener should be removed from TCP_LISTENERS"
            );
        });
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_can_send_message_to_server() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::new(127, 0, 0, 1));

                let mut buf = vec![];

                let count = stream.read_to_end(&mut buf).await.unwrap();
                assert_eq!(count, 3);

                let bytes = &buf[0..count];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "hey");
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();

            connection.write_all(b"hey").await.unwrap();
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_can_send_two_messages_to_server() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::new(127, 0, 0, 1));

                let mut buf = vec![];

                let count = stream.read_to_end(&mut buf).await.unwrap();
                assert_eq!(count, 6);

                let bytes = &buf[0..3];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "hey");

                let bytes = &buf[3..count];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "sup");
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();

            connection.write_all(b"hey").await.unwrap();
            connection.write_all(b"sup").await.unwrap();
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_can_handle_multiple_stream_connections() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::new(127, 0, 0, 1));

                let mut buf = vec![];

                let count = stream.read_to_end(&mut buf).await.unwrap();
                assert_eq!(count, 6);

                let bytes = &buf[0..3];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "hey");

                let bytes = &buf[3..count];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "sup");

                // Second connection
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::new(127, 0, 0, 1));

                let mut buf = vec![];

                let count = stream.read_to_end(&mut buf).await.unwrap();
                assert_eq!(count, 8);

                let bytes = &buf[0..4];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "hey2");

                let bytes = &buf[4..count];
                log::debug!("Received bytes={bytes:?}");
                let value = String::from_utf8(bytes.to_vec()).unwrap();
                assert_eq!(value, "sup2");
            });

            let mut c1 = TcpStream::connect(server_addr.to_string()).await.unwrap();
            assert_eq!(c1.peer_addr, server_addr);

            c1.write_all(b"hey").await.unwrap();
            c1.write_all(b"sup").await.unwrap();

            let mut c2 = TcpStream::connect(server_addr.to_string()).await.unwrap();
            assert_eq!(c2.peer_addr, server_addr);
            assert_ne!(c2.local_addr, c1.local_addr);

            c2.write_all(b"hey2").await.unwrap();
            c2.write_all(b"sup2").await.unwrap();
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_should_recycle_ephemeral_ports() {
        static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(TOKEN.run_until_cancelled(async move {
                while let Ok((_, addr)) = listener.accept().await {
                    log::debug!("client connected at addr={addr}");
                }
            }));

            task::spawn(async move {
                for i in 0..=(u32::from(u16::MAX) + 1) {
                    log::debug!("client {i} connecting");
                    TcpStream::connect(server_addr.to_string()).await.unwrap();
                }

                TOKEN.cancel();
            });
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_should_error_if_connection_queue_is_full() {
        static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
            let _listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            #[allow(clippy::collection_is_never_read)]
            let mut connections = vec![];

            for i in 0..64 {
                log::debug!("client {i} connecting");
                connections.push(TcpStream::connect(server_addr.to_string()).await.unwrap());
            }

            assert_eq!(
                TcpStream::connect(server_addr.to_string())
                    .await
                    .map_err(|e| e.kind())
                    .err(),
                Some(io::ErrorKind::ConnectionRefused)
            );

            TOKEN.cancel();
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }
}
