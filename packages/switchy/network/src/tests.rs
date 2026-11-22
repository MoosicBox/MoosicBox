use std::{
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

use switchy_time::simulator::{current_step, set_step, set_step_multiplier};

use super::*;

/// Run `f` in a fresh simulation context with `seed`, one millisecond per step.
fn run<T>(seed: u64, f: impl FnOnce() -> T) -> T {
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        set_step_multiplier(1);
        let _ = set_step(0);
        f()
    })
}

fn advance(millis: u64) {
    let _ = set_step(current_step() + millis);
}

fn drain(endpoint: &Endpoint) -> Vec<Vec<u8>> {
    std::iter::from_fn(|| endpoint.try_recv().unwrap())
        .map(|x| x.payload)
        .collect()
}

#[derive(Default)]
struct CountingWaker(AtomicUsize);

impl Wake for CountingWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test_log::test]
fn reliable_links_deliver_immediately_and_in_order() {
    run(1, || {
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        for i in 0..10_u8 {
            a.send("b", vec![i]).unwrap();
        }
        assert_eq!(drain(&b), (0..10_u8).map(|i| vec![i]).collect::<Vec<_>>());
        assert_eq!(stats().sent, 10);
        assert_eq!(stats().delivered, 10);
    });
}

#[test_log::test]
fn latency_delays_delivery() {
    run(2, || {
        set_default_policy(
            LinkPolicy::reliable()
                .with_latency(Duration::from_millis(10), Duration::from_millis(10)),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        a.send("b", b"x".to_vec()).unwrap();
        advance(9);
        assert_eq!(b.try_recv().unwrap(), None);
        advance(1);
        assert_eq!(b.try_recv().unwrap().unwrap().payload, b"x");
    });
}

/// Deliveries (in order, with drops and duplicates) of a lossy, reordering, duplicating network.
fn chaotic_trace(seed: u64) -> Vec<(String, Vec<u8>)> {
    run(seed, || {
        set_default_policy(
            LinkPolicy::reliable()
                .with_latency(Duration::from_millis(1), Duration::from_millis(50))
                .with_loss(0.2)
                .with_duplicate(0.2)
                .with_reorder(true),
        );
        let nodes = ["a", "b", "c"].map(|x| Endpoint::bind(x).unwrap());
        let mut trace = vec![];
        for round in 0..50_u8 {
            for (i, from) in nodes.iter().enumerate() {
                let to = nodes[(i + 1) % nodes.len()].node();
                from.send(to, vec![round, u8::try_from(i).unwrap()])
                    .unwrap();
            }
            advance(5);
            for node in &nodes {
                while let Some(message) = node.try_recv().unwrap() {
                    trace.push((message.to, message.payload));
                }
            }
        }
        trace
    })
}

#[test_log::test]
fn the_same_seed_replays_the_same_deliveries() {
    let first = chaotic_trace(42);
    assert_eq!(first, chaotic_trace(42));
    assert_ne!(first, chaotic_trace(43));
}

#[test_log::test]
fn loss_duplication_and_reordering_happen() {
    run(7, || {
        set_default_policy(
            LinkPolicy::reliable()
                .with_latency(Duration::ZERO, Duration::from_millis(20))
                .with_loss(0.3)
                .with_duplicate(0.3)
                .with_reorder(true),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        for i in 0..200_u8 {
            a.send("b", vec![i]).unwrap();
        }
        advance(20);
        let received = drain(&b);
        let stats = stats();
        assert!(stats.dropped_loss > 0, "{stats:?}");
        assert!(stats.duplicated > 0, "{stats:?}");
        assert!(
            received.windows(2).any(|x| x[0] > x[1]),
            "nothing was reordered"
        );
    });
}

#[test_log::test]
fn links_without_reordering_stay_in_order_whatever_the_latency() {
    run(8, || {
        set_default_policy(
            LinkPolicy::reliable().with_latency(Duration::ZERO, Duration::from_millis(30)),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        for i in 0..100_u8 {
            a.send("b", vec![i]).unwrap();
        }
        advance(30);
        assert_eq!(drain(&b), (0..100_u8).map(|i| vec![i]).collect::<Vec<_>>());
    });
}

#[test_log::test]
fn a_partition_blocks_both_directions_and_drops_messages_in_flight() {
    run(3, || {
        set_default_policy(
            LinkPolicy::reliable()
                .with_latency(Duration::from_millis(10), Duration::from_millis(10)),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        let c = Endpoint::bind("c").unwrap();
        a.send("b", b"in flight".to_vec()).unwrap();
        partition(["a"], ["b", "c"]);
        assert!(is_blocked("a", "b") && is_blocked("b", "a") && is_blocked("c", "a"));
        assert!(!is_blocked("b", "c"));
        a.send("b", b"a->b".to_vec()).unwrap();
        b.send("a", b"b->a".to_vec()).unwrap();
        b.send("c", b"b->c".to_vec()).unwrap();
        advance(10);
        assert_eq!(drain(&a), Vec::<Vec<u8>>::new());
        assert_eq!(drain(&b), Vec::<Vec<u8>>::new());
        assert_eq!(drain(&c), vec![b"b->c".to_vec()]);
        assert_eq!(stats().dropped_partition, 3);

        heal_partition(["a"], ["b", "c"]);
        a.send("b", b"healed".to_vec()).unwrap();
        advance(10);
        assert_eq!(drain(&b), vec![b"healed".to_vec()]);
    });
}

#[test_log::test]
fn a_one_way_partition_blocks_one_direction() {
    run(4, || {
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        partition_one_way(["a"], ["b"]);
        a.send("b", b"blocked".to_vec()).unwrap();
        b.send("a", b"open".to_vec()).unwrap();
        assert_eq!(drain(&b), Vec::<Vec<u8>>::new());
        assert_eq!(drain(&a), vec![b"open".to_vec()]);
        heal();
        a.send("b", b"healed".to_vec()).unwrap();
        assert_eq!(drain(&b), vec![b"healed".to_vec()]);
    });
}

#[test_log::test]
fn isolation_cuts_a_node_off_until_it_rejoins() {
    run(5, || {
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        isolate("a");
        a.send("a", b"self".to_vec()).unwrap();
        a.send("b", b"out".to_vec()).unwrap();
        b.send("a", b"in".to_vec()).unwrap();
        assert_eq!(drain(&a), vec![b"self".to_vec()]);
        assert_eq!(drain(&b), Vec::<Vec<u8>>::new());
        // Isolation also covers nodes the network had not seen yet.
        assert!(is_blocked("new", "a"));
        rejoin("a");
        b.send("a", b"back".to_vec()).unwrap();
        assert_eq!(drain(&a), vec![b"back".to_vec()]);
    });
}

#[test_log::test]
fn a_crashed_node_loses_its_messages_and_its_endpoint() {
    run(6, || {
        set_default_policy(
            LinkPolicy::reliable().with_latency(Duration::from_millis(5), Duration::from_millis(5)),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        let c = Endpoint::bind("c").unwrap();
        a.send("b", b"to b".to_vec()).unwrap();
        b.send("c", b"from b".to_vec()).unwrap();
        c.send("b", b"arrived".to_vec()).unwrap();
        advance(5);
        b.send("c", b"from b, in flight".to_vec()).unwrap();

        assert_eq!(incarnation("b"), 0);
        node_down("b");
        assert!(!is_up("b"));
        assert_eq!(incarnation("b"), 1);
        assert_eq!(b.try_recv(), Err(Error::NodeDown("b".to_owned())));
        assert_eq!(
            b.send("c", b"x".to_vec()),
            Err(Error::NodeDown("b".to_owned()))
        );
        assert_eq!(
            Endpoint::bind("b").unwrap_err(),
            Error::NodeDown("b".to_owned())
        );
        // Arrived before the crash: c keeps it. Still in flight: dropped.
        advance(5);
        assert_eq!(drain(&c), vec![b"from b".to_vec()]);
        a.send("b", b"to a dead node".to_vec()).unwrap();

        node_up("b");
        // The old endpoint belongs to the earlier incarnation.
        assert_eq!(b.try_recv(), Err(Error::NodeDown("b".to_owned())));
        let b = Endpoint::bind("b").unwrap();
        advance(5);
        assert_eq!(b.try_recv().unwrap(), None);
        a.send("b", b"after restart".to_vec()).unwrap();
        advance(5);
        assert_eq!(drain(&b), vec![b"after restart".to_vec()]);
        assert_eq!(stats().dropped_down, 4);
    });
}

#[test_log::test]
fn rebinding_closes_the_earlier_endpoint() {
    run(9, || {
        let first = Endpoint::bind("a").unwrap();
        let second = Endpoint::bind("a").unwrap();
        assert_eq!(first.try_recv(), Err(Error::Closed("a".to_owned())));
        drop(first);
        // Dropping the replaced endpoint leaves the new binding alone.
        let sender = Endpoint::bind("b").unwrap();
        sender.send("a", b"x".to_vec()).unwrap();
        assert_eq!(drain(&second), vec![b"x".to_vec()]);
    });
}

#[test_log::test]
fn messages_to_an_unbound_node_are_dropped() {
    run(10, || {
        let a = Endpoint::bind("a").unwrap();
        a.send("b", b"nobody listens".to_vec()).unwrap();
        let b = Endpoint::bind("b").unwrap();
        a.send("b", b"listening".to_vec()).unwrap();
        assert_eq!(drain(&b), vec![b"listening".to_vec()]);
        assert_eq!(stats().dropped_unbound, 1);
    });
}

#[test_log::test]
fn recv_waits_for_a_message_and_is_woken_by_its_sender() {
    run(11, || {
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        let counter = Arc::new(CountingWaker::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);
        let mut recv = pin!(b.recv());
        assert!(recv.as_mut().poll(&mut cx).is_pending());
        assert_eq!(counter.0.load(Ordering::SeqCst), 0);
        a.send("b", b"wake up".to_vec()).unwrap();
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        let Poll::Ready(Ok(message)) = recv.as_mut().poll(&mut cx) else {
            panic!("no message");
        };
        assert_eq!(message.payload, b"wake up");
    });
}

#[test_log::test]
fn recv_waits_on_the_clock_for_a_message_in_flight() {
    run(12, || {
        set_default_policy(
            LinkPolicy::reliable().with_latency(Duration::from_millis(3), Duration::from_millis(3)),
        );
        let a = Endpoint::bind("a").unwrap();
        let b = Endpoint::bind("b").unwrap();
        a.send("b", b"soon".to_vec()).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        let mut recv = pin!(b.recv());
        for _ in 0..3 {
            assert!(recv.as_mut().poll(&mut cx).is_pending());
            advance(1);
        }
        assert!(matches!(recv.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
    });
}

#[test_log::test]
fn recv_timeout_times_out_on_the_simulated_clock() {
    run(13, || {
        let b = Endpoint::bind("b").unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        let mut recv = pin!(b.recv_timeout(Duration::from_millis(10)));
        assert!(recv.as_mut().poll(&mut cx).is_pending());
        advance(9);
        assert!(recv.as_mut().poll(&mut cx).is_pending());
        advance(1);
        assert_eq!(
            recv.as_mut().poll(&mut cx),
            Poll::Ready(Err(Error::Timeout))
        );
    });
}

#[test_log::test]
fn recv_fails_when_its_node_goes_down() {
    run(14, || {
        let b = Endpoint::bind("b").unwrap();
        let counter = Arc::new(CountingWaker::default());
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);
        let mut recv = pin!(b.recv());
        assert!(recv.as_mut().poll(&mut cx).is_pending());
        node_down("b");
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            recv.as_mut().poll(&mut cx),
            Poll::Ready(Err(Error::NodeDown("b".to_owned())))
        );
    });
}

#[test_log::test]
fn changes_wake_registered_wakers_and_bump_the_version() {
    run(15, || {
        let counter = Arc::new(CountingWaker::default());
        let waker = Waker::from(Arc::clone(&counter));
        let version = version();
        wake_on_change(&waker);
        wake_on_change(&waker);
        partition(["a"], ["b"]);
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        assert!(super::version() > version);
        heal();
        assert_eq!(counter.0.load(Ordering::SeqCst), 1, "wakers fire once");
    });
}

#[test_log::test]
fn link_policies_override_the_default_per_direction() {
    run(16, || {
        let slow = LinkPolicy::reliable()
            .with_latency(Duration::from_millis(100), Duration::from_millis(100));
        set_link_policy("a", "b", slow);
        assert_eq!(link_policy("a", "b"), slow);
        assert_eq!(link_policy("b", "a"), LinkPolicy::reliable());
        assert_eq!(sample_latency("a", "b"), Duration::from_millis(100));
        clear_link_policy("a", "b");
        assert_eq!(link_policy("a", "b"), LinkPolicy::reliable());
        set_link_policy_symmetric("a", "b", slow);
        assert_eq!(link_policy("b", "a"), slow);
    });
}

#[test_log::test]
fn runs_have_independent_networks() {
    run(17, || partition(["a"], ["b"]));
    run(17, || assert!(!is_blocked("a", "b")));
    run(18, || {
        partition(["a"], ["b"]);
        reset();
        assert!(!is_blocked("a", "b"));
    });
}

#[test_log::test]
fn with_node_scopes_the_current_node() {
    assert_eq!(current_node(), None);
    with_node("outer", || {
        assert_eq!(current_node().as_deref(), Some("outer"));
        with_node("inner", || {
            assert_eq!(current_node().as_deref(), Some("inner"));
        });
        assert_eq!(current_node().as_deref(), Some("outer"));
    });
    assert_eq!(current_node(), None);
}
