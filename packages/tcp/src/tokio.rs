use std::{marker::PhantomData, net::SocketAddr};

use async_trait::async_trait;

use crate::{
    Error, GenericTcpListener, GenericTcpStream, GenericTcpStreamReadHalf,
    GenericTcpStreamWriteHalf, impl_read_inner, impl_write_inner,
};

pub struct TcpListener(::tokio::net::TcpListener);
pub struct TcpStream(::tokio::net::TcpStream);
pub struct TcpStreamReadHalf(::tokio::net::tcp::OwnedReadHalf);
pub struct TcpStreamWriteHalf(::tokio::net::tcp::OwnedWriteHalf);

impl TcpListener {
    /// # Errors
    ///
    /// * If the `tokio::new::TcpListener` fails to bind the address
    pub async fn bind(addr: &str) -> Result<Self, crate::Error> {
        Ok(Self(::tokio::net::TcpListener::bind(addr).await?))
    }
}

impl crate::TokioTcpListener {
    /// # Errors
    ///
    /// * If the `tokio::net::TcpListener` fails to bind the address
    pub async fn bind(addr: impl Into<String>) -> Result<Self, Error> {
        Ok(Self(
            TcpListener(::tokio::net::TcpListener::bind(addr.into()).await?),
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
}

impl GenericTcpStreamReadHalf for TcpStreamReadHalf {}
impl GenericTcpStreamWriteHalf for TcpStreamWriteHalf {}

impl_read_inner!(TcpStream);
impl_read_inner!(TcpStreamReadHalf);
impl_write_inner!(TcpStream);
impl_write_inner!(TcpStreamWriteHalf);
