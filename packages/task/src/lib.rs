#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(tokio_unstable)]
pub fn spawn<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    spawn_on(name, &tokio::runtime::Handle::current(), future)
}

#[cfg(not(tokio_unstable))]
pub fn spawn<Fut>(name: &str, future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    spawn_on(name, &tokio::runtime::Handle::current(), future)
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
    spawn_blocking_on(name, &tokio::runtime::Handle::current(), function)
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
    spawn_blocking_on(name, &tokio::runtime::Handle::current(), function)
}

/// # Panics
///
/// * If fails to `spawn_local` the `tokio` task
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
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

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
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = future.await;
            log::trace!("spawn_local finished: {name}");

            response
        }
    };
    tokio::task::spawn_local(future)
}

/// # Panics
///
/// * If fails to `spawn_on` the `tokio` task
#[cfg(tokio_unstable)]
pub fn spawn_on<Fut>(
    name: &str,
    handle: &tokio::runtime::Handle,
    future: Fut,
) -> tokio::task::JoinHandle<Fut::Output>
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
        .spawn_on(future, handle)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn_on<Fut>(
    name: &str,
    handle: &tokio::runtime::Handle,
    future: Fut,
) -> tokio::task::JoinHandle<Fut::Output>
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
    handle.spawn(future)
}

/// # Panics
///
/// * If fails to `spawn_blocking_on` the `tokio` task
#[cfg(tokio_unstable)]
pub fn spawn_blocking_on<Function, Output>(
    name: &str,
    handle: &tokio::runtime::Handle,
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
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = function();
            log::trace!("spawn_blocking finished: {name}");

            response
        }
    };
    tokio::task::Builder::new()
        .name(name)
        .spawn_blocking_on(function, handle)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn_blocking_on<Function, Output>(
    name: &str,
    handle: &tokio::runtime::Handle,
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
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = function();
            log::trace!("spawn_blocking finished: {name}");

            response
        }
    };
    handle.spawn_blocking(function)
}

/// # Panics
///
/// * If fails to `spawn_local_on` the `tokio` task
#[cfg(tokio_unstable)]
pub fn spawn_local_on<Fut>(
    name: &str,
    local_set: &tokio::task::LocalSet,
    future: Fut,
) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("spawn_local start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = future.await;
            log::trace!("spawn_local finished: {name}");

            response
        }
    };
    tokio::task::Builder::new()
        .name(name)
        .spawn_local_on(future, local_set)
        .unwrap()
}

#[cfg(not(tokio_unstable))]
pub fn spawn_local_on<Fut>(
    name: &str,
    local_set: &tokio::task::LocalSet,
    future: Fut,
) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("spawn_local start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = future.await;
            log::trace!("spawn_local finished: {name}");

            response
        }
    };
    local_set.spawn_local(future)
}

#[cfg(tokio_unstable)]
pub fn block_on<Fut>(name: &str, future: Fut) -> Fut::Output
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    block_on_runtime(name, &tokio::runtime::Handle::current(), future)
}

#[cfg(not(tokio_unstable))]
pub fn block_on<Fut>(name: &str, future: Fut) -> Fut::Output
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    block_on_runtime(name, &tokio::runtime::Handle::current(), future)
}

#[cfg(tokio_unstable)]
pub fn block_on_runtime<Fut>(
    name: &str,
    handle: &tokio::runtime::Handle,
    future: Fut,
) -> Fut::Output
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("block_on start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = future.await;
            log::trace!("block_on finished: {name}");

            response
        }
    };
    handle.block_on(future)
}

#[cfg(not(tokio_unstable))]
pub fn block_on_runtime<Fut>(
    name: &str,
    handle: &tokio::runtime::Handle,
    future: Fut,
) -> Fut::Output
where
    Fut: futures::Future + 'static,
    Fut::Output: 'static,
{
    log::trace!("block_on start: {name}");
    #[cfg(debug_assertions)]
    let future = {
        let name = name.to_owned();
        async move {
            #[cfg(feature = "profiling")]
            profiling::function_scope!(&name);

            let response = future.await;
            log::trace!("block_on finished: {name}");

            response
        }
    };
    handle.block_on(future)
}
