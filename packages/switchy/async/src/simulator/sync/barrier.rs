//! Barrier synchronization primitive for simulator runtime.
//!
//! This provides a barrier that allows multiple tasks to synchronize at the same point.
//! The implementation uses oneshot channels to coordinate between waiting tasks.

use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// A barrier enables multiple tasks to synchronize the beginning of some computation.
///
/// # Examples
///
/// ```rust
/// use switchy_async::sync::Barrier;
/// use std::sync::Arc;
///
/// # #[switchy_async::test]
/// # async fn example() {
/// let mut handles = Vec::with_capacity(10);
/// let barrier = Arc::new(Barrier::new(10));
/// for _ in 0..10 {
///     let c = barrier.clone();
///     // The same messages will be printed together.
///     // You will NOT see any interleaving.
///     handles.push(switchy_async::task::spawn(async move {
///         println!("before wait");
///         let wait_result = c.wait().await;
///         println!("after wait");
///         wait_result
///     }));
/// }
///
/// // Will not resolve until all "after wait" messages have been printed
/// let mut num_leaders = 0;
/// for handle in handles {
///     let wait_result = handle.await.unwrap();
///     if wait_result.is_leader() {
///         num_leaders += 1;
///     }
/// }
///
/// // Exactly one barrier will resolve as the "leader"
/// assert_eq!(num_leaders, 1);
/// # }
/// ```
#[derive(Debug)]
pub struct Barrier {
    inner: Arc<Mutex<BarrierInner>>,
}

#[derive(Debug)]
struct BarrierInner {
    n: usize,
    count: usize,
    generation: usize,
    waiters: Vec<oneshot::Sender<bool>>, // true = leader
}

/// A result returned from [`Barrier::wait`] when all tasks in the barrier have rendezvoused.
#[derive(Clone, Debug)]
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// Returns `true` if this task from wait is the "leader task".
    ///
    /// Only one task will have `true` returned from their result, all other tasks will have `false` returned.
    #[must_use]
    pub const fn is_leader(&self) -> bool {
        self.is_leader
    }
}

impl Barrier {
    /// Creates a new barrier that can block a given number of tasks.
    ///
    /// A barrier will block `n-1` tasks which call [`Barrier::wait`] and then wake up all tasks
    /// at once when the `n`th task calls `wait`.
    ///
    /// # Panics
    ///
    /// * If `n` is 0
    #[must_use]
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "barrier size must be positive");

        Self {
            inner: Arc::new(Mutex::new(BarrierInner {
                n,
                count: 0,
                generation: 0,
                waiters: Vec::with_capacity(n.saturating_sub(1)),
            })),
        }
    }

    /// Does not resolve until all tasks have rendezvoused here.
    ///
    /// Barriers are re-usable after all tasks have rendezvoused once, and can be used continuously.
    ///
    /// A single (arbitrary) future will receive a [`BarrierWaitResult`] that returns `true` from
    /// [`BarrierWaitResult::is_leader`] when returning from this function, and all other tasks will receive
    /// a result that will return `false` from `is_leader`.
    ///
    /// # Cancel safety
    ///
    /// This method is not cancel safe.
    ///
    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    pub async fn wait(&self) -> BarrierWaitResult {
        let receiver = {
            let mut inner = self.inner.lock().unwrap();

            // Check if we need to reset for a new generation
            if inner.count == 0 {
                // First waiter of new generation - clear any leftover waiters
                inner.waiters.clear();
            }

            inner.count += 1;

            if inner.count == inner.n {
                // We're the last task - release everyone
                inner.count = 0;
                inner.generation = inner.generation.wrapping_add(1);

                // Send false (not leader) to all waiting tasks
                for tx in inner.waiters.drain(..) {
                    let _ = tx.send(false);
                }

                // Return immediately as the leader
                return BarrierWaitResult { is_leader: true };
            }

            // Not the last task - create channel and wait
            let (tx, rx) = oneshot::channel();
            inner.waiters.push(tx);
            rx
        };

        // Wait for the signal
        // If the sender is dropped (shouldn't happen), treat as not leader
        let is_leader = receiver.await.unwrap_or(false);
        BarrierWaitResult { is_leader }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::runtime::Builder;

    #[test]
    fn test_barrier_basic() {
        let runtime = crate::simulator::runtime::build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let barrier = Arc::new(Barrier::new(2));
            let b1 = barrier.clone();
            let b2 = barrier.clone();

            let (r1, r2) =
                futures::future::join(
                    async move { b1.wait().await },
                    async move { b2.wait().await },
                )
                .await;

            // Exactly one leader
            assert_ne!(r1.is_leader(), r2.is_leader());
            assert!(r1.is_leader() || r2.is_leader());
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn test_barrier_multiple_tasks() {
        let runtime = crate::simulator::runtime::build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let barrier = Arc::new(Barrier::new(5));
            let mut handles = vec![];

            for i in 0..5 {
                let b = barrier.clone();
                handles.push(crate::task::spawn(async move {
                    let result = b.wait().await;
                    (i, result.is_leader())
                }));
            }

            let results: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(Result::unwrap)
                .collect();

            // Exactly one leader
            let leader_count = results.iter().filter(|(_, is_leader)| *is_leader).count();
            assert_eq!(leader_count, 1);
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn test_barrier_single_task() {
        let runtime = crate::simulator::runtime::build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let barrier = Barrier::new(1);
            let result = barrier.wait().await;
            assert!(result.is_leader());
        });

        runtime.wait().unwrap();
    }

    #[test]
    #[should_panic(expected = "barrier size must be positive")]
    fn test_barrier_zero_size() {
        let _ = Barrier::new(0);
    }

    #[test]
    fn test_barrier_wait_result_clone() {
        // Test that BarrierWaitResult can be cloned
        let result = BarrierWaitResult { is_leader: true };
        let cloned = result.clone();
        assert_eq!(result.is_leader(), cloned.is_leader());

        let result2 = BarrierWaitResult { is_leader: false };
        let cloned2 = result2.clone();
        assert_eq!(result2.is_leader(), cloned2.is_leader());
    }

    #[test]
    fn test_barrier_generation_advances_on_each_cycle() {
        let runtime = crate::simulator::runtime::build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let barrier = Arc::new(Barrier::new(2));

            // Run multiple cycles and verify barrier continues to work
            for cycle in 0..5 {
                let b1 = barrier.clone();
                let b2 = barrier.clone();

                let (r1, r2) = futures::future::join(async move { b1.wait().await }, async move {
                    b2.wait().await
                })
                .await;

                // Each cycle should have exactly one leader
                assert_ne!(
                    r1.is_leader(),
                    r2.is_leader(),
                    "Cycle {cycle}: expected exactly one leader"
                );
            }
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn test_barrier_inner_state_initialization() {
        // Test that barrier inner state is initialized correctly
        let barrier = Barrier::new(5);
        let inner = barrier.inner.lock().unwrap();

        assert_eq!(inner.n, 5);
        assert_eq!(inner.count, 0);
        assert_eq!(inner.generation, 0);
        assert!(inner.waiters.is_empty());
        drop(inner);
    }
}
