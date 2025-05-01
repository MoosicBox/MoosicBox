use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use moosicbox_random::{RNG, rand::rand::seq::IteratorRandom};

use crate::{
    Error,
    runtime::{Builder, GenericRuntime},
};

type Queue = Arc<Mutex<Vec<Arc<Task>>>>;

thread_local! {
    static RUNTIME: RefCell<Mutex<Option<Runtime>>> = const { RefCell::new(Mutex::new(None)) };
}

#[derive(Clone)]
pub struct Runtime {
    queue: Queue,
    spawner: Spawner,
    tasks: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
}

impl GenericRuntime for Runtime {
    fn block_on<F: Future + Send + 'static>(&self, f: F) -> F::Output
    where
        F::Output: Send,
    {
        log::trace!("block_on");
        self.start();
        let rx = self.spawner.spawn_blocking(self.clone(), f).rx;
        block_on_receiver(rx)
    }

    fn wait(self) -> Result<(), Error> {
        while self.tasks.load(Ordering::Relaxed) > 0 {
            self.process_next_task();
        }

        self.active.store(false, Ordering::SeqCst);

        Ok(())
    }
}

impl Runtime {
    fn new() -> Self {
        let queue = Arc::new(Mutex::new(vec![]));

        Self {
            spawner: Spawner {
                queue: queue.clone(),
            },
            queue,
            tasks: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start(&self) {
        if self.active.fetch_or(true, Ordering::SeqCst) {
            return;
        }

        RUNTIME.with_borrow({
            let runtime = self.clone();
            |x| {
                let mut binding = x.lock().unwrap();
                assert!(binding.is_none(), "Cannot start a Runtime within a Runtime");
                *binding = Some(runtime);
            }
        });
    }

    fn next_task(&self) -> Option<Arc<Task>> {
        let mut queue = self.queue.lock().unwrap();
        let task_count = queue.len();
        if task_count == 0 {
            return None;
        }
        let index = queue
            .iter()
            .enumerate()
            .filter(|(_, x)| x.block)
            .map(|(i, _)| i)
            .choose(&mut RNG.clone())
            .unwrap_or_else(|| RNG.gen_range(0..task_count));

        Some(queue.remove(index))
    }

    fn process_next_task(&self) -> bool {
        let Some(task) = self.next_task() else {
            return false;
        };

        task.process();

        true
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.start();
        self.spawner.spawn(self.clone(), future)
    }

    /// # Panics
    ///
    /// * If the `RUNTIME` `Mutex` fails to lock
    #[must_use]
    pub fn current() -> Option<Self> {
        RUNTIME.with_borrow(|x| x.lock().unwrap().clone())
    }

    fn current_unwrap() -> Self {
        Self::current().unwrap_or_else(|| panic!("No runtime"))
    }
}

pub struct JoinHandle<T: Send> {
    rx: futures::channel::oneshot::Receiver<T>,
}

impl<T: Send> Future for JoinHandle<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let receiver = Pin::new(&mut self.get_mut().rx);
        match receiver.poll(cx) {
            Poll::Ready(x) => Poll::Ready(x.ok()),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Spawner {
    queue: Queue,
}

impl Spawner {
    fn spawn<T: Send + 'static>(
        &self,
        runtime: Runtime,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        let (tx, rx) = futures::channel::oneshot::channel();

        let wrapped = async move {
            let _ = tx.send(future.await);
        };

        self.inner_spawn(&Task::new(runtime, false, wrapped));

        JoinHandle { rx }
    }

    fn spawn_blocking<T: Send + 'static>(
        &self,
        runtime: Runtime,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        log::trace!("spawn_blocking");
        let (tx, rx) = futures::channel::oneshot::channel();

        let wrapped = async move {
            let _ = tx.send(future.await);
        };

        self.inner_spawn_blocking(&Task::new(runtime, true, wrapped));

        JoinHandle { rx }
    }

    fn inner_spawn(&self, task: &Arc<Task>) {
        self.queue.lock().unwrap().push(task.clone());
    }

    fn inner_spawn_blocking(&self, task: &Arc<Task>) {
        self.queue.lock().unwrap().push(task.clone());
    }
}

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    Runtime::current_unwrap().spawn(future);
}

pub fn block_on(future: impl Future<Output = ()> + Send + 'static) {
    Runtime::current_unwrap().block_on(future);
}

/// # Errors
///
/// * If the thread fails to join
pub fn wait() -> Result<(), Error> {
    Runtime::current_unwrap().wait()
}

struct Task {
    runtime: Runtime,
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    block: bool,
}

impl Task {
    fn new(
        runtime: Runtime,
        block: bool,
        future: impl Future<Output = ()> + Send + 'static,
    ) -> Arc<Self> {
        runtime.tasks.fetch_add(1, Ordering::Relaxed);
        Arc::new(Self {
            runtime,
            future: Mutex::new(Box::pin(future)),
            block,
        })
    }

    fn waker(self: &Arc<Self>) -> Waker {
        self.clone().into()
    }

    fn poll(self: &Arc<Self>) -> Poll<()> {
        let waker = self.waker();
        let mut ctx = Context::from_waker(&waker);
        self.future.lock().unwrap().as_mut().poll(&mut ctx)
    }

    fn process(self: Arc<Self>) {
        if self.block {
            while self.poll().is_pending() {
                std::thread::yield_now();
            }
        } else if self.poll().is_pending() {
            self.wake();
        }
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        Runtime::current_unwrap()
            .tasks
            .fetch_sub(1, Ordering::Relaxed);
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        if self.block {
            self.runtime.spawner.inner_spawn_blocking(&self);
        } else {
            self.runtime.spawner.inner_spawn(&self);
        }
    }
}

fn block_on_receiver<T>(mut receiver: futures::channel::oneshot::Receiver<T>) -> T {
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut receiver = Pin::new(&mut receiver);

    loop {
        match receiver.as_mut().poll(&mut cx) {
            Poll::Ready(Ok(x)) => return x,
            Poll::Ready(Err(..)) => panic!("Task was cancelled"),
            Poll::Pending => {
                if let Some(runtime) = Runtime::current() {
                    if !runtime.process_next_task() {
                        std::thread::yield_now();
                    }
                } else {
                    std::thread::yield_now();
                }
            }
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn build_runtime(_builder: &Builder) -> Result<Runtime, Error> {
    Ok(Runtime::new())
}

#[cfg(test)]
mod test {
    #[allow(unused)]
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{runtime::Builder, simulator::runtime::build_runtime, task};

    #[test]
    fn rt_current_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn rt_current_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test]
    fn rt_multi_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test]
    fn rt_multi_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }
}
