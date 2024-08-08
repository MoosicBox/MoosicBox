#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(tokio_unstable)]
pub fn spawn<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    log::trace!("spawn start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            let response = future.await;
            log::trace!("spawn finished: {name}");

            response
        }
    };
    tokio::task::Builder::new()
        .name(name)
        .spawn(future)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    log::trace!("spawn start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            let response = future.await;
            log::trace!("spawn finished: {name}");

            response
        }
    };
    tokio::task::spawn(future)
}

#[cfg(tokio_unstable)]
pub fn spawn_blocking<Function, Output>(
    name: &str,
    function: Function,
) -> tokio::task::JoinHandle<Output>
where
    Function: FnOnce() -> Output + Send + 'static,
    Output: Send + 'static,
{
    log::trace!("spawn_blocking start: {name}");
    #[cfg(debug_assertions)]
    let function = {
        let name = name.to_owned();
        move || {
            let response = function();
            log::trace!("spawn_blocking finished: {name}");

            response
        }
    };
    tokio::task::Builder::new()
        .name(name)
        .spawn_blocking(function)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn_blocking<Function, Output>(
    name: &str,
    function: Function,
) -> tokio::task::JoinHandle<Output>
where
    Function: FnOnce() -> Output + Send + 'static,
    Output: Send + 'static,
{
    log::trace!("spawn_blocking start: {name}");
    #[cfg(debug_assertions)]
    let function = {
        let name = name.to_owned();
        move || {
            let response = function();
            log::trace!("spawn_blocking finished: {name}");

            response
        }
    };
    tokio::task::spawn_blocking(function)
}

#[cfg(tokio_unstable)]
pub fn spawn_local<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("spawn_local start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            let response = future.await;
            log::trace!("spawn_local finished: {name}");

            response
        }
    };
    tokio::task::Builder::new()
        .name(name)
        .spawn_local(future)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn_local<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("spawn_local start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            let response = future.await;
            log::trace!("spawn_local finished: {name}");

            response
        }
    };
    tokio::task::spawn_local(future)
}
