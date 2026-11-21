# Switchy PTY

Pseudo-terminals and the processes running on them, with a native backend and
(planned) a deterministic simulator backend.

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
