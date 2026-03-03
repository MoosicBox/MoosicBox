//! In-memory TCP simulator for testing.
//!
//! This module provides an in-memory implementation of TCP streams and listeners that
//! simulates network behavior without actual network I/O. It includes features like:
//!
//! * In-memory connection handling
//! * DNS simulation for hostname resolution
//! * Ephemeral port management
//! * Connection queue management
//!
//! This is useful for deterministic testing, avoiding port conflicts, and testing
//! network code without requiring actual network access.

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
    sync::mpsc::{Receiver, Sender, TrySendError},
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

/// Returns the starting port number for ephemeral port allocation.
///
/// Ephemeral ports are automatically assigned to client connections when no specific port
/// is requested.
#[must_use]
pub fn ephemeral_port_start() -> u16 {
    EPHEMERAL_PORT_START.with_borrow(|x| x.load(Ordering::SeqCst))
}

/// Resets the ephemeral port counter to its starting value.
///
/// This is useful for test isolation to ensure deterministic port allocation.
pub fn reset_next_port() {
    NEXT_PORT.with_borrow(|x| {
        x.store(ephemeral_port_start(), Ordering::SeqCst);
    });
}

/// Allocates and returns the next available ephemeral port number.
///
/// Port numbers automatically wrap around to the ephemeral port start when reaching
/// the maximum port value.
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

/// Allocates and returns the next available IP address.
///
/// IP addresses are allocated sequentially starting from the configured IP start value.
/// This is useful for simulating multiple hosts in tests.
///
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

/// Returns the starting IP address for IP allocation.
///
/// This is the first IP address that will be allocated when simulating connections.
#[must_use]
pub fn ip_start() -> Ipv4Addr {
    IP_START.with_borrow(|x| *x)
}

/// Resets the IP address counter to its starting value.
///
/// This is useful for test isolation to ensure deterministic IP allocation.
pub fn reset_next_ip() {
    NEXT_IP.with_borrow_mut(|x| {
        *x = ip_start();
    });
}

/// Clears all DNS hostname-to-IP mappings.
///
/// This is useful for test isolation to ensure a clean DNS state.
pub fn reset_dns() {
    DNS.with_borrow_mut(BTreeMap::clear);
}

/// Resets all simulator state.
///
/// This includes ephemeral ports, IP addresses, and DNS mappings. Useful for ensuring
/// a clean state between tests.
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

/// Returns the current host address if one is set in the current scope.
///
/// This is used by [`with_host`] to provide scoped hostname context for connections.
#[must_use]
pub fn current_host() -> Option<String> {
    if HOST.is_set() {
        Some(HOST.with(|x| x.addr.clone()))
    } else {
        None
    }
}

/// Executes a closure with a specific host address set in the current scope.
///
/// This allows connections to localhost addresses within the closure to be resolved
/// to the specified host address instead.
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

/// In-memory TCP listener for the simulator.
///
/// Listens for incoming TCP connections in the simulator without actual network I/O.
/// Maintains a queue of pending connections and automatically manages the listener
/// lifecycle through a cancellation token.
pub struct TcpListener {
    token: CancellationToken,
    addr: SocketAddr,
    // Handles listening for incoming connections
    rx: flume::Receiver<(TcpStream, SocketAddr)>,
}

impl TcpListener {
    /// Binds a TCP listener to the specified address in the simulator.
    ///
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

    /// Shuts down the TCP listener and removes it from the simulator registry.
    ///
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
    /// Binds a wrapped TCP listener to the specified address in the simulator.
    ///
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

/// In-memory TCP stream for the simulator.
///
/// Represents a bidirectional TCP connection in the simulator. Data is transferred
/// through in-memory channels rather than actual network sockets.
pub struct TcpStream {
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    read_half: TcpStreamReadHalf,
    write_half: TcpStreamWriteHalf,
}

impl TcpStream {
    /// Connects to a remote TCP server at the specified address in the simulator.
    ///
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
        let client_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), client_port);
        let (peer_addr, _host_name) = parse_addr(server_addr, false).map_err(|e| match e {
            Error::IO(e) => e,
            Error::AddrParse(..) | Error::ParseInt(..) | Error::Send => io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed to connect: {e:?}"),
            ),
        })?;

        // FIXME: use mpmc::bounded when it's implemented
        // let (tx1, rx1) = switchy_async::sync::mpsc::bounded(16);
        // let (tx2, rx2) = switchy_async::sync::mpsc::bounded(16);

        let (tx1, rx1) = switchy_async::sync::mpsc::unbounded();
        let (tx2, rx2) = switchy_async::sync::mpsc::unbounded();

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

/// Read half of an in-memory TCP stream.
///
/// Receives data from the peer through an in-memory channel. Implements buffering
/// to handle partial reads efficiently.
pub struct TcpStreamReadHalf {
    /// Receiver for receiving data from the peer
    rx: Receiver<Bytes>,
    read_buf: BytesMut,
}
impl GenericTcpStreamReadHalf for TcpStreamReadHalf {}

/// Write half of an in-memory TCP stream.
///
/// Sends data to the peer through an in-memory channel.
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
            Err(TrySendError::Disconnected(..)) => {
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

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_can_bind() {
        let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await;
        assert!(
            listener.is_ok(),
            "Failed to bind TcpListener: {:?}",
            listener.err()
        );
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_after_bind_exists_in_tcp_listener() {
        let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

        let _listener = TcpListener::bind(addr.to_string()).await.unwrap();
        TCP_LISTENERS.with_borrow_mut(|x| {
            assert!(
                x.read().unwrap().contains_key(&addr),
                "TcpListener should exist in TCP_LISTENERS"
            );
        });
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_addr_matches_bind_addr() {
        let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await.unwrap();
        assert_eq!(listener.addr, addr, "TcpListener address mismatch");
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_rx_is_empty_initially() {
        let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

        let listener = TcpListener::bind(addr.to_string()).await.unwrap();
        assert!(
            listener.rx.is_empty(),
            "TcpListener receiver should be empty initially"
        );
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_listener_shutdown_removes_from_tcp_listeners() {
        let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

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
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);

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
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);

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
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, addr) = listener.accept().await.unwrap();

                assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);

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

                assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);

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
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
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
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
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

    #[test_log::test]
    #[serial]
    fn tcp_stream_connect_fails_for_nonexistent_hostname() {
        reset_dns();
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            // Try to connect to a hostname that doesn't exist in DNS
            let result = TcpStream::connect("nonexistent.server:9999".to_string()).await;
            assert!(result.is_err());
            if let Err(err) = result {
                assert_eq!(err.kind(), io::ErrorKind::HostUnreachable);
            }
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_connect_fails_for_nonexistent_listener_on_registered_host() {
        reset_dns();
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            // Register a hostname but don't create a listener
            let _result = parse_addr("registered.host:8080".to_string(), true).unwrap();

            // Try to connect to a different port on the registered host
            let result = TcpStream::connect("registered.host:9999".to_string()).await;
            assert!(result.is_err());
            if let Err(err) = result {
                assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused);
            }
        });

        log::debug!("Finished block_on. waiting for Runtime to finish");
        runtime.wait().unwrap();
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_stream_into_split_returns_read_write_halves() {
        let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
        let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

        task::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (_read, _write) = stream.into_split();
            // Successfully split the stream
        });

        let stream = TcpStream::connect(server_addr.to_string()).await.unwrap();
        let (_read_half, _write_half) = stream.into_split();
        // Verify we can split into read and write halves
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_stream_local_addr_returns_correct_address() {
        let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
        let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

        task::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            assert_eq!(stream.local_addr().unwrap(), server_addr);
        });

        let stream = TcpStream::connect(server_addr.to_string()).await.unwrap();
        let local_addr = stream.local_addr().unwrap();
        assert_eq!(local_addr.ip(), Ipv4Addr::LOCALHOST);
        assert!(local_addr.port() >= ephemeral_port_start());
    }

    #[switchy_async::test]
    #[test_log::test]
    #[serial]
    async fn tcp_stream_peer_addr_returns_correct_address() {
        let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
        let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

        task::spawn(async move {
            let (stream, client_addr) = listener.accept().await.unwrap();
            assert_eq!(stream.peer_addr().unwrap(), client_addr);
        });

        let stream = TcpStream::connect(server_addr.to_string()).await.unwrap();
        assert_eq!(stream.peer_addr().unwrap(), server_addr);
    }

    #[test_log::test]
    #[serial]
    fn reset_next_port_resets_to_ephemeral_start() {
        // Allocate some ports
        let _p1 = next_port();
        let _p2 = next_port();
        let p3 = next_port();

        // Reset
        reset_next_port();

        // Next port should be back at the start
        let p4 = next_port();
        assert_eq!(p4, ephemeral_port_start());
        assert_ne!(p3, p4);
    }

    #[test_log::test]
    #[serial]
    fn next_ip_increments_correctly() {
        reset_next_ip();
        let start = ip_start();
        let first = next_ip();
        assert_eq!(first, start);

        let second = next_ip();
        assert_ne!(second, first);

        let expected_second = Ipv4Addr::new(
            start.octets()[0],
            start.octets()[1],
            start.octets()[2],
            start.octets()[3] + 1,
        );
        assert_eq!(second, expected_second);
    }

    #[test_log::test]
    #[serial]
    fn next_ip_wraps_fourth_octet_to_third() {
        reset_next_ip();

        // Manually set IP to near overflow of 4th octet
        NEXT_IP.with_borrow_mut(|x| {
            *x = Ipv4Addr::new(192, 168, 1, 255);
        });

        let current = next_ip();
        assert_eq!(current, Ipv4Addr::new(192, 168, 1, 255));

        let next = next_ip();
        assert_eq!(next, Ipv4Addr::new(192, 168, 2, 1));
    }

    #[test_log::test]
    #[serial]
    fn reset_next_ip_returns_to_start() {
        reset_next_ip();
        let start = ip_start();

        let _ip1 = next_ip();
        let _ip2 = next_ip();

        reset_next_ip();
        let ip3 = next_ip();
        assert_eq!(ip3, start);
    }

    #[test_log::test]
    #[serial]
    fn reset_dns_clears_all_entries() {
        reset_dns();

        // Add some DNS entries by binding
        DNS.with_borrow_mut(|dns| {
            dns.insert("test1.local".to_string(), Ipv4Addr::new(10, 0, 0, 1));
            dns.insert("test2.local".to_string(), Ipv4Addr::new(10, 0, 0, 2));
        });

        DNS.with_borrow(|dns| {
            assert_eq!(dns.len(), 2);
        });

        reset_dns();

        DNS.with_borrow(|dns| {
            assert!(dns.is_empty());
        });
    }

    #[test_log::test]
    #[serial]
    fn reset_clears_all_simulator_state() {
        // Set up some state
        let _p1 = next_port();
        let _p2 = next_port();
        let _ip1 = next_ip();
        DNS.with_borrow_mut(|dns| {
            dns.insert("test.local".to_string(), Ipv4Addr::new(10, 0, 0, 1));
        });

        // Reset everything
        reset();

        // Verify state is reset
        let port = next_port();
        assert_eq!(port, ephemeral_port_start());

        let ip = next_ip();
        assert_eq!(ip, ip_start());

        DNS.with_borrow(|dns| {
            assert!(dns.is_empty());
        });
    }

    #[test_log::test]
    fn current_host_returns_none_when_not_set() {
        assert_eq!(current_host(), None);
    }

    #[test_log::test]
    fn with_host_sets_host_in_scope() {
        let test_addr = "test.example.com:8080".to_string();
        with_host(test_addr.clone(), |addr| {
            assert_eq!(addr, test_addr);
            assert_eq!(current_host(), Some(test_addr.clone()));
        });

        // Host should be unset outside the scope
        assert_eq!(current_host(), None);
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_hostname_creates_dns_entry() {
        reset_dns();

        let result = parse_addr("myhost:8080".to_string(), true);
        assert!(result.is_ok());

        let (sock_addr, host_name) = result.unwrap();
        assert_eq!(sock_addr.port(), 8080);
        assert_eq!(host_name, Some("myhost".to_string()));

        // Verify DNS entry was created
        DNS.with_borrow(|dns| {
            assert!(dns.contains_key("myhost"));
        });
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_fails_on_duplicate_hostname_bind() {
        reset_dns();

        // First bind should succeed
        let result1 = parse_addr("duplicate.host:8080".to_string(), true);
        assert!(result1.is_ok());

        // Second bind to same hostname should fail with AddrInUse
        let result2 = parse_addr("duplicate.host:9090".to_string(), true);
        assert!(result2.is_err());
        let err = result2.unwrap_err();
        match err {
            Error::IO(io_err) => {
                assert_eq!(io_err.kind(), io::ErrorKind::AddrInUse);
            }
            _ => panic!("Expected IO error with AddrInUse"),
        }
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_resolves_existing_hostname_for_client() {
        reset_dns();

        // First register the hostname
        let _result = parse_addr("resolved.host:8080".to_string(), true).unwrap();

        let registered_ip = DNS.with_borrow(|dns| dns.get("resolved.host").copied().unwrap());

        // Now parse as client (host=false) and verify it resolves to same IP
        let result = parse_addr("resolved.host:9090".to_string(), false);
        assert!(result.is_ok());

        let (sock_addr, _) = result.unwrap();
        assert_eq!(sock_addr.ip(), registered_ip);
        assert_eq!(sock_addr.port(), 9090);
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_fails_for_unresolved_hostname() {
        reset_dns();

        // Try to connect to a hostname that doesn't exist in DNS
        let result = parse_addr("nonexistent.host:8080".to_string(), false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            Error::IO(io_err) => {
                assert_eq!(io_err.kind(), io::ErrorKind::HostUnreachable);
            }
            _ => panic!("Expected IO error with HostUnreachable"),
        }
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_localhost_uses_host_scope() {
        reset_dns();

        let scoped_addr = "scoped.host";
        with_host(scoped_addr.to_string(), |_| {
            let result = parse_addr("127.0.0.1:8080".to_string(), true);
            assert!(result.is_ok());

            let (_sock_addr, host_name) = result.unwrap();
            assert_eq!(host_name, Some(scoped_addr.to_string()));
        });
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_ip_address_works() {
        let result = parse_addr("192.168.1.100:8080".to_string(), true);
        assert!(result.is_ok());

        let (sock_addr, _) = result.unwrap();
        assert_eq!(sock_addr.ip(), Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(sock_addr.port(), 8080);
    }

    #[test_log::test]
    #[serial]
    fn tcp_listener_drop_triggers_shutdown() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);

            {
                let _listener = TcpListener::bind(addr.to_string()).await.unwrap();
                TCP_LISTENERS.with_borrow(|x| {
                    assert!(x.read().unwrap().contains_key(&addr));
                });
            } // listener dropped here

            // Verify cleanup happened
            TCP_LISTENERS.with_borrow(|x| {
                assert!(!x.read().unwrap().contains_key(&addr));
            });
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_read_handles_partial_data() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();

                // Read in small chunks
                let mut buf = [0u8; 2];
                let count1 = stream.read(&mut buf).await.unwrap();
                assert_eq!(count1, 2);
                assert_eq!(&buf[..count1], b"he");

                let count2 = stream.read(&mut buf).await.unwrap();
                assert_eq!(count2, 2);
                assert_eq!(&buf[..count2], b"ll");

                let count3 = stream.read(&mut buf).await.unwrap();
                assert_eq!(count3, 1);
                assert_eq!(&buf[..count3], b"o");
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();
            connection.write_all(b"hello").await.unwrap();
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_write_buffering_with_internal_buffer() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();

                // Write multiple small chunks
                stream.write_all(b"a").await.unwrap();
                stream.write_all(b"b").await.unwrap();
                stream.write_all(b"c").await.unwrap();
                stream.flush().await.unwrap();
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();
            let mut buf = vec![0u8; 10];

            // Read should get all the data that was written
            let count = connection.read(&mut buf).await.unwrap();
            assert!(count >= 1);
            assert!(
                buf[..count].contains(&b'a')
                    || buf[..count].contains(&b'b')
                    || buf[..count].contains(&b'c')
            );
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_bidirectional_communication() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();

                // Read request from client
                let mut buf = [0u8; 5];
                stream.read_exact(&mut buf).await.unwrap();
                assert_eq!(&buf, b"hello");

                // Send response back to client
                stream.write_all(b"world").await.unwrap();
                stream.flush().await.unwrap();
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();

            // Send request to server
            connection.write_all(b"hello").await.unwrap();
            connection.flush().await.unwrap();

            // Read response from server
            let mut buf = [0u8; 5];
            connection.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"world");
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    fn next_port_wraps_around_at_u16_max() {
        reset_next_port();

        // Set port counter to u16::MAX - 1
        NEXT_PORT.with_borrow(|x| {
            x.store(u16::MAX - 1, Ordering::SeqCst);
        });

        // Get port at MAX - 1
        let port1 = next_port();
        assert_eq!(port1, u16::MAX - 1);

        // Get port at MAX - this will trigger wraparound immediately
        // because when fetch_add returns u16::MAX, the code resets to ephemeral_port_start
        let port2 = next_port();
        assert_eq!(port2, ephemeral_port_start());

        // Next call returns ephemeral_port_start again because:
        // 1. The store set the counter to ephemeral_port_start
        // 2. fetch_add returns the old value (ephemeral_port_start) and stores ephemeral_port_start + 1
        // So we get ephemeral_port_start again
        let port3 = next_port();
        assert_eq!(port3, ephemeral_port_start());

        // Now we get ephemeral_port_start + 1
        let port4 = next_port();
        assert_eq!(port4, ephemeral_port_start() + 1);
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_write_returns_error_when_receiver_dropped() {
        // This test verifies that writing to a stream where the other end has been
        // dropped will eventually return a BrokenPipe error.

        // Create the channels directly to test the write half behavior
        let (tx, rx) = switchy_async::sync::mpsc::unbounded::<Bytes>();

        // Drop the receiver
        drop(rx);

        // Create a write half with the orphaned sender
        let mut write_half = TcpStreamWriteHalf { tx };

        // Use the low-level poll_write to verify error behavior
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut write_half).poll_write(&mut cx, b"test data");

        match result {
            Poll::Ready(Err(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
            }
            Poll::Ready(Ok(_)) => panic!("Expected BrokenPipe error, got success"),
            Poll::Pending => panic!("Expected immediate error, got Pending"),
        }
    }

    #[test_log::test]
    #[serial]
    fn is_local_host_name_recognizes_localhost_addresses() {
        assert!(is_local_host_name("0.0.0.0"));
        assert!(is_local_host_name("127.0.0.1"));
        assert!(!is_local_host_name("192.168.1.1"));
        assert!(!is_local_host_name("localhost"));
        assert!(!is_local_host_name("example.com"));
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_zero_addr_uses_host_scope() {
        reset_dns();

        let scoped_addr = "scoped.zero.host";
        with_host(scoped_addr.to_string(), |_| {
            // 0.0.0.0 is also recognized as local host name
            let result = parse_addr("0.0.0.0:9090".to_string(), true);
            assert!(result.is_ok());

            let (_sock_addr, host_name) = result.unwrap();
            assert_eq!(host_name, Some(scoped_addr.to_string()));
        });
    }

    #[test_log::test]
    #[serial]
    fn split_stream_halves_can_be_used_independently() {
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let (mut read_half, mut write_half) = stream.into_split();

                // Use read half in one task
                let read_task = task::spawn(async move {
                    let mut buf = [0u8; 5];
                    read_half.read_exact(&mut buf).await.unwrap();
                    assert_eq!(&buf, b"ping!");
                    buf
                });

                // Use write half concurrently
                write_half.write_all(b"pong!").await.unwrap();
                write_half.flush().await.unwrap();

                read_task.await.unwrap();
            });

            let stream = TcpStream::connect(server_addr.to_string()).await.unwrap();
            let (mut read_half, mut write_half) = stream.into_split();

            // Send data while also preparing to read
            write_half.write_all(b"ping!").await.unwrap();
            write_half.flush().await.unwrap();

            // Read response
            let mut buf = [0u8; 5];
            read_half.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"pong!");
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    #[serial]
    #[should_panic(expected = "ran out of available IPs")]
    fn next_ip_panics_when_third_octet_exhausted() {
        reset_next_ip();

        // Set IP to the maximum before third octet overflow
        NEXT_IP.with_borrow_mut(|x| {
            *x = Ipv4Addr::new(192, 168, 255, 255);
        });

        // First call returns the current IP (192.168.255.255)
        let _ip1 = next_ip();

        // Second call should panic because we're out of IPs
        let _ip2 = next_ip();
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_invalid_port_returns_parse_int_error() {
        // Test that an invalid port number returns a ParseIntError
        let result = parse_addr("hostname:not_a_port".to_string(), false);
        assert!(result.is_err());

        match result.unwrap_err() {
            Error::ParseInt(_) => { /* expected */ }
            other => panic!("Expected ParseInt error, got {other:?}"),
        }
    }

    #[test_log::test]
    #[serial]
    fn parse_addr_with_port_overflow_returns_parse_int_error() {
        // Port numbers larger than u16::MAX should fail to parse
        let result = parse_addr("hostname:99999".to_string(), false);
        assert!(result.is_err());

        match result.unwrap_err() {
            Error::ParseInt(_) => { /* expected */ }
            other => panic!("Expected ParseInt error, got {other:?}"),
        }
    }

    #[test_log::test]
    #[serial]
    fn tcp_stream_read_from_non_empty_internal_buffer() {
        // Test that TcpStreamReadHalf correctly returns data from the internal buffer
        // when it has leftover data from a previous read operation
        let runtime = runtime::Runtime::new();

        runtime.block_on(async move {
            let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8080);
            let listener = TcpListener::bind(server_addr.to_string()).await.unwrap();

            task::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();

                // Send a message that will be read in multiple chunks
                stream.write_all(b"abcdefghij").await.unwrap();
                stream.flush().await.unwrap();
            });

            let mut connection = TcpStream::connect(server_addr.to_string()).await.unwrap();

            // Read only 3 bytes first - this will leave data in the internal buffer
            let mut buf1 = [0u8; 3];
            let count1 = connection.read(&mut buf1).await.unwrap();
            assert_eq!(count1, 3);
            assert_eq!(&buf1, b"abc");

            // Read another 3 bytes - this should come from the internal buffer
            let mut buf2 = [0u8; 3];
            let count2 = connection.read(&mut buf2).await.unwrap();
            assert_eq!(count2, 3);
            assert_eq!(&buf2, b"def");

            // Read the remaining 4 bytes
            let mut buf3 = [0u8; 4];
            let count3 = connection.read(&mut buf3).await.unwrap();
            assert_eq!(count3, 4);
            assert_eq!(&buf3, b"ghij");
        });

        runtime.wait().unwrap();
    }
}
