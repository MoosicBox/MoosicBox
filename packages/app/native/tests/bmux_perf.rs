//! Opt-in structural benchmark of production Albums UI and the real presenter.
//! No server, PTY, filesystem cache, helper parser, or terminal ownership.
//! Run with --ignored --nocapture after obtaining the compiler slot. Build with
//! the normal bmux image features; use release for comparisons. Optional
//! `BMUX_PERF_ARTIFACTS` names a NEW directory for per-case terminal streams.
//! Each `pages-N-MODE.init.bin` contains initial output and all warm-up output;
//! replay it before `pages-N-MODE.bin`. Metadata locators are relative to that
//! new directory (null when not saved). `prefix_bytes` is the init length;
//! `byte_range` stays delta-relative and `stream_byte_range` includes the prefix.
//! `BMUX_PERF_SOURCE_ID` and `BMUX_PERF_BUILD_ID` are coordinator-supplied
//! provenance (including dirty source/dependency identity and build flags).
//! Missing identity is reported as null, never inferred from a stale HEAD.
//! `BMUX_PERF_SINK=recording` (default) retains replay bytes; `counting` only
//! counts accepted bytes, without copying them. Counting never saves streams,
//! even when `BMUX_PERF_ARTIFACTS` is set. Both are wall-time CPU-work proxies,
//! not CPU-time measurements; compare only matching sink modes.
//! `BMUX_PERF_BUFFERING=unbuffered` (default) is the legacy fragment reference;
//! `buffered_64k` batches successful output before the instrumented memory sink.
//! This is not production `FrameWriter` error/Drop equivalence. Compare only
//! identical sink AND buffering metadata; neither measures application latency.
//! Sink timings below measure only in-memory copies/locks, NOT PTY blocking.
//! Compare the exact same 64 wheel inputs, not a time-limited input generator.
//! Cold = terminal-resident-cold: freshly decoded in-memory resources + fresh
//! presenter, NOT cold asynchronous fetch/network/decode. Warm = the same
//! resources/presenter after an unmeasured down/up trace. Decode is separately
//! timed. This is one shared 128px icon texture, NOT a photo/decode throughput
//! benchmark or asynchronous resource scheduling benchmark. No pass/fail timing
//! thresholds: compare baseline/candidate artifacts with identical build flags.
#![cfg(feature = "bmux")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

use bmux_tui::{
    event::{Event, MouseEvent, MouseEventKind},
    geometry::{Point, Rect, Size},
    terminal::Terminal,
};
use bmux_tui_runtime::{Presenter, Program, RuntimeEvent};
use hyperchad::{
    renderer::View,
    renderer_bmux::{
        graphics::{GraphicsSource, SourcePolicy},
        layout::node_key,
        presentation::BmuxPresenter,
        program::BmuxProgram,
        renderer::{BmuxConfig, ImageProtocol},
    },
    transformer::{Container, Element},
};
use moosicbox_music_models::{AlbumSort, api::ApiAlbum};

const ARTWORK: &[u8] = include_bytes!("../public/img/icon128.png");
const STEPS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SinkMode {
    #[default]
    Recording,
    Counting,
}

impl SinkMode {
    fn parse(value: Option<&str>) -> Result<Self, &'static str> {
        match value {
            None | Some("recording") => Ok(Self::Recording),
            Some("counting") => Ok(Self::Counting),
            Some(_) => Err("BMUX_PERF_SINK must be counting or recording"),
        }
    }

    fn artifacts(self, path: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
        path.filter(|_| self == Self::Recording)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Counting => "counting",
        }
    }

    const fn timing_semantics(self) -> &'static str {
        match self {
            Self::Recording => "in_memory_copy_and_lock_not_output_blocking",
            Self::Counting => "in_memory_count_and_lock_not_output_blocking",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Buffering {
    Unbuffered,
    Buffered64k,
}

impl Buffering {
    fn parse(value: Option<&str>) -> Result<Self, &'static str> {
        match value {
            None | Some("unbuffered") => Ok(Self::Unbuffered),
            Some("buffered_64k") => Ok(Self::Buffered64k),
            Some(_) => Err("BMUX_PERF_BUFFERING must be unbuffered or buffered_64k"),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Unbuffered => "unbuffered",
            Self::Buffered64k => "buffered_64k",
        }
    }

    const fn capacity(self) -> usize {
        match self {
            Self::Unbuffered => 0,
            Self::Buffered64k => 64 * 1024,
        }
    }
}

// Successful-output batching only: std BufWriter's Drop/error behavior is NOT
// the production FrameWriter discard/no-publication-on-drop contract.
enum FixtureWriter {
    Direct(MemoryWriter),
    Buffered(io::BufWriter<MemoryWriter>),
}

impl FixtureWriter {
    fn new(writer: MemoryWriter, buffering: Buffering) -> Self {
        match buffering {
            Buffering::Unbuffered => Self::Direct(writer),
            Buffering::Buffered64k => {
                Self::Buffered(io::BufWriter::with_capacity(buffering.capacity(), writer))
            }
        }
    }
}

impl Write for FixtureWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Direct(writer) => writer.write(bytes),
            Self::Buffered(writer) => writer.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Direct(writer) => writer.flush(),
            Self::Buffered(writer) => writer.flush(),
        }
    }
}

#[derive(Default)]
struct Output {
    mode: SinkMode,
    accepted_bytes: usize,
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
    write_ns: u128,
    max_write_ns: u128,
    flush_ns: u128,
}

#[derive(Clone, Default)]
struct MemoryWriter(Arc<Mutex<Output>>);

impl MemoryWriter {
    fn new(mode: SinkMode) -> Self {
        Self(Arc::new(Mutex::new(Output {
            mode,
            ..Output::default()
        })))
    }
}

impl Write for MemoryWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let started = Instant::now();
        let mut output = self
            .0
            .lock()
            .map_err(|_| io::Error::other("sink lock poisoned"))?;
        if output.mode == SinkMode::Recording {
            output.bytes.extend_from_slice(bytes);
        }
        output.accepted_bytes += bytes.len();
        output.writes += 1;
        let elapsed = started.elapsed().as_nanos();
        output.write_ns += elapsed;
        output.max_write_ns = output.max_write_ns.max(elapsed);
        drop(output);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let started = Instant::now();
        let mut output = self
            .0
            .lock()
            .map_err(|_| io::Error::other("sink lock poisoned"))?;
        output.flushes += 1;
        output.flush_ns += started.elapsed().as_nanos();
        drop(output);
        Ok(())
    }
}

#[test]
fn sink_mode_parsing_is_pure() {
    assert_eq!(SinkMode::parse(None), Ok(SinkMode::Recording));
    assert_eq!(SinkMode::parse(Some("recording")), Ok(SinkMode::Recording));
    assert_eq!(SinkMode::parse(Some("counting")), Ok(SinkMode::Counting));
    assert_eq!(Buffering::parse(None), Ok(Buffering::Unbuffered));
    assert_eq!(
        Buffering::parse(Some("unbuffered")),
        Ok(Buffering::Unbuffered)
    );
    assert_eq!(
        Buffering::parse(Some("buffered_64k")),
        Ok(Buffering::Buffered64k)
    );
    for value in ["", "buffered", "BUFFERED_64K"] {
        assert!(Buffering::parse(Some(value)).is_err());
    }
    for value in ["", "Counting", "recording ", "other"] {
        assert!(SinkMode::parse(Some(value)).is_err());
    }
}

#[test]
fn counting_suppresses_artifact_directory_and_locators() {
    let requested = std::path::PathBuf::from("requested-replay-directory");
    assert_eq!(
        SinkMode::Recording.artifacts(Some(requested.clone())),
        Some(requested.clone())
    );
    let artifacts = SinkMode::Counting.artifacts(Some(requested));
    // The matrix creates/writes only through this selected directory; no
    // directory or stream file is created when counting was selected.
    assert!(artifacts.is_none());
    let prefix_locator = artifacts.as_ref().map(|_| "pages-1-warm.init.bin");
    let delta_locator = artifacts.as_ref().map(|_| "pages-1-warm.bin");
    assert_eq!(
        serde_json::json!([prefix_locator, delta_locator]),
        serde_json::json!([null, null])
    );
}

// This is sink accounting equivalence, not committed-image/pixel equivalence.
// BmuxPresenter does not expose its owned Terminal images. image_snapshot()
// prepares/paints/commits through a different terminal and cannot authenticate
// this presenter's committed image state. Keep the recording upload check and
// the separate production image/full-layout oracles; recording replay is still
// required before graphics correctness claims. Counting retains no replay.
#[test]
fn sinks_count_identical_production_output_without_counting_storage() {
    moosicbox_app_native::register_api_sources();
    let root = fixture(1);
    let mut keys = BTreeSet::new();
    walk(&root, &mut Vec::new(), &mut |node, path| {
        if matches!(node.element, Element::Image { .. }) {
            keys.insert(node_key(node, path));
        }
    });
    for images in [false, true] {
        let mut results = Vec::new();
        for mode in [SinkMode::Recording, SinkMode::Counting] {
            let mut config = BmuxConfig {
                width: 140,
                height: 40,
                graphics_enabled: images,
                image_protocol: if images {
                    ImageProtocol::Kitty
                } else {
                    ImageProtocol::Disabled
                },
                ..Default::default()
            };
            config.graphics.max_entries = keys.len() + 128;
            config.graphics.max_total_bytes = 256 * 1024 * 1024;
            let (actions, _action_rx) = flume::unbounded();
            let (resizes, _resize_rx) = flume::unbounded();
            let mut program = BmuxProgram::new(config.clone(), actions, resizes);
            program.apply_view(View::from(root.clone()), None).unwrap();
            if images {
                let policy = SourcePolicy {
                    allow_memory: true,
                    ..Default::default()
                };
                for key in &keys {
                    program
                        .graphics
                        .decode(key, &GraphicsSource::Memory, &policy, ARTWORK)
                        .unwrap();
                }
            }
            // Each Terminal owns a fresh presenter/image registry. Neither sink
            // changes the production buffering or image identity generator.
            let writer = MemoryWriter::new(mode);
            let terminal = Terminal::new(writer.clone(), Rect::new(0, 0, 140, 40));
            let mut presenter = BmuxPresenter::new(terminal, &config);
            presenter.present(&mut program).unwrap();
            if images && mode == SinkMode::Recording {
                let output = writer.0.lock().unwrap();
                let stream = String::from_utf8_lossy(&output.bytes);
                assert!(
                    stream.split("\x1b_G").skip(1).any(|command| {
                        let Some((body, _)) = command.split_once("\x1b\\") else {
                            return false;
                        };
                        let Some((header, payload)) = body.split_once(';') else {
                            return false;
                        };
                        header.split(',').any(|field| field == "a=t")
                            && header.split(',').any(|field| field.starts_with("f="))
                            && !payload.is_empty()
                    }),
                    "recording must contain a complete Kitty upload with payload"
                );
                drop(output);
            }
            // Warm the same full down/up trace as the opt-in matrix, then
            // compare actual accepted counts/calls for every warm frame.
            for index in 0..STEPS * 2 {
                wheel(&mut program, index);
                presenter.present(&mut program).unwrap();
            }
            let mut checkpoints = Vec::new();
            for index in 0..STEPS * 2 {
                wheel(&mut program, index);
                presenter.present(&mut program).unwrap();
                let output = writer.0.lock().unwrap();
                checkpoints.push((output.accepted_bytes, output.writes, output.flushes));
            }
            presenter.cleanup().unwrap();
            let output = writer.0.lock().unwrap();
            assert!(output.accepted_bytes > 0);
            assert!(output.flushes > 0);
            if mode == SinkMode::Recording {
                assert_eq!(output.bytes.len(), output.accepted_bytes);
                assert!(output.bytes.contains(&0x1b), "production control output");
            } else {
                assert_eq!(output.bytes, [] as [u8; 0]);
                assert_eq!(output.bytes.capacity(), 0);
            }
            checkpoints.push((output.accepted_bytes, output.writes, output.flushes));
            results.push(checkpoints);
            drop(output);
        }
        assert_eq!(results[0], results[1], "graphics_enabled={images}");
    }
}

#[test]
fn buffering_preserves_successful_kitty_trace_bytes_and_flushes() {
    moosicbox_app_native::register_api_sources();
    let root = fixture(1);
    let keys = image_keys(&root);
    let mut results = Vec::new();
    for buffering in [Buffering::Unbuffered, Buffering::Buffered64k] {
        let mut config = BmuxConfig {
            width: 140,
            height: 40,
            graphics_enabled: true,
            image_protocol: ImageProtocol::Kitty,
            ..Default::default()
        };
        config.graphics.max_entries = keys.len() + 128;
        let (actions, _action_rx) = flume::unbounded();
        let (resizes, _resize_rx) = flume::unbounded();
        let mut program = BmuxProgram::new(config.clone(), actions, resizes);
        program.apply_view(View::from(root.clone()), None).unwrap();
        decode_fixture_images(&mut program, &keys, true);
        let writer = MemoryWriter::new(SinkMode::Recording);
        let terminal = Terminal::new(
            FixtureWriter::new(writer.clone(), buffering),
            Rect::new(0, 0, 140, 40),
        );
        let mut presenter = BmuxPresenter::new(terminal, &config);
        presenter.present(&mut program).unwrap();
        for index in 0..STEPS * 4 {
            wheel(&mut program, index % (STEPS * 2));
            presenter.present(&mut program).unwrap();
        }
        presenter.cleanup().unwrap();
        let output = writer.0.lock().unwrap();
        assert_eq!(output.accepted_bytes, output.bytes.len());
        assert!(output.bytes.windows(7).any(|part| part == b"\x1b_Ga=t,"));
        results.push((
            output.bytes.clone(),
            output.accepted_bytes,
            output.writes,
            output.flushes,
        ));
        drop(output);
    }
    // Owner namespaces have equal-width fresh registry IDs here; compare actual
    // wire bytes, not normalized placements or invented counting-sink bytes.
    assert_eq!(results[0].0, results[1].0);
    assert_eq!(results[0].1, results[1].1);
    assert_eq!(results[0].3, results[1].3);
    assert!(results[1].2 < results[0].2);
}

#[test]
fn sinks_preserve_write_and_flush_error_contract() {
    for mode in [SinkMode::Recording, SinkMode::Counting] {
        let mut writer = MemoryWriter::new(mode);
        assert_eq!(writer.write(b"\x1b[0m").unwrap(), 4);
        assert_eq!(writer.write(b"").unwrap(), 0);
        writer.flush().unwrap();
        {
            let output = writer.0.lock().unwrap();
            assert_eq!(
                (output.accepted_bytes, output.writes, output.flushes),
                (4, 2, 1)
            );
            if mode == SinkMode::Recording {
                assert_eq!(output.bytes, b"\x1b[0m");
            } else {
                assert_eq!(output.bytes, [] as [u8; 0]);
            }
            drop(output);
        }
        let shared = writer.0.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = shared.lock().unwrap();
                panic!("deliberate sink poison");
            })
            .join()
            .is_err()
        );
        assert!(writer.write(b"not accepted").is_err());
        assert!(writer.flush().is_err());
        let output = writer
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(
            (output.accepted_bytes, output.writes, output.flushes),
            (4, 2, 1)
        );
    }
}

fn walk(node: &Container, path: &mut Vec<usize>, apply: &mut impl FnMut(&Container, &[usize])) {
    apply(node, path);
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        walk(child, path, apply);
        path.pop();
    }
}

fn image_keys(root: &Container) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    walk(root, &mut Vec::new(), &mut |node, path| {
        if matches!(node.element, Element::Image { .. }) {
            keys.insert(node_key(node, path));
        }
    });
    keys
}

fn albums_slot(root: &mut Container) -> Option<&mut Container> {
    if root.str_id.as_deref() == Some("albums") {
        return Some(root);
    }
    root.children.iter_mut().find_map(albums_slot)
}

fn fixture(pages: u32) -> Container {
    let count = pages * 100;
    let albums: Vec<_> = (0..count)
        .map(|index| ApiAlbum {
            album_id: u64::from(index + 1).into(),
            title: format!("Perf album {index:04}"),
            artist: format!("Artist {:02}", index % 8),
            contains_cover: true,
            ..Default::default()
        })
        .collect();
    let state = moosicbox_app_native_ui::state::State::default();
    let mut root: Container =
        moosicbox_app_native_ui::albums::albums(&state, &[], AlbumSort::NameAsc, None).into();
    let cards =
        moosicbox_app_native_ui::albums::show_albums("https://fixture.invalid", albums.iter(), 200);
    // Only populate the production grid slot; do not reproduce card/shell layout.
    let slot = albums_slot(&mut root).expect("production Albums grid slot");
    assert_eq!(cards.len(), usize::try_from(count).unwrap());
    slot.children = cards;
    root
}

fn wheel(program: &mut BmuxProgram, index: usize) {
    let kind = if index < STEPS {
        MouseEventKind::ScrollDown
    } else {
        MouseEventKind::ScrollUp
    };
    // Deliberately drop returned commands: every image is registered in memory;
    // this fixture never executes application loads, playback or HTTP effects.
    let _ = program
        .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
            kind,
            Point::new(105, 20),
        ))))
        .unwrap();
}

#[test]
#[ignore = "expensive opt-in performance matrix; needs compiler slot and release build"]
fn production_albums_fixed_input_matrix() {
    moosicbox_app_native::register_api_sources();
    assert!(
        std::env::var_os("BMUX_IMAGE_PROTOCOL").is_none(),
        "unset BMUX_IMAGE_PROTOCOL: fixture explicitly selects its protocol"
    );
    let sink_value = std::env::var("BMUX_PERF_SINK");
    let sink_mode = match &sink_value {
        Ok(value) => SinkMode::parse(Some(value)),
        Err(std::env::VarError::NotPresent) => SinkMode::parse(None),
        Err(std::env::VarError::NotUnicode(_)) => panic!("BMUX_PERF_SINK must be Unicode"),
    }
    .expect("invalid benchmark sink");
    let buffering_value = std::env::var("BMUX_PERF_BUFFERING");
    let buffering = match &buffering_value {
        Ok(value) => Buffering::parse(Some(value)),
        Err(std::env::VarError::NotPresent) => Buffering::parse(None),
        Err(std::env::VarError::NotUnicode(_)) => panic!("BMUX_PERF_BUFFERING must be Unicode"),
    }
    .expect("invalid benchmark buffering");
    let artifacts =
        sink_mode.artifacts(std::env::var_os("BMUX_PERF_ARTIFACTS").map(std::path::PathBuf::from));
    if let Some(path) = &artifacts {
        fs::create_dir(path).expect("artifact directory must not already exist");
    }
    for pages in [1, 3, 10] {
        let root = fixture(pages);
        let mut keys = BTreeSet::new();
        let mut nodes = 0;
        walk(&root, &mut Vec::new(), &mut |node, path| {
            nodes += 1;
            if matches!(node.element, Element::Image { .. }) {
                keys.insert(node_key(node, path));
            }
        });
        assert!(keys.len() >= usize::try_from(pages * 100).unwrap());
        for mode in ["text", "cold", "warm"] {
            run_case(
                pages,
                mode,
                &root,
                &keys,
                nodes,
                artifacts.as_deref(),
                (sink_mode, buffering),
            );
        }
    }
}

fn decode_fixture_images(program: &mut BmuxProgram, keys: &BTreeSet<String>, images: bool) -> u128 {
    let decode = Instant::now();
    if images {
        let policy = SourcePolicy {
            allow_memory: true,
            ..Default::default()
        };
        for key in keys {
            program
                .graphics
                .decode(key, &GraphicsSource::Memory, &policy, ARTWORK)
                .unwrap();
        }
    }
    decode.elapsed().as_micros()
}

fn initial_present(
    presenter: &mut BmuxPresenter<FixtureWriter>,
    program: &mut BmuxProgram,
) -> u128 {
    let started = Instant::now();
    presenter.present(program).unwrap();
    started.elapsed().as_micros()
}

fn run_case(
    pages: u32,
    mode: &str,
    root: &Container,
    keys: &BTreeSet<String>,
    nodes: usize,
    artifacts: Option<&std::path::Path>,
    (sink_mode, buffering): (SinkMode, Buffering),
) {
    let images = mode != "text";
    let mut config = BmuxConfig {
        width: 140,
        height: 40,
        graphics_enabled: images,
        image_protocol: if images {
            ImageProtocol::Kitty
        } else {
            ImageProtocol::Disabled
        },
        // Keep all fixture resources resident: no accidental LRU-dependent
        // warm/cold comparison. These explicit limits are reported below.
        max_nodes: 200_000,
        ..Default::default()
    };
    config.graphics.max_entries = keys.len() + 128;
    config.graphics.max_total_bytes = 256 * 1024 * 1024;
    let (actions, _action_rx) = flume::unbounded();
    let (resizes, _resize_rx) = flume::unbounded();
    let mut program = BmuxProgram::new(config.clone(), actions, resizes);
    program.apply_view(View::from(root.clone()), None).unwrap();
    let decode_us = decode_fixture_images(&mut program, keys, images);
    let writer = MemoryWriter::new(sink_mode);
    let terminal = Terminal::new(
        FixtureWriter::new(writer.clone(), buffering),
        Rect::new(0, 0, 140, 40),
    );
    let mut presenter = BmuxPresenter::new(terminal, &config);
    let initial_us = initial_present(&mut presenter, &mut program);
    if images && sink_mode == SinkMode::Recording {
        let output = writer.0.lock().unwrap();
        assert!(
            output.bytes.windows(3).any(|part| part == b"\x1b_G"),
            "image mode must actually emit Kitty graphics; enable bmux images"
        );
        drop(output);
    }
    if mode == "warm" {
        for index in 0..STEPS * 2 {
            wheel(&mut program, index);
            presenter.present(&mut program).unwrap();
        }
    }
    // Preserve the complete initialization history, including warm-up graphics
    // residency, before resetting measured offsets. Prefix + delta reconstructs
    // the stream; existing .bin files and byte_range remain delta-relative.
    let (prefix_bytes, prefix_locator) = {
        let output = writer.0.lock().unwrap();
        let locator = artifacts.map(|path| {
            let name = format!("pages-{pages}-{mode}.init.bin");
            fs::write(path.join(&name), &output.bytes).unwrap();
            name
        });
        (output.accepted_bytes, locator)
    };
    // All cases now start at the same top scroll position. No parser is
    // running: event/presenter time cannot include Python parser backlog.
    *writer.0.lock().unwrap() = Output {
        mode: sink_mode,
        ..Output::default()
    };
    eprintln!(
        "{}",
        serde_json::json!({
            "fixture": "production_albums_v1", "pages": pages, "albums": pages * 100,
            "source_id": std::env::var("BMUX_PERF_SOURCE_ID").ok(),
            "build_id": std::env::var("BMUX_PERF_BUILD_ID").ok(),
            "trace_id": "wheel-v1-32-down-32-up-105-20",
            "timer_scope_version": 2,
            "prefix_locator": prefix_locator,
            "prefix_bytes": prefix_bytes,
            "delta_locator": artifacts.map(|_| format!("pages-{pages}-{mode}.bin")),
            "stream_offset_semantics": if sink_mode == SinkMode::Recording {
                "byte_range_delta_relative_stream_byte_range_prefix_inclusive"
            } else {
                "accepted_byte_counts_no_stream"
            },
            "sink_mode": sink_mode.name(),
            "buffering_mode": buffering.name(),
            "buffer_capacity": buffering.capacity(),
            "sink_timing_semantics": sink_mode.timing_semantics(),
            "mode": mode, "nodes": nodes, "registered_images": if images {keys.len()} else {0},
            "decode_us": decode_us, "initial_prepare_present_us": initial_us,
            "wheel_count": STEPS * 2, "wheel_point": [105, 20],
            "resident_budget_bytes": config.graphics.max_total_bytes,
            "max_entries": config.graphics.max_entries,
            "sink": "in_memory_no_parser", "viewport": [140, 40],
        })
    );
    measure_trace(
        pages,
        mode,
        &writer,
        &mut program,
        &mut presenter,
        prefix_bytes,
        sink_mode,
    );
    // Save once after timing; offline replay uses the exact recorded bytes.
    if let Some(path) = artifacts {
        fs::write(
            path.join(format!("pages-{pages}-{mode}.bin")),
            &writer.0.lock().unwrap().bytes,
        )
        .unwrap();
    }
    presenter.cleanup().unwrap();
}

fn measure_trace(
    pages: u32,
    mode: &str,
    writer: &MemoryWriter,
    program: &mut BmuxProgram,
    presenter: &mut BmuxPresenter<FixtureWriter>,
    prefix_bytes: usize,
    sink_mode: SinkMode,
) {
    for index in 0..STEPS * 2 {
        let (before, writes_before, flushes_before, write_ns_before, flush_ns_before) = {
            let mut output = writer.0.lock().unwrap();
            output.max_write_ns = 0;
            (
                output.accepted_bytes,
                output.writes,
                output.flushes,
                output.write_ns,
                output.flush_ns,
            )
        };
        let started = Instant::now();
        wheel(program, index);
        let update_us = started.elapsed().as_micros();
        let started = Instant::now();
        presenter.present(program).unwrap();
        let prepare_present_us = started.elapsed().as_micros();
        // Public diagnostic value API only: no layout/ABI/serde assumptions.
        // Read after presentation so this attempt includes paint and commit.
        // prepare_us includes project/controls/reveal/resources: do not add
        // those nested stages again. prepare_us, paint_us and commit_us are
        // disjoint wall-time scopes, not CPU time. Missing fields in older
        // artifacts are unknown, never zero. Their sum leaves an approximate
        // residual within prepare_present_us (rounding/instrumentation remain).
        let performance = program.performance_snapshot();
        let output = writer.0.lock().unwrap();
        let end = output.accepted_bytes;
        eprintln!(
            "{}",
            serde_json::json!({
                "pages": pages, "mode": mode, "frame": index,
                "direction": if index < STEPS {"down"} else {"up"},
                "update_us": update_us, "prepare_present_us": prepare_present_us,
                "sequence": performance.sequence,
                "full_layouts": performance.full_layouts,
                "projection_used": performance.projection_used,
                "prepare_us": performance.prepare_us,
                "commit_us": performance.commit_us,
                "project_us": performance.project_us,
                "controls_us": performance.controls_us,
                "reveal_us": performance.reveal_us,
                "resource_us": performance.resource_us,
                "paint_us": performance.paint_us,
                "project_visited": performance.project_visited,
                "retained_nodes": performance.retained_nodes,
                "controls_visited": performance.controls_visited,
                "reveal_visited": performance.reveal_visited,
                "resource_visited": performance.resource_visited,
                "paint_visited": performance.paint_visited,
                "byte_range": [before, end],
                "stream_byte_range": (sink_mode == SinkMode::Recording)
                    .then_some([prefix_bytes + before, prefix_bytes + end]),
                "bytes": end - before,
                "writes": output.writes - writes_before,
                "flushes": output.flushes - flushes_before,
                "sink_write_ns": output.write_ns - write_ns_before,
                "sink_max_write_ns": output.max_write_ns,
                "sink_flush_ns": output.flush_ns - flush_ns_before,
            })
        );
        drop(output);
    }
}

/// Exact real-paint oracle, separate from timing. Same-size resize deliberately
/// invalidates reuse on the comparator through the public geometry API.
/// Action hit targets remain covered by renderer-internal projection tests;
/// `BmuxProgram` does not expose its authoritative hit-test scene publicly.
#[test]
#[ignore = "expensive full-layout oracle; needs compiler slot and coherent sources"]
fn production_fixed_trace_cells_and_images_match_full_layout() {
    moosicbox_app_native::register_api_sources();
    for pages in [1, 3, 10] {
        for mode in ["text", "cold", "warm"] {
            let root = fixture(pages);
            let keys = image_keys(&root);
            let mut allowed_keys = BTreeSet::new();
            walk(&root, &mut Vec::new(), &mut |node, path| {
                if matches!(node.element, Element::Image { .. } | Element::Canvas) {
                    allowed_keys.insert(node_key(node, path));
                }
            });
            let make_program = || {
                let mut config = BmuxConfig {
                    width: 140,
                    height: 40,
                    max_nodes: 200_000,
                    graphics_enabled: mode != "text",
                    image_protocol: if mode == "text" {
                        ImageProtocol::Disabled
                    } else {
                        ImageProtocol::Kitty
                    },
                    ..Default::default()
                };
                config.graphics.max_entries = keys.len() + 128;
                config.graphics.max_total_bytes = 256 * 1024 * 1024;
                let (actions, _action_rx) = flume::unbounded();
                let (resizes, _resize_rx) = flume::unbounded();
                let mut program = BmuxProgram::new(config, actions, resizes);
                program.apply_view(View::from(root.clone()), None).unwrap();
                if mode != "text" {
                    let policy = SourcePolicy {
                        allow_memory: true,
                        ..Default::default()
                    };
                    for key in &keys {
                        program
                            .graphics
                            .decode(key, &GraphicsSource::Memory, &policy, ARTWORK)
                            .unwrap();
                    }
                }
                program.set_graphics_available(mode != "text");
                program
            };
            let mut candidate = make_program();
            let mut baseline = make_program();
            let mut actual = Terminal::new(Vec::new(), Rect::new(0, 0, 140, 40));
            let mut expected = Terminal::new(Vec::new(), Rect::new(0, 0, 140, 40));
            let mut actual_namespace = None;
            let mut expected_namespace = None;
            let cycles = if mode == "warm" { 2 } else { 1 };
            for step in 0..=STEPS * 2 * cycles {
                if step > 0 {
                    let index = (step - 1) % (STEPS * 2);
                    wheel(&mut candidate, index);
                    wheel(&mut baseline, index);
                }
                baseline.resize(Size::new(140, 40));
                candidate.prepare().unwrap();
                baseline.prepare().unwrap();
                assert_eq!(
                    baseline.performance_snapshot().full_layouts,
                    1,
                    "same-size resize must force ordinary full layout"
                );
                assert!(
                    !baseline.performance_snapshot().projection_used,
                    "full-layout comparator unexpectedly used projection"
                );
                actual.draw(|cx| candidate.paint(cx)).unwrap();
                expected.draw(|cx| baseline.paint(cx)).unwrap();
                candidate.commit_prepared();
                baseline.commit_prepared();
                assert_eq!(
                    actual.retained_buffer(),
                    expected.retained_buffer(),
                    "glyph/style/width continuation mismatch pages={pages} mode={mode} step={step}"
                );
                let actual_images =
                    logical_images(actual.images(), &mut actual_namespace, &allowed_keys);
                let expected_images =
                    logical_images(expected.images(), &mut expected_namespace, &allowed_keys);
                assert!(
                    actual_images == expected_images,
                    "image snapshot mismatch pages={pages} mode={mode} step={step}:\n{}",
                    image_difference_summary(&actual_images, &expected_images)
                );
                if mode != "text" {
                    assert!(!actual.images().is_empty(), "oracle must paint real images");
                }
                // Byte streams are not the oracle and must not grow across the
                // expensive full-layout trace. Retained cells stay intact.
                actual.writer_mut().clear();
                expected.writer_mut().clear();
            }
        }
    }
}

// Pin once per independently constructed program, never once per frame. Only
// the owner prefix is normalized; the full ordered contribution remains exact.
fn logical_images(
    images: &[bmux_tui::image::ImageContribution],
    namespace: &mut Option<String>,
    source_keys: &BTreeSet<String>,
) -> Vec<bmux_tui::image::ImageContribution> {
    use bmux_tui::image::{ImageContribution, ImageKey};

    images
        .iter()
        .map(|image| {
            let mut image = image.clone();
            let present = matches!(&image, ImageContribution::Present(_));
            let key = match &mut image {
                ImageContribution::Present(placement) => &mut placement.key,
                ImageContribution::Remove(key) => key,
            };
            let (owner, logical) = key
                .as_str()
                .split_once(':')
                .expect("image must have owner namespace");
            let serial = owner
                .strip_prefix("hyperchad-")
                .expect("unexpected image owner");
            assert!(
                !serial.is_empty() && serial.bytes().all(|byte| byte.is_ascii_digit()),
                "invalid image owner namespace"
            );
            let prefix = format!("{owner}:");
            if let Some(pinned) = namespace.as_ref() {
                assert_eq!(pinned, &prefix, "per-program image namespace changed");
            } else {
                assert!(present, "cannot authenticate owner from removal");
                assert!(
                    source_keys.contains(logical),
                    "first image must match registered fixture source"
                );
                *namespace = Some(prefix);
            }
            // Fragment suffixes are retained verbatim, never stripped/renumbered.
            assert!(
                source_keys.contains(logical)
                    || logical
                        .rsplit_once(":fragment:")
                        .is_some_and(|(source, part)| source_keys.contains(source)
                            && !part.is_empty()
                            && part.bytes().all(|byte| byte.is_ascii_digit())),
                "image key does not belong to the fixture: owner={owner:?} logical={logical:?}"
            );
            *key = ImageKey::new(logical);
            image
        })
        .collect()
}

#[test]
fn logical_image_comparison_preserves_identity_and_owner_stability() {
    use bmux_tui::image::{ImageContribution, ImageKey};

    let keys = BTreeSet::from(["@0/1".to_owned(), "@0/2".to_owned()]);
    let mut owner = Some("hyperchad-7:".to_owned());
    let input = vec![ImageContribution::Remove(ImageKey::new(
        "hyperchad-7:@0/1:fragment:2",
    ))];
    let normalized = logical_images(&input, &mut owner, &keys);
    assert_eq!(
        normalized,
        vec![ImageContribution::Remove(ImageKey::new("@0/1:fragment:2"))]
    );
    assert_eq!(owner.as_deref(), Some("hyperchad-7:"));
    for invalid in [
        "hyperchad-8:@0/1",
        "hyperchad-7:@0/9",
        "hyperchad-7:@0/1:fragment:x",
        "bogus:@0/1",
    ] {
        let input = vec![ImageContribution::Remove(ImageKey::new(invalid))];
        assert!(
            std::panic::catch_unwind(|| {
                logical_images(&input, &mut Some("hyperchad-7:".to_owned()), &keys)
            })
            .is_err(),
            "invalid identity accepted: {invalid}"
        );
    }
}

fn image_difference_summary(
    actual: &[bmux_tui::image::ImageContribution],
    expected: &[bmux_tui::image::ImageContribution],
) -> String {
    use bmux_tui::image::ImageContribution;

    let describe = |image: Option<&ImageContribution>| match image {
        Some(ImageContribution::Present(image)) => format!(
            "key={:?} dst={:?} clip={:?} allocation={:?} fit={:?} lifecycle={:?} {}",
            image.key,
            image.destination,
            image.clip,
            image.allocation,
            image.fit,
            image.lifecycle,
            payload_summary(&image.payload),
        ),
        other => format!("{other:?}"),
    };
    let mut differences = vec![format!(
        "contributions actual={} expected={}",
        actual.len(),
        expected.len()
    )];
    for index in 0..actual.len().max(expected.len()) {
        if actual.get(index) != expected.get(index) {
            differences.push(format!(
                "index={index} actual=[{}] expected=[{}]",
                describe(actual.get(index)),
                describe(expected.get(index))
            ));
            if differences.len() == 9 {
                break;
            }
        }
    }
    differences.join("\n")
}

fn payload_summary(payload: &bmux_tui::image::ImagePayload) -> String {
    use bmux_tui::image::ImagePayload;
    // A deterministic diagnostic fingerprint only; equality below still compares
    // every payload byte, so collisions cannot weaken the regression contract.
    let (kind, bytes, width, height) = match payload {
        ImagePayload::Png {
            bytes,
            width,
            height,
        } => ("png".to_owned(), bytes, width, height),
        ImagePayload::Pixels {
            bytes,
            width,
            height,
            format,
        } => (format!("pixels/{format:?}"), bytes, width, height),
    };
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!(
        "{kind} {width}x{height} bytes={} fnv1a64={hash:016x}",
        bytes.len()
    )
}

fn assert_same_images(
    before: &BTreeMap<bmux_tui::image::ImageKey, bmux_tui::image::ImagePlacement>,
    after: &BTreeMap<bmux_tui::image::ImageKey, bmux_tui::image::ImagePlacement>,
    phase: &str,
) {
    if before == after {
        return;
    }
    // Keep failure output independent of payload size and bounded to eight keys.
    let mut differences = Vec::new();
    let mut changed = 0;
    for key in before
        .keys()
        .chain(after.keys().filter(|key| !before.contains_key(*key)))
    {
        let old = before.get(key);
        let new = after.get(key);
        if old == new {
            continue;
        }
        changed += 1;
        if differences.len() >= 8 {
            continue;
        }
        let describe = |image: Option<&bmux_tui::image::ImagePlacement>| {
            image.map_or_else(
                || "absent".to_owned(),
                |image| {
                    format!(
                        "dst={:?} clip={:?} allocation={:?} fit={:?} lifecycle={:?} {}",
                        image.destination,
                        image.clip,
                        image.allocation,
                        image.fit,
                        image.lifecycle,
                        payload_summary(&image.payload),
                    )
                },
            )
        };
        differences.push(format!(
            "key={key:?} before=[{}] after=[{}]",
            describe(old),
            describe(new)
        ));
    }
    panic!(
        "{phase}: {} -> {} images; {changed} differing keys (first eight):\n{}",
        before.len(),
        after.len(),
        differences.join("\n")
    );
}

fn image_snapshot(
    program: &mut BmuxProgram,
    terminal: &mut Terminal<Vec<u8>>,
) -> BTreeMap<bmux_tui::image::ImageKey, bmux_tui::image::ImagePlacement> {
    use bmux_tui::image::ImageContribution;

    program.prepare().unwrap();
    terminal.draw(|cx| program.paint(cx)).unwrap();
    program.commit_prepared();
    let mut snapshot = BTreeMap::new();
    for contribution in terminal.images() {
        if let ImageContribution::Present(image) = contribution {
            assert!(image.destination.width > 0 && image.destination.height > 0);
            assert_eq!(
                image.destination.intersection(image.clip),
                image.destination
            );
            assert_eq!(
                image.destination.intersection(Rect::new(0, 0, 140, 40)),
                image.destination
            );
            if let Some(bounds) = image.allocation {
                assert!(bounds.covers(image.destination));
            }
            assert!(
                snapshot.insert(image.key.clone(), image.clone()).is_none(),
                "duplicate image identity in a frame"
            );
        }
    }
    assert!(
        !snapshot.is_empty(),
        "real production images must be submitted"
    );
    snapshot
}

/// Small non-timing contract: the same warmed wheel trace must preserve image
/// identities and bounds, independently of the ignored wall-time matrix.
#[test]
fn production_initial_and_warm_wheel_image_contract() {
    moosicbox_app_native::register_api_sources();
    let root = fixture(1);
    let config = BmuxConfig {
        width: 140,
        height: 40,
        ..Default::default()
    };
    let (actions, _action_rx) = flume::unbounded();
    let (resizes, _resize_rx) = flume::unbounded();
    let mut keys = BTreeSet::new();
    walk(&root, &mut Vec::new(), &mut |node, path| {
        if matches!(node.element, Element::Image { .. }) {
            keys.insert(node_key(node, path));
        }
    });
    // Production cards include cover + playback-control images. Preloading the
    // whole document into the default 128-entry LRU silently evicts earlier
    // registrations. Match the benchmark's explicit resident-resource contract.
    let mut config = config;
    config.graphics.max_entries = keys.len() + 128;
    let mut program = BmuxProgram::new(config, actions, resizes);
    program.apply_view(View::from(root), None).unwrap();
    for key in &keys {
        program
            .graphics
            .register_rgba(key, 1, 1, vec![80, 160, 240, 255])
            .unwrap();
    }
    for key in &keys {
        assert!(
            program.graphics.rgba(key).is_some(),
            "fixture preload evicted {key}"
        );
    }
    program.set_graphics_available(true);
    let mut terminal = Terminal::new(Vec::new(), Rect::new(0, 0, 140, 40));
    let mut snapshots = Vec::new();
    let mut wheels = 0;
    // Two identical one-step down/up cycles, not a long benchmark trace.
    for frame in 0..5 {
        if frame != 0 {
            wheel(&mut program, if frame % 2 == 1 { 0 } else { STEPS });
            wheels += 1;
        }
        snapshots.push(image_snapshot(&mut program, &mut terminal));
    }
    assert_eq!(wheels, 4);
    assert_same_images(
        &snapshots[1],
        &snapshots[3],
        "same warm down position changed identity/payload/bounds",
    );
    assert_same_images(
        &snapshots[2],
        &snapshots[4],
        "same warm up position changed identity/payload/bounds",
    );
    assert!(
        snapshots[1] != snapshots[2],
        "wheel must move actual image geometry"
    );

    // Bounded synchronous burst + prepare barrier. This exercises actual
    // Program::update accumulation, not runtime queue admission/coalescing.
    // Compare at both direction barriers: a dropped down/up pair must not be
    // hidden by checking only the final return to the top.
    let start = image_snapshot(&mut program, &mut terminal);
    for _ in 0..8 {
        wheel(&mut program, 0);
        let _ = image_snapshot(&mut program, &mut terminal);
    }
    let paced_down = image_snapshot(&mut program, &mut terminal);
    for _ in 0..8 {
        wheel(&mut program, STEPS);
        let _ = image_snapshot(&mut program, &mut terminal);
    }
    assert_same_images(
        &start,
        &image_snapshot(&mut program, &mut terminal),
        "paced trace failed to return to initial geometry",
    );
    for _ in 0..8 {
        wheel(&mut program, 0);
    }
    let burst_down = image_snapshot(&mut program, &mut terminal);
    assert_same_images(&paced_down, &burst_down, "burst dropped wheel movement");
    assert!(start != burst_down, "burst must move actual image geometry");
    for _ in 0..8 {
        wheel(&mut program, STEPS);
    }
    assert_same_images(
        &start,
        &image_snapshot(&mut program, &mut terminal),
        "burst return barrier changed geometry",
    );
}
