//! Runtime ownership tests use two independent executors on different OS threads.

use super::*;
use std::{future::Future, task::Poll};

fn http() -> HostRequest {
    HostRequest::Http(Box::new(RequestEffect {
        source: hyperchad_renderer_bmux::actions::Source {
            key: "runtime-test".into(),
            path: Vec::new(),
            generation: 0,
        },
        url: "/runtime".into(),
        base_url: "/".into(),
        method: "POST".into(),
        body: RequestBody::Empty,
        headers: BTreeMap::new(),
        target: hyperchad_renderer::transformer::models::Selector::SelfTarget,
        strategy: hyperchad_renderer::transformer::models::SwapStrategy::This,
        request_id: None,
        navigation: false,
    }))
}

#[test]
fn local_handler_runs_once_on_owner_runtime_not_terminal_executor() {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (stop_tx, stop_rx) = flume::bounded::<()>(1);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = calls.clone();
    let owner = std::thread::spawn(move || {
        let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
        runtime.block_on(async {
            let owner_thread = std::thread::current().id();
            let router =
                Router::new().with_route_result("/runtime", move |_request: RouteRequest| {
                    let observed = observed.clone();
                    async move {
                        assert_eq!(std::thread::current().id(), owner_thread);
                        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        // Exercise owner-runtime timer registration as well as thread identity.
                        switchy::unsync::time::sleep(Duration::from_millis(1)).await;
                        Ok::<_, std::io::Error>(Content::View(Box::default()))
                    }
                });
            let config = attach_on(BmuxConfig::default(), router, runtime.handle());
            ready_tx.send(config.host.request.unwrap()).unwrap();
            stop_rx.recv_async().await.unwrap();
        });
    });
    let callback = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let terminal = switchy::unsync::runtime::Builder::new().build().unwrap();
    terminal.block_on(async {
        assert!(matches!(
            callback(http()).await.unwrap(),
            HostResponse::Route { status: 200, .. }
        ));
    });
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    stop_tx.send(()).unwrap();
    owner.join().unwrap();
}

#[test]
fn dropping_terminal_request_aborts_owner_handler_instead_of_detaching_it() {
    struct Dropped(std::sync::mpsc::Sender<()>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (started_tx, started_rx) = flume::bounded::<()>(1);
    let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
    let (stop_tx, stop_rx) = flume::bounded::<()>(1);
    let owner = std::thread::spawn(move || {
        let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
        runtime.block_on(async {
            let router =
                Router::new().with_route_result("/runtime", move |_request: RouteRequest| {
                    let started = started_tx.clone();
                    let dropped = dropped_tx.clone();
                    async move {
                        let _guard = Dropped(dropped);
                        started.send_async(()).await.unwrap();
                        std::future::pending::<()>().await;
                        Ok::<_, std::io::Error>(Content::View(Box::default()))
                    }
                });
            let config = attach_on(BmuxConfig::default(), router, runtime.handle());
            ready_tx.send(config.host.request.unwrap()).unwrap();
            stop_rx.recv_async().await.unwrap();
        });
    });
    let callback = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let terminal = switchy::unsync::runtime::Builder::new().build().unwrap();
    terminal.block_on(async {
        let mut request = callback(http());
        std::future::poll_fn(|cx| {
            assert!(matches!(request.as_mut().poll(cx), Poll::Pending));
            Poll::Ready(())
        })
        .await;
        started_rx.recv_async().await.unwrap();
        drop(request);
    });
    dropped_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("owner handler remained detached after cancellation");
    stop_tx.send(()).unwrap();
    owner.join().unwrap();
}
