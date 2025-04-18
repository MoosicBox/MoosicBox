#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{marker::PhantomData, net::SocketAddr};

use ::tokio::io::{AsyncRead, AsyncWrite};
use async_trait::async_trait;
use thiserror::Error;

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "simulator")]
pub mod simulator;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] ::std::io::Error),
}

#[async_trait]
pub trait GenericTcpListener<T>: Send + Sync {
    async fn accept(&self) -> Result<(T, SocketAddr), Error>;
}

#[async_trait]
pub trait GenericTcpStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

pub struct TcpListenerWrapper<S: GenericTcpStream, T: GenericTcpListener<S>>(T, PhantomData<S>);
pub struct TcpStreamWrapper<T: GenericTcpStream>(T);

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
                pub type [< $module:camel TcpStream >] = TcpStreamWrapper<$module::TcpStream>;
                type ModuleTcpStream = [< $module:camel TcpStream >];

                pub type [< $module:camel TcpListener >] = TcpListenerWrapper<ModuleTcpStream, $module::TcpListener>;
                type ModuleTcpListener = [< $module:camel TcpListener >];
            }

            #[async_trait]
            impl GenericTcpListener<ModuleTcpStream> for ModuleTcpListener {
                async fn accept(&self) -> Result<(ModuleTcpStream, SocketAddr), Error> {
                    self.0.accept().await
                }
            }

            impl GenericTcpStream for ModuleTcpStream {}

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
            pub type TcpStream = [< $module:camel TcpStream >];
            pub type TcpListener = [< $module:camel TcpListener >];
        }
    };
}

#[cfg(feature = "simulator")]
impl_gen_types!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_gen_types!(tokio);
