# Switchy PTY

Pseudo-terminals and the processes running on them, with a native backend and
a deterministic simulator backend.

A `PtyPair` is both ends of a new terminal: the `PtyMaster` reads output,
writes input and resizes; the `PtySlave` starts processes with
`PtySlave::spawn`. A `SpawnCommand` describes the process: arguments,
environment (captured from the caller and readable back), working directory,
where each standard stream goes (`Stdio::Terminal`, `Piped` or `Null`) and
whether the terminal becomes its controlling terminal.

- **Native**: `openpty` on Unix; a pseudo-console (ConPTY) on Windows, where
  each process can be started inside a caller's Job Object.
- **Non-blocking use** (Unix): `PtyMaster::set_nonblocking` with
  `PtyReader::wait_readable` and `PtyInput::wait_writable`, and
  `PtyMaster::has_peer` to tell whether anything still holds the terminal open.
- **Process groups** (Unix): `signal_group` signals everything a session
  leader started.

## Simulator

With the `simulator` feature, terminals, pipes, processes and signals are an
in-memory kernel belonging to the current simulation run, and every operation is
a `switchy_thread` switch point, so a run's seed decides every interleaving and
waits use simulated time. Programs are Rust closures registered by name
(`simulator::register_program`, or `simulator::install_default_programs` for a
library of programs such as `cat`, `flood`, `daemonize`, `ignore-hup` and
`resize-reporter`), each running as a simulated thread with a
`simulator::Process` for its streams, signals and children.

It models bounded terminal buffers (writers wait when nobody reads), a line
discipline subset (echo, `ctrl+c`, end of file), window size changes, hang-ups,
sessions and process groups, process ids reused once free, Linux and macOS
terminal lifetimes (`simulator::Personality`), and seeded faults on the master
side (`simulator::Faults`). `simulator::snapshot` and
`simulator::terminal_written` report what really happened, for checking what a
program under test saw.

## Installation

```toml
[dependencies]
switchy_pty = "0.5.0"
```

## Usage

```rust,no_run
use std::io::Read as _;
use switchy_pty::{PtyPair, PtySize, SpawnCommand};

let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24))?;
let mut child = slave.spawn(&SpawnCommand::new("echo"))?;
drop(slave); // only the process keeps the terminal open now
let mut output = String::new();
master.try_clone_reader()?.read_to_string(&mut output)?;
child.wait()?;
# Ok::<(), std::io::Error>(())
```
