//! Datagram endpoints: a node's socket on the simulated network.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use switchy_time::Instant;

use crate::{Datagram, Error, state, state::with_state};

/// A node's socket on the simulated network: it sends messages as the node and receives the
/// messages sent to it.
///
/// A node has at most one endpoint: binding another closes the earlier one. An endpoint stops
/// working ([`Error::NodeDown`]) once its node goes down, even after the node comes back up, so
/// a restarted node binds a new one. Dropping an endpoint unbinds it; messages arriving while
/// no endpoint is bound are dropped.
#[derive(Debug)]
pub struct Endpoint {
    node: String,
    incarnation: u64,
    binding: u64,
}

impl Endpoint {
    /// Bind an endpoint to `node`, closing any endpoint bound to it before.
    ///
    /// # Errors
    ///
    /// * [`Error::NodeDown`] if `node` is down
    pub fn bind(node: impl Into<String>) -> Result<Self, Error> {
        let node = node.into();
        let (binding, incarnation, wakers) = with_state(|state| state.bind(&node))?;
        // Receivers of a replaced endpoint learn it is closed.
        for waker in wakers {
            waker.wake();
        }
        Ok(Self {
            node,
            incarnation,
            binding,
        })
    }

    /// Bind an endpoint to the [current node](crate::current_node).
    ///
    /// # Errors
    ///
    /// * [`Error::Closed`] if no node is current
    /// * [`Error::NodeDown`] if the current node is down
    pub fn bind_current() -> Result<Self, Error> {
        let node = crate::current_node().ok_or_else(|| Error::Closed(String::new()))?;
        Self::bind(node)
    }

    /// The node this endpoint belongs to.
    #[must_use]
    pub fn node(&self) -> &str {
        &self.node
    }

    /// Send `payload` to `to`; see [`crate::send`].
    ///
    /// # Errors
    ///
    /// * [`Error::NodeDown`] if this endpoint's node is down or crashed since it was bound
    /// * [`Error::Closed`] if another endpoint was bound to the node since
    pub fn send(&self, to: &str, payload: impl Into<Vec<u8>>) -> Result<(), Error> {
        let payload = payload.into();
        let wakers = with_state(|state| {
            state.check_binding(&self.node, self.incarnation, self.binding)?;
            state.send(&self.node, to, payload)
        })?;
        for waker in wakers {
            waker.wake();
        }
        Ok(())
    }

    /// The next message that has arrived, if any, without waiting.
    ///
    /// # Errors
    ///
    /// * [`Error::NodeDown`] if this endpoint's node is down or crashed since it was bound
    /// * [`Error::Closed`] if another endpoint was bound to the node since
    pub fn try_recv(&self) -> Result<Option<Datagram>, Error> {
        with_state(|state| {
            state.check_binding(&self.node, self.incarnation, self.binding)?;
            Ok(state.take(&self.node))
        })
    }

    /// Wait for the next message, on the simulated clock.
    ///
    /// The future resolves to an error if the node goes down or the endpoint is replaced
    /// meanwhile (see [`Self::try_recv`]).
    pub const fn recv(&self) -> Recv<'_> {
        Recv {
            endpoint: self,
            deadline: None,
            timeout: None,
        }
    }

    /// Wait for the next message for at most `timeout` of simulated time.
    ///
    /// The future resolves to [`Error::Timeout`] if none arrives in time, and to the errors of
    /// [`Self::recv`].
    pub const fn recv_timeout(&self, timeout: Duration) -> Recv<'_> {
        Recv {
            endpoint: self,
            deadline: None,
            timeout: Some(timeout),
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        with_state(|state| {
            if let Some(node) = state.nodes.get_mut(&self.node)
                && node.binding.is_some_and(|x| x.id == self.binding)
            {
                node.binding = None;
            }
        });
    }
}

/// The future of [`Endpoint::recv`] and [`Endpoint::recv_timeout`].
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Recv<'a> {
    endpoint: &'a Endpoint,
    /// When it times out, fixed on first poll.
    deadline: Option<Instant>,
    timeout: Option<Duration>,
}

impl Future for Recv<'_> {
    type Output = Result<Datagram, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let now = Instant::now();
        if let Some(timeout) = this.timeout.take() {
            this.deadline = Some(now.checked_add(timeout).unwrap_or(now));
        }
        let endpoint = this.endpoint;
        let deadline = this.deadline;
        let (poll, spin) = with_state(|state| {
            if let Err(e) =
                state.check_binding(&endpoint.node, endpoint.incarnation, endpoint.binding)
            {
                return (Poll::Ready(Err(e)), false);
            }
            if let Some(message) = state.take(&endpoint.node) {
                return (Poll::Ready(Ok(message)), false);
            }
            if deadline.is_some_and(|deadline| now >= deadline) {
                return (Poll::Ready(Err(Error::Timeout)), false);
            }
            if deadline.is_none() && !state.has_in_flight(&endpoint.node) {
                // Nothing on its way: a sender, or a change of the network, wakes us.
                state::register(&mut state.node_mut(&endpoint.node).recv_wakers, cx.waker());
                return (Poll::Pending, false);
            }
            (Poll::Pending, true)
        });
        if spin {
            // Waiting for a time (a message's arrival or the deadline): like the simulator's
            // sleep, poll again as the clock moves.
            cx.waker().wake_by_ref();
        }
        poll
    }
}
