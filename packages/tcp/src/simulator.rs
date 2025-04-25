use std::{marker::PhantomData, net::SocketAddr};

use async_trait::async_trait;

use crate::{
    Error, GenericTcpListener, GenericTcpStream, GenericTcpStreamReadHalf,
    GenericTcpStreamWriteHalf, impl_read_inner, impl_write_inner,
};

pub struct TcpListener(turmoil::net::TcpListener);
pub struct TcpStream(turmoil::net::TcpStream);
pub struct TcpStreamReadHalf(turmoil::net::tcp::OwnedReadHalf);
pub struct TcpStreamWriteHalf(turmoil::net::tcp::OwnedWriteHalf);

impl TcpListener {
    /// # Errors
    ///
    /// * If the `turmoil::net::TcpListener` fails to bind the address
    pub async fn bind(addr: &str) -> Result<Self, crate::Error> {
        Ok(Self(turmoil::net::TcpListener::bind(addr).await?))
    }
}

impl crate::SimulatorTcpListener {
    /// # Errors
    ///
    /// * If the `turmoil::net::TcpListener` fails to bind the address
    pub async fn bind(addr: impl Into<String>) -> Result<Self, Error> {
        Ok(Self(
            TcpListener(turmoil::net::TcpListener::bind(addr.into()).await?),
            PhantomData,
            PhantomData,
            PhantomData,
        ))
    }
}

#[async_trait]
impl GenericTcpListener<crate::SimulatorTcpStream> for TcpListener {
    async fn accept(&self) -> Result<(crate::SimulatorTcpStream, SocketAddr), crate::Error> {
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
