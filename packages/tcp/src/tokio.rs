use std::{marker::PhantomData, net::SocketAddr, pin::pin};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{Error, GenericTcpListener, GenericTcpStream};

pub struct TcpListener(::tokio::net::TcpListener);
pub struct TcpStream(::tokio::net::TcpStream);

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
        ))
    }
}

#[async_trait]
impl GenericTcpListener<crate::TokioTcpStream> for TcpListener {
    async fn accept(&self) -> Result<(crate::TokioTcpStream, SocketAddr), crate::Error> {
        let (stream, addr) = self.0.accept().await?;
        Ok((crate::TcpStreamWrapper(TcpStream(stream)), addr))
    }
}

#[async_trait]
impl GenericTcpStream for TcpStream {}

impl AsyncRead for TcpStream {
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
