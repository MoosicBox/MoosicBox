use std::pin::Pin;

use moosicbox_simulator_utils::run_until_simulation_cancelled;
use scoped_tls::scoped_thread_local;
use switchy::unsync::{futures::FutureExt as _, runtime, task::JoinHandle};

use crate::Actor;

struct Handle {
    name: String,
}

scoped_thread_local! {
    static HANDLE: Handle
}

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

pub type ClientResult = Result<(), Box<dyn std::error::Error + Send>>;

pub struct Client {
    pub(crate) name: String,
    #[allow(clippy::type_complexity)]
    pub(crate) action: Option<Pin<Box<dyn Future<Output = ClientResult> + Send>>>,
    pub(crate) handle: Option<JoinHandle<Option<ClientResult>>>,
    pub(crate) runtime: runtime::Runtime,
}

impl Client {
    pub(crate) fn new(
        name: impl Into<String>,
        action: impl Future<Output = ClientResult> + Send + 'static,
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

        self.handle = Some(self.runtime.spawn(run_until_simulation_cancelled(action)));
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
