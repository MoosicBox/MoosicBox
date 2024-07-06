#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

pub use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};

pub use async_trait::async_trait;
pub use flume::{unbounded, Receiver, RecvError, SendError, Sender};
pub use futures::Future;
pub use log;
pub use thiserror::Error;
pub use tokio;
pub use tokio::task::{JoinError, JoinHandle};
pub use tokio_util::sync::CancellationToken;

#[macro_export]
macro_rules! async_service_body {
    ($command:path, $context:path, $sync:expr $(,)?) => {
        #[$crate::async_trait]
        pub trait Processor {
            type Error;

            async fn process_command(
                ctx: $crate::Arc<$crate::tokio::sync::RwLock<$context>>,
                command: $command,
            ) -> Result<(), Self::Error>;

            #[allow(unused_variables)]
            async fn on_start(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }

            #[allow(unused_variables)]
            async fn on_shutdown(
                ctx: $crate::Arc<$crate::tokio::sync::RwLock<$context>>,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        pub struct Service {
            pub ctx: $crate::Arc<$crate::tokio::sync::RwLock<$context>>,
            pub token: $crate::CancellationToken,
            sender: $crate::Sender<Command>,
            receiver: $crate::Receiver<Command>,
        }

        impl Service {
            pub fn new(ctx: $context) -> Self {
                let (tx, rx) = $crate::unbounded();
                Self {
                    ctx: $crate::Arc::new($crate::tokio::sync::RwLock::new(ctx)),
                    sender: tx,
                    receiver: rx,
                    token: $crate::CancellationToken::new(),
                }
            }

            pub fn start(mut self) -> $crate::JoinHandle<Result<(), Error>> {
                $crate::tokio::spawn(async move {
                    self.on_start().await?;
                    let ctx = self.ctx;

                    while let Ok(Ok(command)) = $crate::tokio::select!(
                        () = self.token.cancelled() => {
                            log::debug!("Service was cancelled");
                            Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                        }
                        command = self.receiver.recv_async() => { Ok(command) }
                    ) {
                        if $sync {
                            log::trace!("Received Service command");
                            if let Err(e) = Self::process_command(ctx.clone(), command.cmd).await {
                                log::error!("Failed to process command: {e:?}");
                            }
                            if let Some(tx) = command.tx {
                                tx.send_async(()).await?;
                            }
                        } else {
                            let ctx = ctx.clone();
                            $crate::tokio::spawn(async move {
                                log::trace!("Received Service command");
                                if let Err(e) = Self::process_command(ctx, command.cmd).await {
                                    log::error!("Failed to process command: {e:?}");
                                }
                                if let Some(tx) = command.tx {
                                    tx.send_async(()).await?;
                                }
                                Ok::<_, Error>(())
                            });
                        }
                    }

                    Self::on_shutdown(ctx).await?;

                    $crate::log::debug!("Stopped Service");

                    Ok(())
                })
            }

            pub fn handle(&self) -> Handle {
                Handle {
                    sender: self.sender.clone(),
                    token: self.token.clone(),
                }
            }
        }

        pub struct Command {
            cmd: $command,
            tx: Option<$crate::Sender<()>>,
        }

        #[$crate::async_trait]
        pub trait Commander {
            type Error;

            #[allow(unused)]
            fn send_command(&self, command: $command) -> Result<(), Self::Error>;
            #[allow(unused)]
            async fn send_command_async(&self, command: $command) -> Result<(), Self::Error>;
            #[allow(unused)]
            async fn send_command_and_wait_async(&self, command: $command) -> Result<(), Self::Error>;
            #[allow(unused)]
            fn shutdown(&self) -> Result<(), Self::Error>;
        }

        #[derive(Clone)]
        pub struct Handle {
            sender: $crate::Sender<Command>,
            token: $crate::CancellationToken,
        }

        impl From<$crate::SendError<Command>> for CommanderError {
            fn from(_value: $crate::SendError<Command>) -> Self {
                Self::Send
            }
        }

        #[derive(Debug, $crate::Error)]
        pub enum CommanderError {
            #[error("Failed to send")]
            Send,
            #[error(transparent)]
            Recv(#[from] $crate::RecvError),
        }

        #[$crate::async_trait]
        impl Commander for Handle {
            type Error = CommanderError;

            fn send_command(&self, command: $command) -> Result<(), Self::Error> {
                Ok(self.sender.send(Command {
                    cmd: command,
                    tx: None
                })?)
            }

            async fn send_command_async(&self, command: $command) -> Result<(), Self::Error> {
                Ok(self.sender.send_async(Command {
                    cmd: command,
                    tx: None
                }).await?)
            }

            async fn send_command_and_wait_async(&self, command: $command) -> Result<(), Self::Error> {
                let (tx, rx) = $crate::unbounded();
                let sender = self.sender.clone();
                $crate::tokio::spawn(async move {
                    sender.send_async(Command {
                        cmd: command,
                        tx: Some(tx)
                    }).await
                });
                Ok(rx.recv_async().await?)
            }

            fn shutdown(&self) -> Result<(), Self::Error> {
                log::debug!("Shutting down Service");
                self.token.cancel();
                Ok(())
            }
        }
    };
}

#[macro_export]
macro_rules! async_service_sync {
    ($command:path, $context:path $(,)?) => {
        impl From<$crate::SendError<()>> for Error {
            fn from(_value: $crate::SendError<()>) -> Self {
                Self::Send
            }
        }

        #[derive(Debug, $crate::Error)]
        pub enum Error {
            #[error(transparent)]
            Join(#[from] $crate::JoinError),
            #[error("Failed to send")]
            Send,
            #[allow(unused)]
            #[error(transparent)]
            IO(#[from] std::io::Error),
        }

        $crate::async_service_body!($command, $context, true);
    };

    ($command:path, $context:path, $error:path $(,)?) => {
        impl From<$crate::SendError<()>> for Error {
            fn from(_value: $crate::SendError<()>) -> Self {
                Self::Send
            }
        }

        #[derive(Debug, $crate::Error)]
        pub enum Error {
            #[error(transparent)]
            Join(#[from] $crate::JoinError),
            #[error("Failed to send")]
            Send,
            #[allow(unused)]
            #[error(transparent)]
            IO(#[from] std::io::Error),
            #[error(transparent)]
            Process(#[from] $error),
        }

        $crate::async_service_body!($command, $context, true);
    };
}

#[macro_export]
macro_rules! async_service {
    ($command:path, $context:path $(,)?) => {
        impl From<$crate::SendError<()>> for Error {
            fn from(_value: $crate::SendError<()>) -> Self {
                Self::Send
            }
        }

        #[derive(Debug, $crate::Error)]
        pub enum Error {
            #[error(transparent)]
            Join(#[from] $crate::JoinError),
            #[error("Failed to send")]
            Send,
            #[allow(unused)]
            #[error(transparent)]
            IO(#[from] std::io::Error),
        }

        $crate::async_service_body!($command, $context, false);
    };

    ($command:path, $context:path, $error:path $(,)?) => {
        impl From<$crate::SendError<()>> for Error {
            fn from(_value: $crate::SendError<()>) -> Self {
                Self::Send
            }
        }

        #[derive(Debug, $crate::Error)]
        pub enum Error {
            #[error(transparent)]
            Join(#[from] $crate::JoinError),
            #[error("Failed to send")]
            Send,
            #[allow(unused)]
            #[error(transparent)]
            IO(#[from] std::io::Error),
            #[error(transparent)]
            Process(#[from] $error),
        }

        $crate::async_service_body!($command, $context, false);
    };
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use async_trait::async_trait;
    use pretty_assertions::assert_eq;
    use tokio::sync::RwLock;

    pub enum ExampleCommand {
        TestCommand { value: String },
        TestCommand2,
    }

    pub struct ExampleContext {
        value: String,
    }

    mod example {
        async_service!(crate::test::ExampleCommand, crate::test::ExampleContext,);
    }

    #[async_trait]
    impl example::Processor for example::Service {
        type Error = example::Error;

        async fn process_command(
            ctx: Arc<RwLock<ExampleContext>>,
            command: ExampleCommand,
        ) -> Result<(), Self::Error> {
            match command {
                ExampleCommand::TestCommand { value } => {
                    ctx.write().await.value.clone_from(&value);
                }
                ExampleCommand::TestCommand2 => {
                    assert_eq!(ctx.read().await.value, "hey".to_string());
                }
            }
            Ok(())
        }
    }

    #[test_log::test(tokio::test)]
    async fn can_create_an_example_service() {
        use example::Commander;

        let ctx = ExampleContext {
            value: "start".into(),
        };
        let service = example::Service::new(ctx);
        let handle = service.handle();
        let join = service.start();

        handle
            .send_command_and_wait_async(ExampleCommand::TestCommand {
                value: "hey".into(),
            })
            .await
            .unwrap();

        handle
            .send_command_and_wait_async(ExampleCommand::TestCommand2)
            .await
            .unwrap();

        handle.shutdown().unwrap();
        join.await.unwrap().unwrap();
    }
}
