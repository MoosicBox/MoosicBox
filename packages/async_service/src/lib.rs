//! Asynchronous service management framework for the `MoosicBox` ecosystem.
//!
//! This crate provides macros and utilities for building async services with command processing,
//! lifecycle management, and cancellation support. It generates boilerplate code for managing
//! service state, processing commands via channels, and coordinating async tasks.
//!
//! # Examples
//!
//! ```
//! use moosicbox_async_service::*;
//!
//! #[derive(Debug)]
//! pub enum MyCommand {
//!     ProcessData { data: String },
//! }
//!
//! pub struct MyContext {
//!     pub count: u32,
//! }
//!
//! // Generate service with sequential command processing
//! async_service_sequential!(MyCommand, MyContext);
//!
//! #[async_trait]
//! impl Processor for Service {
//!     type Error = Error;
//!
//!     async fn process_command(
//!         ctx: Arc<sync::RwLock<MyContext>>,
//!         command: MyCommand,
//!     ) -> Result<(), Self::Error> {
//!         match command {
//!             MyCommand::ProcessData { data } => {
//!                 ctx.write().await.count += 1;
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use std::{pin::Pin, sync::Arc, time::Duration};

pub use async_trait::async_trait;
pub use flume::{Receiver, RecvError, SendError, Sender, unbounded};
pub use futures::Future;
pub use log;

pub use switchy_async::task::{JoinError, JoinHandle};
pub use switchy_async::util::CancellationToken;
pub use switchy_async::{runtime, select, sync};
pub use thiserror::Error;

/// Generates the core async service implementation with customizable command processing mode.
///
/// This is a low-level macro that generates the complete service infrastructure including:
/// * `Processor` trait for implementing command processing logic
/// * `Service` struct for managing service lifecycle and state
/// * `Handle` struct for sending commands to the service
/// * `Command` struct for wrapping commands with optional completion notifications
/// * `Commander` trait with methods for sending commands
/// * `CommanderError` enum for command sending errors
///
/// # Parameters
///
/// * `$command` - The command enum type that defines all possible commands
/// * `$context` - The context struct type that holds service state
/// * `$sequential` - Boolean literal (`true` or `false`) controlling command processing:
///   * `true`: Commands are processed sequentially in the order received
///   * `false`: Commands are processed concurrently, spawning a new task for each
///
/// # Generated Types
///
/// The macro generates several types in the current module:
///
/// * `Service` - Main service struct with methods:
///   * `new(ctx)` - Create a new service with the given context
///   * `with_name(name)` - Set a name for the service (for logging and task naming)
///   * `start()` - Start the service on the current runtime
///   * `start_on(handle)` - Start the service on a specific runtime handle
///   * `handle()` - Get a cloneable handle for sending commands
///
/// * `Handle` - Cloneable handle for interacting with the service (implements `Commander`)
///
/// # Examples
///
/// ```
/// use moosicbox_async_service::*;
///
/// #[derive(Debug)]
/// pub enum MyCommand {
///     DoWork { value: u32 },
/// }
///
/// pub struct MyContext {
///     total: u32,
/// }
///
/// // Define your own Error type
/// #[derive(Debug, thiserror::Error)]
/// pub enum Error {
///     #[error(transparent)]
///     Join(#[from] JoinError),
///     #[error("Failed to send")]
///     Send,
///     #[error(transparent)]
///     IO(#[from] std::io::Error),
/// }
///
/// impl From<SendError<()>> for Error {
///     fn from(_: SendError<()>) -> Self {
///         Self::Send
///     }
/// }
///
/// // Generate service with sequential processing
/// async_service_body!(MyCommand, MyContext, true);
///
/// #[async_trait]
/// impl Processor for Service {
///     type Error = Error;
///
///     async fn process_command(
///         ctx: Arc<sync::RwLock<MyContext>>,
///         command: MyCommand,
///     ) -> Result<(), Self::Error> {
///         match command {
///             MyCommand::DoWork { value } => {
///                 ctx.write().await.total += value;
///             }
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// # Notes
///
/// Most users should use the higher-level macros [`async_service!`] or
/// [`async_service_sequential!`] instead, which also generate the `Error` enum automatically.
#[macro_export]
macro_rules! async_service_body {
    ($command:path, $context:path, $sequential:expr $(,)?) => {
        #[$crate::async_trait]
        pub trait Processor {
            type Error;

            async fn process_command(
                ctx: $crate::Arc<$crate::sync::RwLock<$context>>,
                command: $command,
            ) -> Result<(), Self::Error>;

            #[allow(unused_variables)]
            async fn on_start(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }

            #[allow(unused_variables)]
            async fn on_shutdown(
                ctx: $crate::Arc<$crate::sync::RwLock<$context>>,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        pub struct Service {
            pub name: $crate::Arc<String>,
            pub ctx: $crate::Arc<$crate::sync::RwLock<$context>>,
            pub token: $crate::CancellationToken,
            sender: $crate::Sender<Command>,
            receiver: $crate::Receiver<Command>,
        }

        impl Service {
            pub fn new(ctx: $context) -> Self {
                let (tx, rx) = $crate::unbounded();
                Self {
                    ctx: $crate::Arc::new($crate::sync::RwLock::new(ctx)),
                    sender: tx,
                    receiver: rx,
                    token: $crate::CancellationToken::new(),
                    name: $crate::Arc::new("Unnamed".to_string())
                }
            }

            pub fn with_name(mut self, name: &str) -> Self {
                self.name = $crate::Arc::new(name.to_owned());
                self
            }

            pub fn start(self) -> $crate::JoinHandle<Result<(), Error>> {
                self.start_on(&$crate::runtime::Handle::current())
            }

            pub fn start_on(mut self, handle: &$crate::runtime::Handle) -> $crate::JoinHandle<Result<(), Error>> {
                let service_name = self.name.clone();
                handle.spawn_with_name(
                    &format!("async_service: {}", service_name),
                    async move {
                        self.on_start().await?;
                        let ctx = self.ctx;

                        while let Ok(Ok(command)) = $crate::select!(
                            () = self.token.cancelled() => {
                                log::debug!("Service was cancelled");
                                Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                            }
                            command = self.receiver.recv_async() => { Ok(command) }
                        ) {
                            if $sequential {
                                log::trace!("Received Service command");
                                if let Err(e) = Self::process_command(ctx.clone(), command.cmd).await {
                                    log::error!("Failed to process command: {e:?}");
                                }
                                if let Some(tx) = command.tx {
                                    tx.send_async(()).await?;
                                }
                            } else {
                                let ctx = ctx.clone();
                                $crate::runtime::Handle::current().spawn_with_name(
                                    &format!("async_service: {} - process_command", service_name),
                                    async move {
                                        log::trace!("Received Service command");
                                        if let Err(e) = Self::process_command(ctx, command.cmd).await {
                                            log::error!("Failed to process command: {e:?}");
                                        }
                                        if let Some(tx) = command.tx {
                                            tx.send_async(()).await?;
                                        }
                                        Ok::<_, Error>(())
                                    },
                                );
                            }
                        }

                        Self::on_shutdown(ctx).await?;

                        $crate::log::debug!("Stopped Service");

                        Ok(())
                    },
                )
            }

            pub fn handle(&self) -> Handle {
                Handle {
                    name: self.name.clone(),
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
            async fn send_command_and_wait_async_on(&self, command: $command, handle: &$crate::runtime::Handle) -> Result<(), Self::Error>;
            #[allow(unused)]
            fn shutdown(&self) -> Result<(), Self::Error>;
        }

        #[derive(Clone)]
        pub struct Handle {
            name: $crate::Arc<String>,
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
                self.send_command_and_wait_async_on(command, &$crate::runtime::Handle::current()).await
            }

            async fn send_command_and_wait_async_on(&self, command: $command, handle: &$crate::runtime::Handle) -> Result<(), Self::Error> {
                let (tx, rx) = $crate::unbounded();
                let sender = self.sender.clone();
                handle.spawn_with_name(
                    &format!("async_service: {} - send_command_and_wait_async", self.name),
                    async move {
                        sender.send_async(Command {
                            cmd: command,
                            tx: Some(tx)
                        }).await
                    },
                );
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

/// Generates an async service with sequential command processing.
///
/// This macro is a convenience wrapper around [`async_service_body!`] that:
/// * Automatically generates an `Error` enum with common error types
/// * Configures the service to process commands sequentially (one at a time, in order)
/// * Optionally includes a custom error type for command processing errors
///
/// Commands are processed in the order they are received, and each command completes
/// before the next one begins. This is useful when command processing must be serialized,
/// such as when modifying shared state or when order matters.
///
/// # Variants
///
/// ## Two-argument form (no custom error)
///
/// ```text
/// async_service_sequential!(CommandType, ContextType);
/// ```
///
/// Generates an `Error` enum with these variants:
/// * `Error::Join` - Task join errors
/// * `Error::Send` - Command sending errors
/// * `Error::IO` - I/O errors
///
/// ## Three-argument form (with custom error)
///
/// ```text
/// async_service_sequential!(CommandType, ContextType, ProcessErrorType);
/// ```
///
/// Adds an additional `Error::Process` variant for command processing errors.
///
/// # Examples
///
/// ```
/// use moosicbox_async_service::*;
///
/// #[derive(Debug)]
/// pub enum Command {
///     Increment,
///     GetValue,
/// }
///
/// pub struct Context {
///     value: u32,
/// }
///
/// // Generate service with sequential processing
/// async_service_sequential!(Command, Context);
///
/// #[async_trait]
/// impl Processor for Service {
///     type Error = Error;
///
///     async fn process_command(
///         ctx: Arc<sync::RwLock<Context>>,
///         command: Command,
///     ) -> Result<(), Self::Error> {
///         match command {
///             Command::Increment => {
///                 ctx.write().await.value += 1;
///             }
///             Command::GetValue => {
///                 println!("Value: {}", ctx.read().await.value);
///             }
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// # See Also
///
/// * [`async_service!`] - For concurrent command processing
/// * [`async_service_body!`] - For full control over error types and processing mode
#[macro_export]
macro_rules! async_service_sequential {
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

/// Generates an async service with concurrent command processing.
///
/// This macro is a convenience wrapper around [`async_service_body!`] that:
/// * Automatically generates an `Error` enum with common error types
/// * Configures the service to process commands concurrently (in parallel)
/// * Optionally includes a custom error type for command processing errors
///
/// Each command is processed in its own spawned task, allowing multiple commands to run
/// simultaneously. This is useful for I/O-bound operations or when commands are independent
/// and can be processed in any order.
///
/// # Variants
///
/// ## Two-argument form (no custom error)
///
/// ```text
/// async_service!(CommandType, ContextType);
/// ```
///
/// Generates an `Error` enum with these variants:
/// * `Error::Join` - Task join errors
/// * `Error::Send` - Command sending errors
/// * `Error::IO` - I/O errors
///
/// ## Three-argument form (with custom error)
///
/// ```text
/// async_service!(CommandType, ContextType, ProcessErrorType);
/// ```
///
/// Adds an additional `Error::Process` variant for command processing errors.
///
/// # Examples
///
/// ```
/// use moosicbox_async_service::*;
///
/// #[derive(Debug)]
/// pub enum Command {
///     FetchData { url: String },
///     ProcessResult { data: String },
/// }
///
/// pub struct Context {
///     results: Vec<String>,
/// }
///
/// // Generate service with concurrent processing
/// async_service!(Command, Context);
///
/// #[async_trait]
/// impl Processor for Service {
///     type Error = Error;
///
///     async fn process_command(
///         ctx: Arc<sync::RwLock<Context>>,
///         command: Command,
///     ) -> Result<(), Self::Error> {
///         match command {
///             Command::FetchData { url } => {
///                 // Simulate async I/O - multiple fetches can run concurrently
///                 println!("Fetching from {}", url);
///             }
///             Command::ProcessResult { data } => {
///                 ctx.write().await.results.push(data);
///             }
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// # See Also
///
/// * [`async_service_sequential!`] - For sequential command processing
/// * [`async_service_body!`] - For full control over error types and processing mode
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
    use switchy_async::sync::RwLock;

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

    #[test_log::test(switchy_async::test)]
    async fn can_create_an_example_service() {
        use example::Commander;

        let ctx = ExampleContext {
            value: "start".into(),
        };
        let service = example::Service::new(ctx);
        let handle = service.handle();
        let join = service.with_name("test").start();

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
