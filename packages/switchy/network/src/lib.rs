//! A deterministic simulated network for switchy simulations.
//!
//! Every simulation run (a [`switchy_simulation`] context) has one network. Its nodes are named
//! by strings; in a `simvar` simulation they are the names of its hosts and clients. The network
//! carries messages between nodes subject to faults:
//!
//! * **Link policies** ([`LinkPolicy`]): per directed link latency (uniform between a minimum
//!   and a maximum), loss probability, duplication probability, and whether messages may be
//!   reordered. Links without a policy of their own use the [default policy](set_default_policy).
//! * **Partitions**: [symmetric](partition) and [one-way](partition_one_way) partitions between
//!   groups of nodes, [isolation](isolate) of a node from every other, and [healing](heal).
//! * **Node crashes**: a [down](node_down) node loses every message in flight to or from it,
//!   its endpoint closes, and it cannot send until it is [up](node_up) again. Each crash starts a
//!   new [incarnation](incarnation), so transports can tell connections of an earlier life apart.
//!
//! Every random decision (latency, loss, duplication) is drawn from the run's
//! `"switchy_network"` RNG stream, so the same seed replays the same deliveries and drops.
//! Time is the run's simulated clock ([`switchy_time::Instant`]).
//!
//! There are two ways to use it:
//!
//! * The message API: [`send`] and [`Endpoint`], a datagram socket per node whose
//!   [`recv`](Endpoint::recv) waits (asynchronously, on the simulated clock) until a message
//!   arrives or a timeout passes.
//! * The fault model for other transports: [`is_blocked`], [`is_up`], [`incarnation`],
//!   [`sample_latency`] and [`wake_on_change`]. `switchy_tcp`'s simulator uses these, so TCP
//!   applications see the same partitions, crashes and latency.
//!
//! # Example
//!
//! ```rust
//! use std::time::Duration;
//!
//! use switchy_network::{Endpoint, LinkPolicy};
//!
//! switchy_network::reset();
//! switchy_network::set_default_policy(LinkPolicy::reliable());
//!
//! let a = Endpoint::bind("a").unwrap();
//! let b = Endpoint::bind("b").unwrap();
//!
//! a.send("b", b"hello".to_vec()).unwrap();
//! let message = b.try_recv().unwrap().unwrap();
//! assert_eq!(message.from, "a");
//! assert_eq!(message.payload, b"hello");
//!
//! // Partitioned links drop everything sent across them.
//! switchy_network::partition(["a"], ["b"]);
//! a.send("b", b"lost".to_vec()).unwrap();
//! assert_eq!(b.try_recv().unwrap(), None);
//!
//! switchy_network::heal();
//! a.send("b", b"again".to_vec()).unwrap();
//! assert_eq!(b.try_recv().unwrap().unwrap().payload, b"again");
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod endpoint;
mod state;

use std::{
    cell::RefCell,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

pub use endpoint::{Endpoint, Recv};
pub use switchy_time::Instant;

use state::with_state;

/// How a directed link between two nodes treats the messages sent over it.
///
/// The default, [`LinkPolicy::reliable`], delivers every message immediately and in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkPolicy {
    /// The least time a message spends in flight.
    pub min_latency: Duration,
    /// The most time a message spends in flight. Each message's latency is drawn uniformly
    /// between [`Self::min_latency`] and this.
    pub max_latency: Duration,
    /// The probability (`0.0` to `1.0`) that a message is lost.
    pub loss: f64,
    /// The probability (`0.0` to `1.0`) that a message is delivered twice.
    pub duplicate: f64,
    /// Whether messages may overtake each other. Without it, a link delivers its messages in
    /// the order they were sent, whatever their latencies.
    pub reorder: bool,
}

impl LinkPolicy {
    /// A link that delivers every message immediately, once, in order.
    #[must_use]
    pub const fn reliable() -> Self {
        Self {
            min_latency: Duration::ZERO,
            max_latency: Duration::ZERO,
            loss: 0.0,
            duplicate: 0.0,
            reorder: false,
        }
    }

    /// This policy with latencies drawn uniformly from `min..=max`. A `max` below `min` is
    /// raised to `min`.
    #[must_use]
    pub const fn with_latency(mut self, min: Duration, max: Duration) -> Self {
        self.min_latency = min;
        self.max_latency = if max.as_nanos() < min.as_nanos() {
            min
        } else {
            max
        };
        self
    }

    /// This policy losing each message with probability `loss`.
    #[must_use]
    pub const fn with_loss(mut self, loss: f64) -> Self {
        self.loss = loss;
        self
    }

    /// This policy duplicating each message with probability `duplicate`.
    #[must_use]
    pub const fn with_duplicate(mut self, duplicate: f64) -> Self {
        self.duplicate = duplicate;
        self
    }

    /// This policy allowing (or not) messages to overtake each other.
    #[must_use]
    pub const fn with_reorder(mut self, reorder: bool) -> Self {
        self.reorder = reorder;
        self
    }
}

impl Default for LinkPolicy {
    fn default() -> Self {
        Self::reliable()
    }
}

/// A message delivered to a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    /// The node that sent it.
    pub from: String,
    /// The node it was delivered to.
    pub to: String,
    /// What was sent.
    pub payload: Vec<u8>,
}

/// Errors of the message API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The node is down, or crashed since the endpoint was bound.
    #[error("node '{0}' is down")]
    NodeDown(String),
    /// The endpoint was replaced by another endpoint bound to the same node.
    #[error("endpoint of node '{0}' is closed")]
    Closed(String),
    /// No message arrived before the timeout.
    #[error("timed out waiting for a message")]
    Timeout,
}

/// Counts of what happened to the messages sent in the current run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Messages handed to the network by nodes that were up.
    pub sent: u64,
    /// Messages received by an endpoint.
    pub delivered: u64,
    /// Extra copies created by duplication.
    pub duplicated: u64,
    /// Messages lost to a link's loss probability.
    pub dropped_loss: u64,
    /// Messages dropped by a partition or isolation (at send time or while in flight).
    pub dropped_partition: u64,
    /// Messages dropped because their sender or receiver was down.
    pub dropped_down: u64,
    /// Messages that arrived while no endpoint was bound to their receiver.
    pub dropped_unbound: u64,
}

/// Forget the current run's network: every node, policy, partition and message in flight.
/// Endpoints bound before stop working, and anything waiting on the network is woken.
pub fn reset() {
    let wakers = with_state(|state| {
        let wakers = state.all_wakers();
        *state = state::State::default();
        wakers
    });
    wake_all(wakers);
}

/// The policy of links without one of their own.
#[must_use]
pub fn default_policy() -> LinkPolicy {
    with_state(|state| state.default_policy)
}

/// Set the policy of links without one of their own. It applies to messages sent from now on.
pub fn set_default_policy(policy: LinkPolicy) {
    with_state(|state| state.default_policy = policy);
}

/// The policy of the link from `from` to `to`.
#[must_use]
pub fn link_policy(from: &str, to: &str) -> LinkPolicy {
    with_state(|state| state.policy(from, to))
}

/// Set the policy of the link from `from` to `to` (one direction only). It applies to messages
/// sent from now on.
pub fn set_link_policy(from: &str, to: &str, policy: LinkPolicy) {
    with_state(|state| {
        state
            .policies
            .insert((from.to_owned(), to.to_owned()), policy);
    });
}

/// Set the policy of the links between `a` and `b`, in both directions.
pub fn set_link_policy_symmetric(a: &str, b: &str, policy: LinkPolicy) {
    set_link_policy(a, b, policy);
    set_link_policy(b, a, policy);
}

/// Make the link from `from` to `to` use the default policy again.
pub fn clear_link_policy(from: &str, to: &str) {
    with_state(|state| {
        state.policies.remove(&(from.to_owned(), to.to_owned()));
    });
}

/// Partition the nodes of `a` from those of `b`, in both directions: messages between them in
/// flight are dropped, and so is everything sent across until the partition heals.
pub fn partition<A: AsRef<str>, B: AsRef<str>>(
    a: impl IntoIterator<Item = A>,
    b: impl IntoIterator<Item = B>,
) {
    let a = names(a);
    let b = names(b);
    let mut links = vec![];
    for x in &a {
        for y in &b {
            links.push((x.clone(), y.clone()));
            links.push((y.clone(), x.clone()));
        }
    }
    block(links);
}

/// Block messages from the nodes of `from` to those of `to`, in that direction only. Messages
/// the other way still flow.
pub fn partition_one_way<A: AsRef<str>, B: AsRef<str>>(
    from: impl IntoIterator<Item = A>,
    to: impl IntoIterator<Item = B>,
) {
    let from = names(from);
    let to = names(to);
    let mut links = vec![];
    for x in &from {
        for y in &to {
            links.push((x.clone(), y.clone()));
        }
    }
    block(links);
}

/// Heal the partition between the nodes of `a` and those of `b`, in both directions.
/// Isolation of a node is healed by [`rejoin`] (or [`heal`]).
pub fn heal_partition<A: AsRef<str>, B: AsRef<str>>(
    a: impl IntoIterator<Item = A>,
    b: impl IntoIterator<Item = B>,
) {
    let a = names(a);
    let b = names(b);
    changed(|state| {
        for x in &a {
            for y in &b {
                state.blocked.remove(&(x.clone(), y.clone()));
                state.blocked.remove(&(y.clone(), x.clone()));
            }
        }
    });
}

/// Cut `node` off from every other node, in both directions, until it [rejoins](rejoin).
pub fn isolate(node: &str) {
    changed(|state| {
        state.isolated.insert(node.to_owned());
        let now = Instant::now();
        state.purge(
            now,
            |from, to| from != to && (from == node || to == node),
            state::Reason::Partition,
        );
    });
}

/// End the isolation of `node`. Partitions it is part of stay.
pub fn rejoin(node: &str) {
    changed(|state| {
        state.isolated.remove(node);
    });
}

/// Heal every partition and isolation.
pub fn heal() {
    changed(|state| {
        state.blocked.clear();
        state.isolated.clear();
    });
}

/// Whether a partition or isolation currently blocks messages from `from` to `to`. A node is
/// never cut off from itself.
#[must_use]
pub fn is_blocked(from: &str, to: &str) -> bool {
    with_state(|state| state.is_blocked(from, to))
}

/// Take `node` down, as when its process crashes.
///
/// Messages in flight to or from it are dropped, its endpoint closes, it cannot send, messages
/// sent to it are dropped, and it starts a new [incarnation]. Does nothing if it is already
/// down.
pub fn node_down(node: &str) {
    changed(|state| {
        let entry = state.node_mut(node);
        if !entry.up {
            return;
        }
        entry.up = false;
        entry.incarnation += 1;
        entry.binding = None;
        let now = Instant::now();
        state.purge_down(now, node);
    });
}

/// Bring `node` up again (in the incarnation its last crash started).
pub fn node_up(node: &str) {
    changed(|state| state.node_mut(node).up = true);
}

/// Whether `node` is up. Nodes are up until taken [down](node_down).
#[must_use]
pub fn is_up(node: &str) -> bool {
    with_state(|state| state.nodes.get(node).is_none_or(|node| node.up))
}

/// How many times `node` has gone down. Transports record it when a connection is made, and
/// treat a connection whose peer's incarnation has changed as reset.
#[must_use]
pub fn incarnation(node: &str) -> u64 {
    with_state(|state| state.nodes.get(node).map_or(0, |node| node.incarnation))
}

/// Whether a message from `from` could reach `to` now: both are up and no partition blocks
/// the link.
#[must_use]
pub fn can_reach(from: &str, to: &str) -> bool {
    with_state(|state| state.is_up(from) && state.is_up(to) && !state.is_blocked(from, to))
}

/// Draw a latency for one message over the link from `from` to `to` from its policy (ignoring
/// loss and duplication), for transports that carry their own data, such as a TCP stream.
#[must_use]
pub fn sample_latency(from: &str, to: &str) -> Duration {
    with_state(|state| {
        let policy = state.policy(from, to);
        state::sample_latency(&policy)
    })
}

/// Send `payload` from `from` to `to`, subject to the link's policy, partitions and the nodes'
/// state. Like a datagram, a message that is dropped is not an error.
///
/// # Errors
///
/// * [`Error::NodeDown`] if `from` is down
pub fn send(from: &str, to: &str, payload: impl Into<Vec<u8>>) -> Result<(), Error> {
    let payload = payload.into();
    let wakers = with_state(|state| state.send(from, to, payload))?;
    wake_all(wakers);
    Ok(())
}

/// The current run's message counts.
#[must_use]
pub fn stats() -> Stats {
    with_state(|state| state.stats)
}

/// A counter that changes whenever the network's faults do (a partition, a heal, an isolation,
/// a node going down or up, a reset).
#[must_use]
pub fn version() -> u64 {
    with_state(|state| state.version)
}

/// Wake `waker` the next time the network's faults change (see [`version`]). Transports waiting
/// for a partition to heal, or watching a peer for a crash, register here.
pub fn wake_on_change(waker: &Waker) {
    with_state(|state| state::register(&mut state.change_wakers, waker));
}

thread_local! {
    static CURRENT_NODE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The node the current code runs as, if any (see [`with_node`]).
#[must_use]
pub fn current_node() -> Option<String> {
    CURRENT_NODE.with_borrow(Clone::clone)
}

/// Run `f` as `node`: [`current_node`] returns it until `f` returns (or panics). Simulation
/// harnesses wrap each actor's work in this, so transports know which node they act for.
pub fn with_node<T>(node: impl Into<String>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            CURRENT_NODE.with_borrow_mut(|current| *current = previous);
        }
    }
    let previous = CURRENT_NODE.with_borrow_mut(|current| current.replace(node.into()));
    let _restore = Restore(previous);
    f()
}

/// Run every poll of `future` as `node` (see [`with_node`]).
pub fn in_node<F: Future>(node: impl Into<String>, future: F) -> InNode<F> {
    InNode {
        node: node.into(),
        future: Box::pin(future),
    }
}

/// The future of [`in_node`].
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct InNode<F> {
    node: String,
    future: Pin<Box<F>>,
}

impl<F: Future> Future for InNode<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.get_mut();
        let future = this.future.as_mut();
        with_node(this.node.clone(), || future.poll(cx))
    }
}

fn names<S: AsRef<str>>(nodes: impl IntoIterator<Item = S>) -> Vec<String> {
    nodes.into_iter().map(|x| x.as_ref().to_owned()).collect()
}

fn block(links: Vec<(String, String)>) {
    changed(|state| {
        let now = Instant::now();
        for (from, to) in links {
            if from == to {
                continue;
            }
            state.purge(now, |f, t| f == from && t == to, state::Reason::Partition);
            state.blocked.insert((from, to));
        }
    });
}

/// Apply a change to the faults, then wake everything waiting on them.
fn changed(f: impl FnOnce(&mut state::State)) {
    let wakers = with_state(|state| {
        f(state);
        state.version += 1;
        state.all_wakers()
    });
    wake_all(wakers);
}

fn wake_all(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}

#[cfg(test)]
mod tests;
