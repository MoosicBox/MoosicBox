//! Behaviour of the simulated kernel, run under the deterministic scheduler.
#![cfg(feature = "simulator")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::{Read as _, Write as _};
use std::time::Duration;

use switchy_pty::{
    PtyPair, PtySize, Signal, SpawnCommand, Stdio,
    simulator::{self, Config, Faults, Personality, ProcessState},
};
use switchy_thread::simulator as scheduler;

fn config(personality: Personality) -> Config {
    Config {
        personality,
        pid_reuse: 0.5,
        faults: Faults::default(),
    }
}

/// Run `f` in a fresh simulation with the default programs, failing on leaked threads.
fn run<R>(seed: u64, personality: Personality, f: impl FnOnce() -> R) -> R {
    let outcome = scheduler::run(seed, || {
        simulator::configure(config(personality));
        simulator::install_default_programs();
        let result = f();
        simulator::kill_all();
        // Let killed programs unwind and exit.
        scheduler::run_until_idle();
        result
    });
    assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
    match outcome.result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn read_all(reader: &mut impl std::io::Read) -> Vec<u8> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output).unwrap();
    output
}

#[test]
fn output_and_exit_codes_reach_the_master() {
    for seed in 0..10 {
        let (output, exit) = run(seed, Personality::Linux, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(SpawnCommand::new("exit").arg("3")).unwrap();
            drop(slave);
            let output = read_all(&mut master.try_clone_reader().unwrap());
            (output, child.wait().unwrap())
        });
        assert_eq!(output, b"bye\r\n");
        assert_eq!(exit.exit_code(), 3);
    }
}

#[test]
fn a_flood_arrives_complete_and_in_order_under_backpressure() {
    for seed in 0..10 {
        let output = run(seed, Personality::MacOs, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave
                .spawn(SpawnCommand::new("flood").args(["20000", "333"]))
                .unwrap();
            drop(slave);
            let mut reader = master.try_clone_reader().unwrap();
            let mut output = Vec::new();
            let mut buffer = [0_u8; 100];
            loop {
                match reader.read(&mut buffer).unwrap() {
                    0 => break,
                    read => output.extend_from_slice(&buffer[..read]),
                }
                // A slow reader: the flood must wait rather than lose output.
                switchy_thread::thread::sleep(Duration::from_millis(1));
            }
            child.wait().unwrap();
            output
        });
        let expected: Vec<u8> = (0_u8..26).cycle().take(20_000).map(|i| b'a' + i).collect();
        assert_eq!(output, expected, "seed {seed}");
    }
}

#[test]
fn typed_input_is_echoed_and_read_by_the_program() {
    let output = run(1, Personality::Linux, || {
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
        drop(slave);
        let mut writer = master.take_writer().unwrap();
        writer.write_all(b"hi\r").unwrap();
        // Dropping the writer types a newline and end of input, so cat exits.
        drop(writer);
        let mut reader = master.try_clone_reader().unwrap();
        child.wait().unwrap();
        let mut output = vec![0; 64];
        let read = reader.read(&mut output).unwrap();
        output.truncate(read);
        output
    });
    // Echo of "hi\n", cat's copy of it, then the echoed newline and cat's copy.
    assert!(
        output.starts_with(b"hi\r\n"),
        "{:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn typed_lines_are_edited_and_read_a_line_at_a_time() {
    for personality in [Personality::Linux, Personality::MacOs] {
        let output = run(2, personality, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
            drop(slave);
            let mut writer = master.take_writer().unwrap();
            // Erase, kill and word erase edit the line; end of file sends a partial line,
            // then ends the input.
            writer
                .write_all(b"abc\x7f\x7fd\rxyz\x15hi\rone two\x17three\rpart\x04\x04")
                .unwrap();
            let exit = child.wait().unwrap();
            assert!(exit.success());
            drop(writer);
            let mut reader = master.try_clone_reader().unwrap();
            read_all(&mut reader)
        });
        let text = String::from_utf8_lossy(&output);
        // The echo shows the editing; cat copies each finished line, and the partial one.
        assert!(
            text.starts_with("abc\x08 \x08\x08 \x08d\r\nxyz\x08 \x08\x08 \x08\x08 \x08hi\r\n"),
            "{text:?}"
        );
        assert!(
            text.contains("ad\r\nhi\r\none three\r\npart"),
            "{personality:?}: {text:?}"
        );
    }
}

#[test]
fn interrupt_reaches_the_whole_process_group() {
    for seed in 0..10 {
        let signals = run(seed, Personality::Linux, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(SpawnCommand::new("shell").arg("2")).unwrap();
            drop(slave);
            let mut reader = master.try_clone_reader().unwrap();
            let mut started = [0_u8; 16];
            let _ = reader.read(&mut started).unwrap();
            switchy_pty::signal_group(child.id(), Signal::Interrupt).unwrap();
            let exit = child.wait().unwrap();
            scheduler::run_until_idle();
            (exit, simulator::snapshot())
        });
        let (exit, snapshot) = signals;
        assert_eq!(exit.signal(), Some("Interrupt"));
        assert!(
            snapshot
                .processes
                .iter()
                .all(|p| p.state != ProcessState::Running),
            "seed {seed}: {snapshot:#?}"
        );
    }
}

#[test]
fn a_child_that_ignores_hangups_is_killed_after_the_grace() {
    let exit = run(2, Personality::Linux, || {
        let PtyPair {
            slave,
            master: _master,
        } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut child = slave.spawn(&SpawnCommand::new("ignore-hup")).unwrap();
        drop(slave);
        switchy_thread::thread::sleep(Duration::from_millis(10));
        child.kill().unwrap();
        child.wait().unwrap()
    });
    assert_eq!(exit.signal(), Some("Killed"));
}

#[test]
fn a_lingering_child_keeps_a_linux_terminal_open_but_not_a_macos_one() {
    for personality in [Personality::Linux, Personality::MacOs] {
        let output = run(3, personality, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave
                .spawn(SpawnCommand::new("daemonize").arg("500"))
                .unwrap();
            drop(slave);
            child.wait().unwrap();
            read_all(&mut master.try_clone_reader().unwrap())
        });
        let text = String::from_utf8_lossy(&output).into_owned();
        assert!(text.contains("parent done"), "{personality:?}: {text}");
        assert_eq!(
            text.contains("lingered"),
            personality == Personality::Linux,
            "{personality:?}: {text}"
        );
    }
}

#[test]
fn captured_stdout_is_a_pipe_and_stderr_the_terminal() {
    let (piped, terminal) = run(4, Personality::Linux, || {
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut command = SpawnCommand::new("echo");
        command
            .arg("piped")
            .stdin(Stdio::Null)
            .stdout(Stdio::Piped)
            .controlling_terminal(false);
        let mut child = slave.spawn(&command).unwrap();
        let mut other = SpawnCommand::new("echo-err");
        other.arg("terminal");
        let mut second = slave.spawn(&other).unwrap();
        drop(slave);
        let piped = read_all(&mut child.take_stdout().unwrap());
        child.wait().unwrap();
        second.wait().unwrap();
        (piped, read_all(&mut master.try_clone_reader().unwrap()))
    });
    assert_eq!(piped, b"piped\n");
    assert_eq!(terminal, b"terminal\r\n");
}

#[test]
fn resizing_notifies_the_program() {
    let output = run(5, Personality::Linux, || {
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut child = slave.spawn(&SpawnCommand::new("resize-reporter")).unwrap();
        drop(slave);
        let mut reader = master.try_clone_reader().unwrap();
        let mut first = [0_u8; 16];
        let _ = reader.read(&mut first).unwrap();
        master.resize(PtySize::new(100, 40)).unwrap();
        let mut second = [0_u8; 16];
        let read = reader.read(&mut second).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        second[..read].to_vec()
    });
    assert_eq!(output, b"100x40\r\n");
}

#[test]
fn a_group_kill_racing_a_fork_leaves_no_child_behind() {
    for seed in 0..40 {
        let survivors = run(seed, Personality::Linux, || {
            let PtyPair {
                slave,
                master: _master,
            } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(SpawnCommand::new("shell").arg("3")).unwrap();
            drop(slave);
            // Kill at once, while the shell may still be starting its children.
            switchy_pty::signal_group(child.id(), Signal::Kill).unwrap();
            child.wait().unwrap();
            scheduler::run_until_idle();
            simulator::snapshot()
                .processes
                .into_iter()
                .filter(|p| p.state == ProcessState::Running)
                .count()
        });
        assert_eq!(survivors, 0, "seed {seed}");
    }
}

#[test]
fn an_unreaped_exit_keeps_the_id_from_reuse() {
    let reused = run(9, Personality::Linux, || {
        let PtyPair {
            slave,
            master: _master,
        } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut first = slave.spawn(SpawnCommand::new("exit").arg("0")).unwrap();
        let exit = first.wait_unreaped().unwrap();
        assert!(exit.success());
        // While the zombie stands, no new process may take its id.
        let others: Vec<_> = (0..20)
            .map(|_| slave.spawn(SpawnCommand::new("exit").arg("0")).unwrap())
            .collect();
        let reused = others.iter().any(|c| c.id() == first.id());
        assert_eq!(first.wait().unwrap(), exit);
        for mut other in others {
            other.wait().unwrap();
        }
        reused
    });
    assert!(!reused);
}

#[test]
fn missing_programs_are_not_found() {
    let error = run(6, Personality::Linux, || {
        assert!(simulator::program_registered("cat"));
        assert!(!simulator::program_registered("no-such-program"));
        let PtyPair { slave, .. } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        slave
            .spawn(&SpawnCommand::new("no-such-program"))
            .unwrap_err()
    });
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn nonblocking_reads_would_block_and_wait_readable_times_out_in_simulated_time() {
    let (would_block, ready) = run(7, Personality::Linux, || {
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut child = slave.spawn(&SpawnCommand::new("never-reads")).unwrap();
        drop(slave);
        master.set_nonblocking(true).unwrap();
        let mut reader = master.try_clone_reader().unwrap();
        let would_block = reader.read(&mut [0; 8]).unwrap_err().kind();
        let ready = reader.wait_readable(Duration::from_millis(10)).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        (would_block, ready)
    });
    assert_eq!(would_block, std::io::ErrorKind::WouldBlock);
    assert!(!ready);
}

#[test]
fn a_seed_replays_the_same_run() {
    let scenario = || {
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut flood = slave
            .spawn(SpawnCommand::new("flood").args(["3000", "100"]))
            .unwrap();
        let mut echo = slave.spawn(SpawnCommand::new("echo").arg("x")).unwrap();
        drop(slave);
        let output = read_all(&mut master.try_clone_reader().unwrap());
        (
            flood.id(),
            echo.id(),
            flood.wait().unwrap(),
            echo.wait().unwrap(),
            output,
        )
    };
    for seed in 0..10 {
        let first = scheduler::run(seed, || {
            simulator::configure(config(Personality::Linux));
            simulator::install_default_programs();
            scenario()
        });
        let second = scheduler::run(seed, || {
            simulator::configure(config(Personality::Linux));
            simulator::install_default_programs();
            scenario()
        });
        assert_eq!(
            first.report.trace_hash, second.report.trace_hash,
            "seed {seed}"
        );
        assert_eq!(first.result.unwrap(), second.result.unwrap(), "seed {seed}");
    }
}

#[test]
fn faults_interrupt_and_shorten_master_reads() {
    let output = scheduler::run(8, || {
        simulator::configure(Config {
            personality: Personality::Linux,
            pid_reuse: 0.0,
            faults: Faults {
                interrupted: 0.3,
                short: 0.5,
                ..Faults::default()
            },
        });
        simulator::install_default_programs();
        let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut child = slave
            .spawn(SpawnCommand::new("flood").args(["2000", "50"]))
            .unwrap();
        drop(slave);
        let mut reader = master.try_clone_reader().unwrap();
        let mut output = Vec::new();
        let mut interrupted = 0;
        let mut buffer = [0_u8; 64];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => output.extend_from_slice(&buffer[..read]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => interrupted += 1,
                Err(error) => panic!("{error}"),
            }
        }
        child.wait().unwrap();
        (output, interrupted)
    })
    .result
    .unwrap();
    let expected: Vec<u8> = (0_u8..26).cycle().take(2000).map(|i| b'a' + i).collect();
    assert_eq!(output.0, expected);
    assert!(output.1 > 0);
}

#[test]
fn a_single_process_can_be_signalled_and_reports_its_directory() {
    run(3, Personality::Linux, || {
        let PtyPair {
            slave,
            master: _master,
        } = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut command = SpawnCommand::new("sleep");
        command.arg("60000").cwd("/work/here");
        let mut child = slave.spawn(&command).unwrap();
        let pid = child.id();
        let process = simulator::snapshot()
            .processes
            .into_iter()
            .find(|p| p.pid == pid)
            .unwrap();
        assert_eq!(process.cwd, std::path::Path::new("/work/here"));
        simulator::signal_process(pid, Signal::Kill).unwrap();
        assert!(child.wait().unwrap().signal_number().is_some());
        assert_eq!(
            simulator::signal_process(pid, Signal::Kill)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    });
}

/// Read from `reader` until `want` arrived (or it ended).
fn read_until(reader: &mut impl std::io::Read, want: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 256];
    while !output.windows(want.len()).any(|w| w == want) {
        match reader.read(&mut buffer).unwrap() {
            0 => break,
            read => output.extend_from_slice(&buffer[..read]),
        }
    }
    output
}

#[test]
fn a_windows_console_asks_for_the_cursor_and_holds_output_until_answered() {
    for seed in 0..10 {
        let (before, after, exit) = run(seed, Personality::Windows, || {
            assert!(switchy_pty::behavior().startup_cursor_query);
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(SpawnCommand::new("exit").arg("3")).unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            let mut writer = master.take_writer().unwrap();
            let mut buffer = [0_u8; 64];
            let read = reader.read(&mut buffer).unwrap();
            let before = buffer[..read].to_vec();
            // Nothing else until the report: the program's output is held.
            assert!(!reader.wait_readable(Duration::from_secs(1)).unwrap());
            writer.write_all(b"\x1b[1;1R").unwrap();
            let exit = child.wait().unwrap();
            let after = read_until(&mut reader, b"bye\r\n");
            drop(writer);
            (before, after, exit)
        });
        assert_eq!(before, b"\x1b[6n");
        assert_eq!(after, b"bye\r\n");
        assert_eq!(exit.exit_code(), 3);
        assert_eq!(exit.signal(), None);
    }
}

#[test]
fn a_windows_cursor_report_never_reaches_the_program_but_other_input_does() {
    for seed in 0..10 {
        let output = run(seed, Personality::Windows, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let _child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            let mut writer = master.take_writer().unwrap();
            // Typed input around the report: an escape that is not a report is typed as is.
            writer.write_all(b"a\x1b[2;").unwrap();
            writer.write_all(b"7Rb\x1bxc\n").unwrap();
            read_until(&mut reader, b"\r\n")
        });
        // The query, the echo of what was typed, then cat's copy of the line.
        let text = String::from_utf8_lossy(&output).into_owned();
        assert!(text.starts_with("\x1b[6n"), "{text:?}");
        assert!(!text.contains("[2;7R"), "{text:?}");
        assert!(text.contains("ab\x1bxc"), "{text:?}");
    }
}

#[test]
fn windows_output_ends_when_the_console_closes_not_when_programs_end() {
    for seed in 0..10 {
        run(seed, Personality::Windows, || {
            assert!(switchy_pty::behavior().output_ends_with_console);
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(SpawnCommand::new("exit").arg("0")).unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            master
                .clone_writer()
                .unwrap()
                .write_all(b"\x1b[1;1R")
                .unwrap();
            child.wait().unwrap();
            drop(slave);
            let output = read_until(&mut reader, b"bye\r\n");
            assert!(output.ends_with(b"bye\r\n"));
            // The program ended and the slave is gone, but the console is open.
            assert!(!reader.wait_readable(Duration::from_secs(1)).unwrap());
            drop(master);
            assert_eq!(read_all(&mut reader), b"");
        });
    }
}

#[test]
fn closing_a_windows_console_ends_its_processes_once_its_output_is_read() {
    for seed in 0..10 {
        let exit = run(seed, Personality::Windows, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(&SpawnCommand::new("sleep")).unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            // The unread query is drained by this reader while the console closes.
            let drain = switchy_thread::thread::spawn(move || read_all(&mut reader));
            drop(slave);
            drop(master);
            assert_eq!(drain.join().unwrap(), b"\x1b[6n");
            child.wait().unwrap()
        });
        assert_eq!(exit.signal(), None);
        assert_eq!(exit.exit_code(), 0xC000_013A);
    }
}

#[test]
fn windows_processes_are_terminated_never_signalled() {
    for seed in 0..10 {
        let exit = run(seed, Personality::Windows, || {
            assert!(!switchy_pty::behavior().process_groups);
            assert!(!switchy_pty::behavior().terminal_device);
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            assert_eq!(master.tty_name(), None);
            assert_eq!(master.foreground_process_group(), None);
            let mut child = slave.spawn(&SpawnCommand::new("ignore-hup")).unwrap();
            let pid = child.id();
            assert_eq!(
                switchy_pty::signal_group(pid, Signal::Interrupt)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::Unsupported
            );
            assert_eq!(
                simulator::signal_process(pid, Signal::Terminate)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::Unsupported
            );
            child.kill().unwrap();
            child.wait().unwrap()
        });
        assert_eq!(exit.signal(), None);
        assert_eq!(exit.exit_code(), 1);
    }
}

#[test]
fn unix_personalities_behave_as_unix() {
    for personality in [Personality::Linux, Personality::MacOs] {
        run(0, personality, || {
            let behavior = switchy_pty::behavior();
            assert_eq!(behavior, switchy_pty::Behavior::of(false));
            let PtyPair {
                slave: _slave,
                master,
            } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            assert!(master.tty_name().is_some());
        });
    }
}

#[test]
fn closing_masters_hangs_up_unix_terminals_and_closes_windows_consoles() {
    for personality in [Personality::Linux, Personality::MacOs, Personality::Windows] {
        for seed in 0..5 {
            let (exit, survivor) = run(seed, personality, || {
                let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut child = slave.spawn(&SpawnCommand::new("sleep")).unwrap();
                let mut stubborn = slave.spawn(&SpawnCommand::new("ignore-hup")).unwrap();
                let mut reader = master.try_clone_reader().unwrap();
                if personality.is_windows() {
                    master
                        .take_writer()
                        .unwrap()
                        .write_all(b"\x1b[1;1R")
                        .unwrap();
                }
                // Once it says so, it ignores hang-ups.
                read_until(&mut reader, b"ignoring hangups");
                // Another terminal, whose master stays open.
                let other = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut bystander = other.slave.spawn(&SpawnCommand::new("sleep")).unwrap();
                let terminal = simulator::process_io(child.id()).unwrap().terminal.unwrap();
                simulator::close_masters(&[terminal]);
                let exit = child.wait().unwrap();
                // The master side reads to its end, whatever still holds the terminal.
                let _ = read_all(&mut reader);
                let survivor = stubborn.try_wait().unwrap().is_none()
                    && bystander.try_wait().unwrap().is_none();
                drop((slave, master, other));
                (exit, survivor)
            });
            if personality.is_windows() {
                assert_eq!(exit.exit_code(), 0xC000_013A, "{personality:?}");
            } else {
                assert_eq!(exit.signal(), Some("Hangup"), "{personality:?}");
            }
            // Hang-ups are ignorable, and other terminals stay open.
            assert!(survivor, "{personality:?}");
        }
    }
}

#[test]
fn a_terminal_forgets_its_session_when_the_leader_exits() {
    for personality in [Personality::Linux, Personality::MacOs] {
        for seed in 0..5 {
            run(seed, personality, || {
                let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut child = slave.spawn(SpawnCommand::new("exit").arg("0")).unwrap();
                let mut reader = master.try_clone_reader().unwrap();
                let _ = read_until(&mut reader, b"bye\r\n");
                child.wait().unwrap();
                assert_eq!(master.foreground_process_group(), None, "{personality:?}");
                // A new session that reuses the old one's id is not reached by this terminal.
                let other = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut unrelated = other.slave.spawn(&SpawnCommand::new("sleep")).unwrap();
                drop((slave, master, reader));
                assert!(unrelated.try_wait().unwrap().is_none(), "{personality:?}");
                drop(other);
            });
        }
    }
}

/// Run `f` in a fresh simulation whose kernel has `faults`.
fn run_with_faults<R>(
    seed: u64,
    personality: Personality,
    faults: Faults,
    f: impl FnOnce() -> R,
) -> R {
    let outcome = scheduler::run(seed, || {
        simulator::configure(Config {
            personality,
            pid_reuse: 0.0,
            faults,
        });
        simulator::install_default_programs();
        let result = f();
        simulator::kill_all();
        scheduler::run_until_idle();
        result
    });
    match outcome.result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// The simulated time now.
fn now() -> std::time::Instant {
    scheduler::deadline_after(Duration::ZERO).unwrap()
}

#[test]
fn an_outside_killer_kills_processes_and_says_so() {
    for seed in 0..5 {
        let exit = run_with_faults(
            seed,
            Personality::Linux,
            Faults {
                kill: 2.0,
                ..Faults::default()
            },
            || {
                let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut child = slave
                    .spawn(SpawnCommand::new("sleep").arg("60000"))
                    .unwrap();
                drop(slave);
                let mut reader = master.try_clone_reader().unwrap();
                let mut buffer = [0_u8; 16];
                // Kernel operations give the killer its chances.
                let exit = loop {
                    if let Some(exit) = child.try_wait().unwrap() {
                        break exit;
                    }
                    let _ = reader.read(&mut buffer);
                };
                let killed = simulator::signal_log()
                    .iter()
                    .any(|record| record.by_fault && record.signal == Signal::Kill);
                (exit, killed)
            },
        );
        assert_eq!(exit.0.signal(), Some("Killed"), "seed {seed}");
        assert!(exit.1, "seed {seed}: the kill is recorded as a fault's");
    }
}

#[test]
fn delayed_signals_arrive_later_but_arrive() {
    for seed in 0..5 {
        let (running_after_send, waited) = run_with_faults(
            seed,
            Personality::Linux,
            Faults {
                signal_delay: 1.0,
                ..Faults::default()
            },
            || {
                let PtyPair {
                    slave,
                    master: _master,
                } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let mut child = slave
                    .spawn(SpawnCommand::new("sleep").arg("60000"))
                    .unwrap();
                let pid = child.id();
                let start = now();
                simulator::signal_process(pid, Signal::Terminate).unwrap();
                let running = simulator::snapshot()
                    .processes
                    .iter()
                    .any(|p| p.pid == pid && p.state == ProcessState::Running);
                child.wait().unwrap();
                (running, now().duration_since(start))
            },
        );
        assert!(running_after_send, "seed {seed}: not delivered at once");
        assert!(waited > Duration::ZERO, "seed {seed}");
    }
}

#[test]
fn master_reads_can_fail_with_input_output_errors() {
    let error = run_with_faults(
        1,
        Personality::Linux,
        Faults {
            read_error: 1.0,
            ..Faults::default()
        },
        || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let _child = slave
                .spawn(SpawnCommand::new("sleep").arg("60000"))
                .unwrap();
            let mut buffer = [0_u8; 16];
            master
                .try_clone_reader()
                .unwrap()
                .read(&mut buffer)
                .unwrap_err()
        },
    );
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
}

#[test]
fn master_writes_can_stall() {
    for seed in 0..5 {
        let stalled = run_with_faults(
            seed,
            Personality::Linux,
            Faults {
                write_stall: 1.0,
                ..Faults::default()
            },
            || {
                let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let _child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
                let mut writer = master.take_writer().unwrap();
                let start = now();
                writer.write_all(b"x").unwrap();
                now().duration_since(start)
            },
        );
        assert!(stalled > Duration::ZERO, "seed {seed}");
    }
}

#[test]
fn a_pseudo_console_may_ask_for_the_cursor_late_or_never() {
    let mut never = 0;
    let mut late = 0;
    for seed in 0..20 {
        let (first, waited) = run_with_faults(
            seed,
            Personality::Windows,
            Faults {
                startup_query: 1.0,
                ..Faults::default()
            },
            || {
                let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
                let _child = slave.spawn(SpawnCommand::new("echo").arg("hi")).unwrap();
                let mut reader = master.try_clone_reader().unwrap();
                let start = now();
                let mut buffer = [0_u8; 4];
                let read = reader.read(&mut buffer).unwrap();
                (buffer[..read].to_vec(), now().duration_since(start))
            },
        );
        if first.starts_with(b"\x1b[") {
            assert!(
                waited > Duration::ZERO,
                "seed {seed}: a late query takes time"
            );
            late += 1;
        } else {
            never += 1;
        }
    }
    assert!(never > 0 && late > 0, "never {never}, late {late}");
}

#[test]
fn a_console_holds_the_echo_of_typing_until_its_cursor_query_is_answered() {
    for seed in 0..10 {
        let (before, after) = run(seed, Personality::Windows, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let _child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
            let mut writer = master.take_writer().unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            writer.write_all(b"hi").unwrap();
            let mut buffer = [0_u8; 64];
            let read = reader.read(&mut buffer).unwrap();
            let before = buffer[..read].to_vec();
            writer.write_all(b"\x1b[1;1R").unwrap();
            let read = reader.read(&mut buffer).unwrap();
            (before, buffer[..read].to_vec())
        });
        assert_eq!(
            before, b"\x1b[6n",
            "seed {seed}: only the query comes first"
        );
        assert!(after.starts_with(b"hi"), "seed {seed}: {after:?}");
    }
}

#[test]
fn a_line_typed_before_a_console_handshake_still_ends_at_enter() {
    for seed in 0..10 {
        let output = run(seed, Personality::Windows, || {
            let PtyPair { slave, master } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let mut child = slave.spawn(&SpawnCommand::new("cat")).unwrap();
            drop(slave);
            let mut writer = master.take_writer().unwrap();
            let mut reader = master.try_clone_reader().unwrap();
            // Typed before the cursor report: Enter still ends the line for the program.
            writer.write_all(b"hi\r").unwrap();
            let mut buffer = [0_u8; 64];
            let _query = reader.read(&mut buffer).unwrap();
            writer.write_all(b"\x1b[1;1R").unwrap();
            let mut output = Vec::new();
            while !String::from_utf8_lossy(&output).contains("hi\r\nhi\r\n") {
                let read = reader.read(&mut buffer).unwrap();
                assert!(read > 0, "seed {seed}: output ended at {output:?}");
                output.extend_from_slice(&buffer[..read]);
            }
            let _ = child.kill();
            output
        });
        assert!(
            String::from_utf8_lossy(&output).contains("hi\r\nhi\r\n"),
            "seed {seed}"
        );
    }
}

#[test]
fn a_detached_child_leaves_the_session_on_unix_and_stays_in_the_job_on_windows() {
    for personality in [Personality::Linux, Personality::MacOs, Personality::Windows] {
        let (parent, child) = run(2, personality, || {
            simulator::register_program("detacher", |process| {
                if process.spawn_session(&["sleep", "3600000"]).is_err() {
                    return 1;
                }
                loop {
                    process.sleep(Duration::from_secs(60));
                }
            });
            let PtyPair {
                slave,
                master: _master,
            } = PtyPair::open(PtySize::new(80, 24)).unwrap();
            let parent = slave.spawn(&SpawnCommand::new("detacher")).unwrap();
            loop {
                let snapshot = simulator::snapshot();
                let find = |pid: u32| snapshot.processes.iter().find(|p| p.pid == pid).cloned();
                if let Some(child) = snapshot
                    .processes
                    .iter()
                    .find(|p| p.parent == Some(parent.id()))
                {
                    break (find(parent.id()).unwrap(), child.clone());
                }
                switchy_thread::thread::sleep(Duration::from_millis(1));
            }
        });
        assert_eq!(child.state, ProcessState::Running, "{personality:?}");
        assert!(child.terminal.is_none(), "{personality:?}: {child:?}");
        if personality.is_windows() {
            assert_eq!(
                child.session_serial, parent.session_serial,
                "{personality:?}"
            );
        } else {
            assert_eq!(child.session, child.pid, "{personality:?}");
            assert_ne!(
                child.session_serial, parent.session_serial,
                "{personality:?}"
            );
        }
    }
}
