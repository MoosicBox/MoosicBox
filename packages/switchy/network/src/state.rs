//! The per-run network state.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, PoisonError},
    task::Waker,
    time::Duration,
};

use switchy_time::Instant;

use crate::{Datagram, Error, LinkPolicy, Stats};

/// The name of the run's RNG stream every network decision draws from.
const STREAM: &str = "switchy_network";

/// The current run's network.
#[derive(Default)]
pub struct Shared(Mutex<State>);

/// Run `f` on the current run's network.
pub fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let shared = switchy_simulation::state::<Shared>();
    let mut state = shared.0.lock().unwrap_or_else(PoisonError::into_inner);
    f(&mut state)
}

/// Why a message was dropped.
#[derive(Debug, Clone, Copy)]
pub enum Reason {
    Partition,
    Down,
}

/// A message in flight (or arrived and not yet received).
pub struct Message {
    pub from: String,
    pub payload: Vec<u8>,
}

/// An endpoint bound to a node.
#[derive(Clone, Copy)]
pub struct Binding {
    pub id: u64,
}

pub struct Node {
    pub up: bool,
    pub incarnation: u64,
    pub binding: Option<Binding>,
    /// Receivers waiting for a message to this node.
    pub recv_wakers: Vec<Waker>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            up: true,
            incarnation: 0,
            binding: None,
            recv_wakers: vec![],
        }
    }
}

#[derive(Default)]
pub struct State {
    pub default_policy: LinkPolicy,
    pub policies: BTreeMap<(String, String), LinkPolicy>,
    /// Directed links a partition blocks.
    pub blocked: BTreeSet<(String, String)>,
    pub isolated: BTreeSet<String>,
    pub nodes: BTreeMap<String, Node>,
    /// Messages by receiver, in order of arrival time (then of sending).
    pub in_flight: BTreeMap<String, BTreeMap<(Instant, u64), Message>>,
    /// The latest arrival scheduled on each link that keeps its messages in order.
    pub last_arrival: BTreeMap<(String, String), Instant>,
    pub next_seq: u64,
    pub next_binding: u64,
    pub version: u64,
    pub change_wakers: Vec<Waker>,
    pub stats: Stats,
}

impl State {
    pub fn node_mut(&mut self, node: &str) -> &mut Node {
        self.nodes.entry(node.to_owned()).or_default()
    }

    pub fn is_up(&self, node: &str) -> bool {
        self.nodes.get(node).is_none_or(|node| node.up)
    }

    pub fn is_blocked(&self, from: &str, to: &str) -> bool {
        from != to
            && (self.isolated.contains(from)
                || self.isolated.contains(to)
                || self.blocked.contains(&(from.to_owned(), to.to_owned())))
    }

    pub fn policy(&self, from: &str, to: &str) -> LinkPolicy {
        self.policies
            .get(&(from.to_owned(), to.to_owned()))
            .copied()
            .unwrap_or(self.default_policy)
    }

    /// Every waker waiting on the network, to wake after a change.
    pub fn all_wakers(&mut self) -> Vec<Waker> {
        let mut wakers = std::mem::take(&mut self.change_wakers);
        for node in self.nodes.values_mut() {
            wakers.append(&mut node.recv_wakers);
        }
        wakers
    }

    /// Drop the messages still in flight (arriving after `now`) whose link matches `matches`.
    pub fn purge(&mut self, now: Instant, matches: impl Fn(&str, &str) -> bool, reason: Reason) {
        let mut dropped = 0;
        for (to, queue) in &mut self.in_flight {
            queue.retain(|(at, _), message| {
                let keep = *at <= now || !matches(&message.from, to);
                if !keep {
                    dropped += 1;
                }
                keep
            });
        }
        self.count_dropped(reason, dropped);
    }

    /// Drop every message to `node` (arrived or not) and those from it still in flight.
    pub fn purge_down(&mut self, now: Instant, node: &str) {
        let mut dropped = self
            .in_flight
            .remove(node)
            .map_or(0, |queue| u64::try_from(queue.len()).unwrap_or(u64::MAX));
        for queue in self.in_flight.values_mut() {
            queue.retain(|(at, _), message| {
                let keep = *at <= now || message.from != node;
                if !keep {
                    dropped += 1;
                }
                keep
            });
        }
        self.last_arrival
            .retain(|(from, to), _| from != node && to != node);
        self.count_dropped(Reason::Down, dropped);
    }

    const fn count_dropped(&mut self, reason: Reason, dropped: u64) {
        match reason {
            Reason::Partition => self.stats.dropped_partition += dropped,
            Reason::Down => self.stats.dropped_down += dropped,
        }
    }

    /// Send a message, returning the wakers of its receiver.
    pub fn send(&mut self, from: &str, to: &str, payload: Vec<u8>) -> Result<Vec<Waker>, Error> {
        if !self.is_up(from) {
            return Err(Error::NodeDown(from.to_owned()));
        }
        self.stats.sent += 1;
        if !self.is_up(to) {
            log::trace!("dropping message {from} -> {to}: receiver is down");
            self.stats.dropped_down += 1;
            return Ok(vec![]);
        }
        if self.is_blocked(from, to) {
            log::trace!("dropping message {from} -> {to}: partitioned");
            self.stats.dropped_partition += 1;
            return Ok(vec![]);
        }
        let policy = self.policy(from, to);
        let stream = switchy_simulation::stream(STREAM);
        let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
        if policy.loss > 0.0 && rng.chance(policy.loss) {
            log::trace!("dropping message {from} -> {to}: lost");
            self.stats.dropped_loss += 1;
            return Ok(vec![]);
        }
        let mut copies = vec![payload];
        if policy.duplicate > 0.0 && rng.chance(policy.duplicate) {
            self.stats.duplicated += 1;
            copies.push(copies[0].clone());
        }
        let now = Instant::now();
        for payload in copies {
            let latency = draw_latency(&policy, &mut rng);
            let mut at = now.checked_add(latency).unwrap_or(now);
            if !policy.reorder {
                let last = self
                    .last_arrival
                    .entry((from.to_owned(), to.to_owned()))
                    .or_insert(at);
                at = at.max(*last);
                *last = at;
            }
            let seq = self.next_seq;
            self.next_seq += 1;
            self.in_flight.entry(to.to_owned()).or_default().insert(
                (at, seq),
                Message {
                    from: from.to_owned(),
                    payload,
                },
            );
        }
        drop(rng);
        Ok(std::mem::take(&mut self.node_mut(to).recv_wakers))
    }

    /// Check that the endpoint `binding` of `node` (bound in `incarnation`) still works.
    pub fn check_binding(&self, node: &str, incarnation: u64, binding: u64) -> Result<(), Error> {
        let Some(entry) = self.nodes.get(node) else {
            return Err(Error::Closed(node.to_owned()));
        };
        if !entry.up || entry.incarnation != incarnation {
            return Err(Error::NodeDown(node.to_owned()));
        }
        if entry.binding.is_none_or(|x| x.id != binding) {
            return Err(Error::Closed(node.to_owned()));
        }
        Ok(())
    }

    /// Take the next message that has arrived at `node`, if any.
    pub fn take(&mut self, node: &str) -> Option<Datagram> {
        self.nodes.get(node)?.binding?;
        let now = Instant::now();
        let queue = self.in_flight.get_mut(node)?;
        if let Some(entry) = queue.first_entry() {
            let (at, _) = *entry.key();
            if at > now {
                return None;
            }
            let message = entry.remove();
            self.stats.delivered += 1;
            return Some(Datagram {
                from: message.from,
                to: node.to_owned(),
                payload: message.payload,
            });
        }
        None
    }

    /// Bind a new endpoint to `node`, dropping the messages that arrived while none was.
    pub fn bind(&mut self, node: &str) -> Result<(u64, u64, Vec<Waker>), Error> {
        let id = self.next_binding;
        self.next_binding += 1;
        let now = Instant::now();
        let entry = self.node_mut(node);
        if !entry.up {
            return Err(Error::NodeDown(node.to_owned()));
        }
        entry.binding = Some(Binding { id });
        let incarnation = entry.incarnation;
        let wakers = std::mem::take(&mut entry.recv_wakers);
        if let Some(queue) = self.in_flight.get_mut(node) {
            let before = queue.len();
            queue.retain(|(at, _), _| *at > now);
            self.stats.dropped_unbound += u64::try_from(before - queue.len()).unwrap_or(u64::MAX);
        }
        Ok((id, incarnation, wakers))
    }

    /// Whether a message to `node` is still on its way.
    pub fn has_in_flight(&self, node: &str) -> bool {
        self.in_flight
            .get(node)
            .is_some_and(|queue| !queue.is_empty())
    }
}

/// Add `waker` to `wakers` unless an equivalent one is there already.
pub fn register(wakers: &mut Vec<Waker>, waker: &Waker) {
    if !wakers.iter().any(|x| x.will_wake(waker)) {
        wakers.push(waker.clone());
    }
}

/// Draw a latency from `policy` with the run's network RNG stream.
pub fn sample_latency(policy: &LinkPolicy) -> Duration {
    let stream = switchy_simulation::stream(STREAM);
    let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
    draw_latency(policy, &mut rng)
}

fn draw_latency(policy: &LinkPolicy, rng: &mut switchy_simulation::Rng) -> Duration {
    let min = policy.min_latency;
    let max = policy.max_latency.max(min);
    if max == min {
        return min;
    }
    let spread = u64::try_from(max.saturating_sub(min).as_nanos()).unwrap_or(u64::MAX);
    min + Duration::from_nanos(rng.range_inclusive(0..=spread))
}
