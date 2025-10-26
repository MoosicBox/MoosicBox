use std::pin::Pin;

use scoped_tls::scoped_thread_local;
use simvar_utils::run_until_simulation_cancelled;
use switchy::unsync::{futures::FutureExt as _, runtime, task::JoinHandle};

use crate::Actor;

struct Handle {
    name: String,
}

scoped_thread_local! {
    static HANDLE: Handle
}

/// Returns the name of the currently executing client, if any.
///
/// This function is only meaningful when called from within a client's action
/// future. Returns `None` if called from outside a client context.
#[allow(unused)]
#[must_use]
pub fn current_client() -> Option<String> {
    if HANDLE.is_set() {
        Some(HANDLE.with(|x| x.name.to_string()))
    } else {
        None
    }
}

fn with_client<T>(name: String, f: impl FnOnce(&str) -> T) -> T {
    let client = Handle { name };
    HANDLE.set(&client, || f(&client.name))
}

/// Result type for client actions.
///
/// Clients return `Ok(())` on success or an error on failure.
pub type ClientResult = Result<(), Box<dyn std::error::Error + Send>>;

/// A client actor in the simulation.
///
/// Clients represent ephemeral actors that perform specific tasks and then exit.
/// Unlike hosts, clients cannot be restarted or "bounced". They are created through
/// the [`Sim::client`] method and run until their action completes or the simulation
/// is cancelled.
///
/// This type is opaque and cannot be constructed directly by users.
///
/// [`Sim::client`]: crate::Sim::client
pub struct Client {
    pub(crate) name: String,
    #[allow(clippy::type_complexity)]
    pub(crate) action: Option<Pin<Box<dyn Future<Output = ClientResult>>>>,
    pub(crate) handle: Option<JoinHandle<Option<ClientResult>>>,
    pub(crate) runtime: runtime::Runtime,
}

impl Client {
    pub(crate) fn new(
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + 'static,
    ) -> Self {
        let runtime = runtime::Runtime::new();
        let name = name.into();
        Self {
            name,
            action: Some(Box::pin(
                run_until_simulation_cancelled(action).map(|x| x.unwrap_or(Ok(()))),
            )),
            handle: None,
            runtime,
        }
    }

    pub(crate) fn start(&mut self) {
        assert!(!self.has_started(), "Client {} already started", self.name);

        let Some(action) = self.action.take() else {
            panic!("Client already started");
        };

        self.handle = Some(
            self.runtime
                .spawn_local(run_until_simulation_cancelled(action)),
        );
    }

    const fn has_started(&self) -> bool {
        self.handle.is_some()
    }

    pub(crate) fn is_running(&mut self) -> bool {
        self.handle.as_mut().is_some_and(|x| !x.is_finished())
    }
}

impl Actor for Client {
    fn tick(&self) {
        with_client(self.name.clone(), |_| self.runtime.tick());
    }
}

impl Actor for &Client {
    fn tick(&self) {
        (*self).tick();
    }
}
