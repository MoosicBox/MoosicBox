//! The `test` attribute's tokio runtime options.
#![cfg(all(
    feature = "macros",
    feature = "rt-multi-thread",
    not(feature = "simulator")
))]

use switchy_async::RuntimeFlavor;

#[switchy_async::test]
async fn the_default_runtime_runs_on_the_current_thread() {
    let flavor = switchy_async::runtime::Handle::current().runtime_flavor();
    assert_eq!(flavor, RuntimeFlavor::CurrentThread);
}

#[switchy_async::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_thread_runtime_can_block_in_place() {
    let flavor = switchy_async::runtime::Handle::current().runtime_flavor();
    assert_eq!(flavor, RuntimeFlavor::MultiThread);
    assert_eq!(switchy_async::task::block_in_place(|| 7), 7);
}

#[switchy_async::test(flavor = "current_thread")]
async fn an_explicit_current_thread_runtime() -> Result<(), String> {
    let flavor = switchy_async::runtime::Handle::current().runtime_flavor();
    (flavor == RuntimeFlavor::CurrentThread)
        .then_some(())
        .ok_or_else(|| format!("{flavor:?}"))
}
