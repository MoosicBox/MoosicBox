//! A small replicated log, model checked.
//!
//! Node 0 leads; nodes 1 and 2 follow. The leader appends entries to its log and sends them
//! to followers over a network that may reorder and drop messages (a multiset of in-flight
//! messages). Followers append an entry when it extends their log and acknowledge; the leader
//! commits an entry once a majority holds it. The leader may crash and restart, keeping only
//! its persisted log: what it knew about followers and its commit index are lost.
//!
//! The followers are interchangeable, so the model canonicalizes states by ordering them,
//! which roughly halves the state space without losing any behaviour.
//!
//! ```text
//! cargo run -p simvar_explore --example replicated_log            # correct: passes
//! cargo run -p simvar_explore --example replicated_log -- --buggy # commits too early
//! ```
//!
//! The buggy variant counts only the leader's own copy toward a majority, so breadth-first
//! search finds the shortest way to commit an entry only the leader holds (five steps: two
//! proposals, and the first one acknowledged by one follower).

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::borrow::Cow;

use simvar_explore::{Checker, Model, Properties};

/// Nodes in the cluster; node 0 is the leader.
const NODES: usize = 3;
/// The most entries the leader appends.
const MAX_ENTRIES: usize = 2;
/// The most messages in flight at once.
const MAX_IN_FLIGHT: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Message {
    /// Leader to follower `to`: the entry at `index` is `value`.
    Append { to: usize, index: usize, value: u8 },
    /// Follower `from` to leader: my log has `len` entries.
    Ack { from: usize, len: usize },
}

impl Message {
    /// The message with followers 1 and 2 swapped.
    const fn swap_followers(&self) -> Self {
        match *self {
            Self::Append { to, index, value } => Self::Append {
                to: swap(to),
                index,
                value,
            },
            Self::Ack { from, len } => Self::Ack {
                from: swap(from),
                len,
            },
        }
    }
}

const fn swap(node: usize) -> usize {
    match node {
        1 => 2,
        2 => 1,
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Cluster {
    /// Each node's persisted log.
    logs: [Vec<u8>; NODES],
    /// Leader's volatile view: how many entries each node is known to hold.
    matched: [usize; NODES],
    /// Leader's volatile commit index: entries `..commit` are committed.
    commit: usize,
    /// In-flight messages, sorted (a multiset).
    network: Vec<Message>,
}

#[derive(Clone, Debug)]
enum Action {
    /// The leader appends an entry.
    Propose,
    /// The leader sends follower `to` the next entry it lacks.
    Send { to: usize },
    /// The `index`th in-flight message arrives.
    Deliver { index: usize },
    /// The `index`th in-flight message is lost.
    Drop { index: usize },
    /// The leader crashes and restarts with only its log.
    RestartLeader,
}

struct ReplicatedLog {
    /// Commit on the leader's copy alone.
    buggy: bool,
}

impl ReplicatedLog {
    fn majority_len(&self, matched: &[usize; NODES]) -> usize {
        if self.buggy {
            return matched[0];
        }
        let mut sorted = *matched;
        sorted.sort_unstable();
        // The largest length at least a majority of nodes hold.
        sorted[NODES / 2]
    }
}

impl Model for ReplicatedLog {
    type State = Cluster;
    type Action = Action;

    fn init_states(&self) -> Vec<Cluster> {
        vec![Cluster {
            logs: Default::default(),
            matched: [0; NODES],
            commit: 0,
            network: Vec::new(),
        }]
    }

    fn actions(&self, state: &Cluster, actions: &mut Vec<Action>) {
        if state.logs[0].len() < MAX_ENTRIES {
            actions.push(Action::Propose);
        }
        if state.network.len() < MAX_IN_FLIGHT {
            for to in 1..NODES {
                if state.matched[to] < state.logs[0].len() {
                    actions.push(Action::Send { to });
                }
            }
        }
        for index in 0..state.network.len() {
            // Identical messages are interchangeable: act on the first of each.
            if index > 0 && state.network[index] == state.network[index - 1] {
                continue;
            }
            actions.push(Action::Deliver { index });
            actions.push(Action::Drop { index });
        }
        actions.push(Action::RestartLeader);
    }

    fn next_state(&self, state: &Cluster, action: &Action) -> Option<Cluster> {
        let mut next = state.clone();
        match *action {
            Action::Propose => {
                let value = u8::try_from(next.logs[0].len()).ok()?;
                next.logs[0].push(value);
                next.matched[0] = next.logs[0].len();
            }
            Action::Send { to } => {
                let index = next.matched[to];
                let value = *next.logs[0].get(index)?;
                next.network.push(Message::Append { to, index, value });
                next.network.sort();
            }
            Action::Drop { index } => {
                next.network.remove(index);
            }
            Action::Deliver { index } => match next.network.remove(index) {
                Message::Append { to, index, value } => {
                    let log = &mut next.logs[to];
                    if log.len() == index {
                        log.push(value);
                    } else if log.len() < index {
                        return Some(next); // a gap: ignore it
                    }
                    let len = log.len();
                    next.network.push(Message::Ack { from: to, len });
                    next.network.sort();
                }
                Message::Ack { from, len } => {
                    next.matched[from] = next.matched[from].max(len);
                    next.commit = next.commit.max(self.majority_len(&next.matched));
                }
            },
            Action::RestartLeader => {
                if next.matched == [next.logs[0].len(), 0, 0] && next.commit == 0 {
                    return None; // nothing volatile to lose
                }
                next.matched = [next.logs[0].len(), 0, 0];
                next.commit = 0;
            }
        }
        Some(next)
    }

    fn canonicalize<'s>(&self, state: &'s Cluster) -> Cow<'s, Cluster> {
        // The smaller of the state and its mirror image (followers swapped).
        let mut swapped = state.clone();
        swapped.logs.swap(1, 2);
        swapped.matched.swap(1, 2);
        swapped.network = state.network.iter().map(Message::swap_followers).collect();
        swapped.network.sort();
        let key = |s: &Cluster| (s.logs.clone(), s.matched, s.network.clone());
        if key(state) <= key(&swapped) {
            Cow::Borrowed(state)
        } else {
            Cow::Owned(swapped)
        }
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties
            .always("committed entries are held by a majority", |_, s| {
                let leader = &s.logs[0][..s.commit];
                let holders = s.logs.iter().filter(|log| log.starts_with(leader)).count();
                holders > NODES / 2
            })
            .always("followers' logs are prefixes of the leader's", |_, s| {
                s.logs[1..].iter().all(|log| s.logs[0].starts_with(log))
            })
            .sometimes("every entry commits", |_, s| s.commit == MAX_ENTRIES);
    }
}

fn main() {
    let buggy = std::env::args().any(|arg| arg == "--buggy");
    let model = ReplicatedLog { buggy };

    let report = Checker::from_env().bfs(&model);
    println!("{report}");

    if !buggy {
        let walks = Checker::from_env()
            .walks(200)
            .max_depth(30)
            .random_walk(&model);
        println!("{walks}");
    }

    if !report.is_success() {
        std::process::exit(1);
    }
}
