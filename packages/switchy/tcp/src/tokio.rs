//! Real TCP networking implementation using tokio.
//!
//! This module provides wrapper types around tokio's TCP primitives that implement
//! the generic TCP traits defined in the parent module.

use std::{marker::PhantomData, net::SocketAddr};

use async_trait::async_trait;

use crate::{
    Error, GenericTcpListener, GenericTcpStream, GenericTcpStreamReadHalf,
    GenericTcpStreamWriteHalf, impl_read_inner, impl_write_inner,
};

/// TCP listener backed by tokio's networking.
///
/// Wraps `tokio::net::TcpListener` to provide the generic TCP listener interface.
pub struct TcpListener(tokio::net::TcpListener);

/// TCP stream backed by tokio's networking.
///
/// Wraps `tokio::net::TcpStream` to provide the generic TCP stream interface.
pub struct TcpStream(tokio::net::TcpStream);

/// Read half of a tokio TCP stream.
///
/// Wraps `tokio::net::tcp::OwnedReadHalf` to provide the generic read half interface.
pub struct TcpStreamReadHalf(tokio::net::tcp::OwnedReadHalf);

/// Write half of a tokio TCP stream.
///
/// Wraps `tokio::net::tcp::OwnedWriteHalf` to provide the generic write half interface.
pub struct TcpStreamWriteHalf(tokio::net::tcp::OwnedWriteHalf);

impl TcpListener {
    /// Binds a TCP listener to the specified address.
    ///
    /// # Errors
    ///
    /// * If the `tokio::net::TcpListener` fails to bind the address
    pub async fn bind(addr: &str) -> Result<Self, crate::Error> {
        Ok(Self(tokio::net::TcpListener::bind(addr).await?))
    }
}

impl crate::TokioTcpListener {
    /// Binds a wrapped TCP listener to the specified address.
    ///
    /// # Errors
    ///
    /// * If the `tokio::net::TcpListener` fails to bind the address
    pub async fn bind(addr: impl Into<String>) -> Result<Self, Error> {
        Ok(Self(
            TcpListener(tokio::net::TcpListener::bind(addr.into()).await?),
            PhantomData,
            PhantomData,
            PhantomData,
        ))
    }
}

#[async_trait]
impl GenericTcpListener<crate::TokioTcpStream> for TcpListener {
    async fn accept(&self) -> Result<(crate::TokioTcpStream, SocketAddr), crate::Error> {
        let (stream, addr) = self.0.accept().await?;
        Ok((
            crate::TcpStreamWrapper(TcpStream(stream), PhantomData, PhantomData),
            addr,
        ))
    }
}

impl GenericTcpStream<TcpStreamReadHalf, TcpStreamWriteHalf> for TcpStream {
    fn into_split(self) -> (TcpStreamReadHalf, TcpStreamWriteHalf) {
        let (r, w) = self.0.into_split();

        (TcpStreamReadHalf(r), TcpStreamWriteHalf(w))
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.0.local_addr()
    }

    fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        self.0.peer_addr()
    }
}

impl TcpStream {
    /// Connects to a remote TCP server at the specified address.
    ///
    /// # Errors
    ///
    /// * If the underlying `tokio::net::TcpStream` fails to connect
    pub async fn connect(addr: &str) -> std::io::Result<Self> {
        Ok(Self(tokio::net::TcpStream::connect(addr).await?))
    }
}

impl GenericTcpStreamReadHalf for TcpStreamReadHalf {}
impl GenericTcpStreamWriteHalf for TcpStreamWriteHalf {}

impl_read_inner!(TcpStream);
impl_read_inner!(TcpStreamReadHalf);
impl_write_inner!(TcpStream);
impl_write_inner!(TcpStreamWriteHalf);
