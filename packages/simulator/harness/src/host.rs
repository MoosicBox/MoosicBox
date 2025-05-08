use std::{pin::Pin, sync::Arc};

use moosicbox_simulator_utils::run_until_simulation_cancelled;
use scoped_tls::scoped_thread_local;
use switchy::{
    tcp::simulator::with_host as with_tcp_host,
    unsync::{runtime, task::JoinHandle},
};

use crate::Actor;

struct Handle {
    name: String,
}

scoped_thread_local! {
    static HANDLE: Handle
}

#[allow(unused)]
#[must_use]
pub fn current_host() -> Option<String> {
    if HANDLE.is_set() {
        Some(HANDLE.with(|x| x.name.to_string()))
    } else {
        None
    }
}

fn with_host<T>(name: String, f: impl FnOnce(&str) -> T) -> T {
    let host = Handle { name };
    HANDLE.set(&host, || f(&host.name))
}

pub type HostResult = Result<(), Box<dyn std::error::Error + Send + 'static>>;

pub struct Host {
    pub(crate) name: String,
    #[allow(clippy::type_complexity)]
    pub(crate) action: Box<dyn Fn() -> Pin<Box<dyn Future<Output = HostResult> + Send + 'static>>>,
    pub(crate) handle: Option<JoinHandle<Option<HostResult>>>,
    pub(crate) runtime: runtime::Runtime,
}

impl Host {
    pub(crate) fn new<
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HostResult> + Send + 'static,
    >(
        name: impl Into<String>,
        action: F,
    ) -> Self {
        let runtime = runtime::Runtime::new();
        let action = Arc::new(action);
        let name = name.into();
        Self {
            name: name.clone(),
            action: Box::new(move || {
                let action = action.clone();
                let name = name.clone();
                Box::pin(async move {
                    with_tcp_host(name.clone(), |name| {
                        log::debug!("starting tcp host on name={name}");
                        with_host(name.to_string(), |name| {
                            log::debug!("starting host on name={name}");
                            action()
                        })
                    })
                    .await
                })
            }),
            handle: None,
            runtime,
        }
    }

    pub(crate) fn start(&mut self) {
        assert!(!self.has_started(), "Host {} already started", self.name);

        self.handle = Some(
            self.runtime
                .spawn(run_until_simulation_cancelled((self.action)())),
        );
    }

    pub(crate) const fn has_started(&self) -> bool {
        self.handle.is_some()
    }

    pub(crate) fn is_running(&mut self) -> bool {
        self.handle.as_mut().is_some_and(|x| !x.is_finished())
    }
}

impl Actor for Host {
    fn tick(&self) {
        with_tcp_host(self.name.clone(), |_| {
            with_host(self.name.clone(), |_| self.runtime.tick());
        });
    }
}

impl Actor for &Host {
    fn tick(&self) {
        (*self).tick();
    }
}
