//! Generic TCP stream and listener abstractions for async Rust.
//!
//! This crate provides generic traits and implementations for TCP networking that work
//! across different async runtimes. It supports both real tokio-based networking and an
//! in-memory simulator for testing.
//!
//! # Features
//!
//! * `tokio` - Real TCP networking using tokio
//! * `simulator` - In-memory TCP simulator for testing without actual network I/O
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "tokio")]
//! # {
//! use switchy_tcp::{TokioTcpListener, GenericTcpListener};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a TCP listener
//! let listener = TokioTcpListener::bind("127.0.0.1:8080").await?;
//!
//! // Accept incoming connections
//! let (stream, addr) = listener.accept().await?;
//! println!("Connection from: {}", addr);
//! # Ok(())
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{marker::PhantomData, net::SocketAddr};

use ::tokio::io::{AsyncRead, AsyncWrite};
use async_trait::async_trait;
use thiserror::Error;

/// Real TCP networking implementation using tokio.
///
/// This module provides TCP streams and listeners backed by the tokio runtime for actual
/// network I/O operations.
#[cfg(feature = "tokio")]
pub mod tokio;

/// In-memory TCP simulator for testing.
///
/// This module provides TCP streams and listeners that simulate network behavior in-memory
/// without actual network I/O. Useful for deterministic testing and avoiding port conflicts.
#[cfg(feature = "simulator")]
pub mod simulator;

/// Error types for TCP operations.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error from the underlying stream or listener.
    #[error(transparent)]
    IO(#[from] ::std::io::Error),
    /// Failed to parse a socket address.
    #[error(transparent)]
    AddrParse(#[from] ::std::net::AddrParseError),
    /// Failed to parse an integer (typically a port number).
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    /// Failed to send data over a channel (simulator only).
    #[cfg(feature = "simulator")]
    #[error("Send error")]
    Send,
}

/// Generic trait for TCP listeners that can accept connections.
///
/// # Errors
///
/// * `accept` may fail if the underlying listener encounters an error while accepting a connection
#[async_trait]
pub trait GenericTcpListener<T>: Send + Sync {
    /// Accepts a new incoming connection.
    ///
    /// Returns the connected stream and the remote address.
    ///
    /// # Errors
    ///
    /// * If the underlying listener fails to accept a connection
    async fn accept(&self) -> Result<(T, SocketAddr), Error>;
}

/// Generic trait for TCP streams that can be split into read and write halves.
///
/// Provides methods for splitting a stream into separate read and write halves, and for
/// querying the local and remote addresses of the connection.
pub trait GenericTcpStream<R: GenericTcpStreamReadHalf, W: GenericTcpStreamWriteHalf>:
    AsyncRead + AsyncWrite + Send + Sync + Unpin
{
    /// Splits the stream into separate read and write halves.
    fn into_split(self) -> (R, W);

    /// Returns the local address of this stream.
    ///
    /// # Errors
    ///
    /// * If the underlying `TcpStream` fails to get the `local_addr`
    fn local_addr(&self) -> std::io::Result<SocketAddr>;

    /// Returns the remote address of this stream.
    ///
    /// # Errors
    ///
    /// * If the underlying `TcpStream` fails to get the `peer_addr`
    fn peer_addr(&self) -> std::io::Result<SocketAddr>;
}

/// Generic trait for the read half of a TCP stream.
///
/// This trait marks types that represent the readable half of a split TCP stream.
pub trait GenericTcpStreamReadHalf: AsyncRead + Send + Sync + Unpin {}

/// Generic trait for the write half of a TCP stream.
///
/// This trait marks types that represent the writable half of a split TCP stream.
pub trait GenericTcpStreamWriteHalf: AsyncWrite + Send + Sync + Unpin {}

/// Wrapper type for generic TCP listeners.
///
/// This type wraps implementations of `GenericTcpListener` and provides a unified interface
/// for accepting TCP connections. It is typically instantiated via type aliases like
/// `TokioTcpListener` or `SimulatorTcpListener`.
pub struct TcpListenerWrapper<
    R: GenericTcpStreamReadHalf,
    W: GenericTcpStreamWriteHalf,
    S: GenericTcpStream<R, W>,
    T: GenericTcpListener<S>,
>(T, PhantomData<R>, PhantomData<W>, PhantomData<S>);

/// Wrapper type for generic TCP streams.
///
/// This type wraps implementations of `GenericTcpStream` and provides a unified interface
/// for reading and writing over TCP connections. It is typically instantiated via type aliases
/// like `TokioTcpStream` or `SimulatorTcpStream`.
pub struct TcpStreamWrapper<
    R: GenericTcpStreamReadHalf,
    W: GenericTcpStreamWriteHalf,
    T: GenericTcpStream<R, W>,
>(T, PhantomData<R>, PhantomData<W>);

#[allow(unused)]
macro_rules! impl_http {
    ($module:ident, $local_module:ident $(,)?) => {
        paste::paste! {
            pub use [< impl_ $module >]::*;
        }

        mod $local_module {
            use std::pin::pin;

            use crate::*;

            paste::paste! {
                #[doc = concat!("Read half of a ", stringify!($module), " TCP stream.\n\nWraps the underlying read half to provide a generic interface.")]
                pub type [< $module:camel TcpStreamReadHalf >] = $module::TcpStreamReadHalf;
                type ModuleTcpStreamReadHalf = [< $module:camel TcpStreamReadHalf >];

                #[doc = concat!("Write half of a ", stringify!($module), " TCP stream.\n\nWraps the underlying write half to provide a generic interface.")]
                pub type [< $module:camel TcpStreamWriteHalf >] = $module::TcpStreamWriteHalf;
                type ModuleTcpStreamWriteHalf = [< $module:camel TcpStreamWriteHalf >];

                #[doc = concat!("TCP stream for ", stringify!($module), ".\n\nWraps the underlying stream to provide a generic interface that can be split into read and write halves.")]
                pub type [< $module:camel TcpStream >] = TcpStreamWrapper<ModuleTcpStreamReadHalf, ModuleTcpStreamWriteHalf, $module::TcpStream>;
                type ModuleTcpStream = [< $module:camel TcpStream >];

                #[doc = concat!("TCP listener for ", stringify!($module), ".\n\nWraps the underlying listener to provide a generic interface for accepting connections.")]
                pub type [< $module:camel TcpListener >] = TcpListenerWrapper<ModuleTcpStreamReadHalf, ModuleTcpStreamWriteHalf, ModuleTcpStream, $module::TcpListener>;
                type ModuleTcpListener = [< $module:camel TcpListener >];
            }

            #[async_trait]
            impl GenericTcpListener<ModuleTcpStream> for ModuleTcpListener {
                async fn accept(&self) -> Result<(ModuleTcpStream, SocketAddr), Error> {
                    self.0.accept().await
                }
            }

            impl GenericTcpStream<ModuleTcpStreamReadHalf, ModuleTcpStreamWriteHalf> for ModuleTcpStream {
                fn into_split(self) -> (ModuleTcpStreamReadHalf, ModuleTcpStreamWriteHalf) {
                    self.0.into_split()
                }

                fn local_addr(&self) -> std::io::Result<SocketAddr> {
                    self.0.local_addr()
                }

                fn peer_addr(&self) -> std::io::Result<SocketAddr> {
                    self.0.peer_addr()
                }
            }

            impl ModuleTcpStream {
                /// Connects to a remote TCP server at the specified address.
                ///
                /// # Errors
                ///
                /// * If the underlying `TcpStream` fails to connect
                pub async fn connect(addr: &str) -> std::io::Result<Self> {
                    Ok(Self($module::TcpStream::connect(addr).await?, PhantomData, PhantomData))
                }

                /// Returns the local socket address of this stream.
                ///
                /// # Errors
                ///
                /// * If the underlying `TcpStream` fails to get the `local_addr`
                pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
                    self.0.local_addr()
                }

                /// Returns the remote peer socket address of this stream.
                ///
                /// # Errors
                ///
                /// * If the underlying `TcpStream` fails to get the `peer_addr`
                pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
                    self.0.peer_addr()
                }
            }

            impl AsyncRead for ModuleTcpStream {
                fn poll_read(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                    buf: &mut ::tokio::io::ReadBuf<'_>,
                ) -> std::task::Poll<std::io::Result<()>> {
                    let this = self.get_mut();
                    let inner = &mut this.0;
                    let inner = pin!(inner);
                    AsyncRead::poll_read(inner, cx, buf)
                }
            }

            impl AsyncWrite for ModuleTcpStream {
                fn poll_write(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                    buf: &[u8],
                ) -> std::task::Poll<Result<usize, std::io::Error>> {
                    let this = self.get_mut();
                    let inner = &mut this.0;
                    let inner = pin!(inner);
                    AsyncWrite::poll_write(inner, cx, buf)
                }

                fn poll_flush(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<Result<(), std::io::Error>> {
                    let this = self.get_mut();
                    let inner = &mut this.0;
                    let inner = pin!(inner);
                    AsyncWrite::poll_flush(inner, cx)
                }

                fn poll_shutdown(
                    self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<Result<(), std::io::Error>> {
                    let this = self.get_mut();
                    let inner = &mut this.0;
                    let inner = pin!(inner);
                    AsyncWrite::poll_shutdown(inner, cx)
                }
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_http!(simulator, impl_simulator);

#[cfg(feature = "tokio")]
impl_http!(tokio, impl_tokio);

#[allow(unused)]
macro_rules! impl_gen_types {
    ($module:ident $(,)?) => {
        paste::paste! {
            /// Default TCP listener type for the current feature configuration.
            ///
            /// This type alias points to the appropriate listener implementation based on
            /// enabled features. With the `simulator` feature, it uses the in-memory simulator.
            /// Otherwise, it uses the tokio-based implementation.
            pub type TcpListener = [< $module:camel TcpListener >];

            /// Default TCP stream type for the current feature configuration.
            ///
            /// This type alias points to the appropriate stream implementation based on
            /// enabled features. With the `simulator` feature, it uses the in-memory simulator.
            /// Otherwise, it uses the tokio-based implementation.
            pub type TcpStream = [< $module:camel TcpStream >];

            /// Default TCP stream read half type for the current feature configuration.
            ///
            /// This type alias points to the appropriate read half implementation based on
            /// enabled features. With the `simulator` feature, it uses the in-memory simulator.
            /// Otherwise, it uses the tokio-based implementation.
            pub type TcpStreamReadHalf = [< $module:camel TcpStreamReadHalf >];

            /// Default TCP stream write half type for the current feature configuration.
            ///
            /// This type alias points to the appropriate write half implementation based on
            /// enabled features. With the `simulator` feature, it uses the in-memory simulator.
            /// Otherwise, it uses the tokio-based implementation.
            pub type TcpStreamWriteHalf = [< $module:camel TcpStreamWriteHalf >];
        }
    };
}

#[cfg(feature = "simulator")]
impl_gen_types!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_gen_types!(tokio);

#[allow(unused)]
macro_rules! impl_read_inner {
    ($type:ty $(,)?) => {
        impl tokio::io::AsyncRead for $type {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut ::tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                let this = self.get_mut();
                let inner = &mut this.0;
                let inner = std::pin::pin!(inner);
                tokio::io::AsyncRead::poll_read(inner, cx, buf)
            }
        }
    };
}

#[allow(unused)]
macro_rules! impl_write_inner {
    ($type:ty $(,)?) => {
        impl tokio::io::AsyncWrite for $type {
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                let this = self.get_mut();
                let inner = &mut this.0;
                let inner = std::pin::pin!(inner);
                tokio::io::AsyncWrite::poll_write(inner, cx, buf)
            }

            fn poll_flush(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                let this = self.get_mut();
                let inner = &mut this.0;
                let inner = std::pin::pin!(inner);
                tokio::io::AsyncWrite::poll_flush(inner, cx)
            }

            fn poll_shutdown(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                let this = self.get_mut();
                let inner = &mut this.0;
                let inner = std::pin::pin!(inner);
                tokio::io::AsyncWrite::poll_shutdown(inner, cx)
            }
        }
    };
}

#[allow(unused)]
pub(crate) use impl_read_inner;
#[allow(unused)]
pub(crate) use impl_write_inner;
