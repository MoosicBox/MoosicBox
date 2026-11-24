#![cfg(feature = "init")]

use moosicbox_log_runtime::writer::non_blocking;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

// Stall exactly one write (not the producer); returning a short write also
// exercises the worker's write_all path. First flush publishes the whole batch.
struct Gate {
    enter: mpsc::Sender<()>,
    release: Option<mpsc::Receiver<()>>,
    output: Arc<Mutex<Vec<u8>>>,
}
impl Write for Gate {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(release) = self.release.take() {
            self.enter.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let n = buf.len().min(1);
        self.output.lock().unwrap().extend_from_slice(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn fifo_capacity_is_in_buffers_not_bytes_and_flush_does_not_wait() {
    let (enter, entered) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let output = Arc::new(Mutex::new(Vec::new()));
    let (mut writer, guard) = non_blocking(Gate {
        enter,
        release: Some(gate),
        output: output.clone(),
    });
    let mut clone = writer.clone();
    writer.write_all(b"first").unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 0..127_999 {
        writer.write_all(b".").unwrap();
    }
    clone.write_all(b"last").unwrap();
    assert_eq!(writer.write(b"discard").unwrap(), 7);
    clone.flush().unwrap();
    assert!(output.lock().unwrap().is_empty());
    release.send(()).unwrap();
    drop(guard); // does not require either producer to disconnect
    clone.write_all(b"late").unwrap();
    let bytes = output.lock().unwrap();
    assert_eq!(bytes.len(), 5 + 127_999 + 4);
    assert!(bytes.starts_with(b"first") && bytes.ends_with(b"last"));
    assert!(bytes[5..bytes.len() - 4].iter().all(|&b| b == b'.'));
}

#[test]
fn blocked_sink_bounds_guard_wait_even_with_live_writer() {
    let (enter, entered) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let (mut writer, guard) = non_blocking(Gate {
        enter,
        release: Some(gate),
        output: Arc::default(),
    });
    writer.write_all(b"first").unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let start = Instant::now();
    drop(guard);
    assert!((Duration::from_millis(900)..Duration::from_secs(4)).contains(&start.elapsed()));
    release.send(()).unwrap();
}

#[test]
fn full_queue_bounds_shutdown_admission() {
    let (enter, entered) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let (mut writer, guard) = non_blocking(Gate {
        enter,
        release: Some(gate),
        output: Arc::default(),
    });
    writer.write_all(b"first").unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 0..128_000 {
        writer.write_all(b".").unwrap();
    }
    let start = Instant::now();
    drop(guard);
    assert!((Duration::from_millis(90)..Duration::from_millis(900)).contains(&start.elapsed()));
    release.send(()).unwrap();
}

#[test]
#[ignore = "local append throughput comparison, not a timing assertion"]
fn append_throughput() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5 {
        let path = dir.path().join(i.to_string());
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let (mut writer, guard) = non_blocking(file);
        let start = Instant::now();
        for _ in 0..100_000 {
            writer.write_all(&[b'x'; 128]).unwrap();
        }
        let enqueue = start.elapsed();
        drop(guard);
        let drained = start.elapsed();
        assert_eq!(std::fs::metadata(path).unwrap().len(), 12_800_000);
        eprintln!("file 100k x 128 bytes: enqueue={enqueue:?} drained={drained:?}");
    }
}

#[test]
#[ignore = "local throughput comparison, not a timing assertion"]
fn throughput() {
    for _ in 0..5 {
        let (mut writer, guard) = non_blocking(io::sink());
        let start = Instant::now();
        for _ in 0..100_000 {
            writer.write_all(&[b'x'; 128]).unwrap();
        }
        let enqueue = start.elapsed();
        drop(guard);
        eprintln!(
            "100k x 128 bytes: enqueue={enqueue:?} drained={:?}",
            start.elapsed()
        );
    }
}
