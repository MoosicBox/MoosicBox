//! Bounded, lossy background writes for tracing file sinks.

use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
enum Message {
    Bytes(Vec<u8>),
    Shutdown,
}

/// Each `write` queues one entire buffer, or discards it on overload/disconnect.
/// Producer `flush` does not wait for the sink.
#[derive(Clone, Debug)]
pub struct NonBlocking {
    sender: SyncSender<Message>,
    dropped: Arc<AtomicUsize>,
}

impl NonBlocking {
    /// Number of discarded buffers across all clones (saturating).
    #[must_use]
    pub fn dropped_lines(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl Write for NonBlocking {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.sender.try_send(Message::Bytes(buf.to_vec())).is_err() {
            let _ = self
                .dropped
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1));
        }
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.write(buf).map(|_| ())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for NonBlocking {
    type Writer = Self;
    fn make_writer(&self) -> Self {
        self.clone()
    }
}

/// Drop before finalizing files or manifests. Shutdown is best effort: at most
/// 100 ms to enqueue a FIFO marker, then 1 s to observe the batch flush.
#[derive(Debug)]
#[must_use = "keep the guard alive while writing logs"]
pub struct WorkerGuard {
    sender: SyncSender<Message>,
    // Preserve Sync for handles kept in process-wide statics. Only Drop reads it.
    flushed: Mutex<Receiver<()>>,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match self.sender.try_send(Message::Shutdown) {
                Ok(()) => {
                    let _ = self
                        .flushed
                        .get_mut()
                        .expect("exclusive guard")
                        .recv_timeout(Duration::from_secs(1));
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(_)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        println!(
                            "Failed to send shutdown signal to logging worker. Error: Shutdown"
                        );
                        return;
                    }
                    thread::sleep(remaining.min(Duration::from_millis(1)));
                }
            }
        }
    }
}

/// Start a single worker with a FIFO capacity of 128,000 write buffers.
///
/// # Panics
/// Panics if the worker thread cannot be created.
#[must_use = "retain the guard for the lifetime of the sink"]
pub fn non_blocking(sink: impl Write + Send + 'static) -> (NonBlocking, WorkerGuard) {
    let (sender, receiver) = mpsc::sync_channel(128_000);
    let (ack, flushed) = mpsc::sync_channel(0);
    thread::Builder::new()
        .name("tracing-appender".into())
        .spawn(move || run(sink, &receiver, &ack))
        .expect("failed to spawn logging worker");
    let writer = NonBlocking {
        sender: sender.clone(),
        dropped: Arc::default(),
    };
    (
        writer,
        WorkerGuard {
            sender,
            flushed: Mutex::new(flushed),
        },
    )
}

fn run(mut sink: impl Write, receiver: &Receiver<Message>, ack: &SyncSender<()>) {
    'worker: loop {
        let mut message = receiver.recv().ok();
        while let Some(Message::Bytes(bytes)) = message {
            if sink.write_all(&bytes).is_err() {
                continue 'worker;
            }
            message = match receiver.try_recv() {
                Ok(next) => Some(next),
                Err(TryRecvError::Empty) => {
                    let _ = sink.flush();
                    continue 'worker;
                }
                Err(TryRecvError::Disconnected) => None,
            };
        }
        // Like the incumbent, a failed batch flush does not acknowledge shutdown.
        if sink.flush().is_ok() {
            let _ = ack.send(());
            break;
        }
        // A disconnected queue can never receive another record; don't spin on a
        // permanently broken sink after the last sender has gone away.
        if message.is_none() {
            break;
        }
    }
    if let Err(err) = sink.flush() {
        eprintln!("Failed to flush. Error: {err}");
    }
}
