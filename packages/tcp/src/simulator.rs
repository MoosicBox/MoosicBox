use std::{marker::PhantomData, net::SocketAddr, pin::pin};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{Error, GenericTcpListener, GenericTcpStream};

pub struct TcpListener(turmoil::net::TcpListener);

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
        ))
    }
}

#[async_trait]
impl GenericTcpListener<crate::SimulatorTcpStream> for TcpListener {
    async fn accept(&self) -> Result<(crate::SimulatorTcpStream, SocketAddr), crate::Error> {
        let (stream, addr) = self.0.accept().await?;
        Ok((crate::TcpStreamWrapper(TcpStream(stream)), addr))
    }
}

pub struct TcpStream(turmoil::net::TcpStream);

#[async_trait]
impl GenericTcpStream for TcpStream {}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncRead::poll_read(inner, cx, buf)
    }
}

impl AsyncWrite for TcpStream {
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
