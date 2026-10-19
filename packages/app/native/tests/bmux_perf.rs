//! Opt-in structural benchmark of production Albums UI and the real presenter.
//! No server, PTY, filesystem cache, helper parser, or terminal ownership.
//! Run with --ignored --nocapture after obtaining the compiler slot. Build with
//! the normal bmux image features; use release for comparisons. Optional
//! `BMUX_PERF_ARTIFACTS` names a NEW directory for per-case terminal streams.
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
    geometry::{Point, Rect},
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

#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
}

#[derive(Clone, Default)]
struct MemoryWriter(Arc<Mutex<Output>>);
impl Write for MemoryWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut output = self.0.lock().unwrap();
        output.bytes.extend_from_slice(bytes);
        output.writes += 1;
        drop(output);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
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
    let artifacts = std::env::var_os("BMUX_PERF_ARTIFACTS").map(std::path::PathBuf::from);
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
            run_case(pages, mode, &root, &keys, nodes, artifacts.as_deref());
        }
    }
}

fn run_case(
    pages: u32,
    mode: &str,
    root: &Container,
    keys: &BTreeSet<String>,
    nodes: usize,
    artifacts: Option<&std::path::Path>,
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
    let decode_us = decode.elapsed().as_micros();
    let writer = MemoryWriter::default();
    let terminal = Terminal::new(writer.clone(), Rect::new(0, 0, 140, 40));
    let mut presenter = BmuxPresenter::new(terminal, &config);
    let initial = Instant::now();
    presenter.present(&mut program).unwrap();
    let initial_us = initial.elapsed().as_micros();
    if images {
        assert!(
            !writer
                .0
                .lock()
                .unwrap()
                .bytes
                .windows(3)
                .all(|part| part != b"\x1b_G"),
            "image mode must actually emit Kitty graphics; enable bmux images"
        );
    }
    if mode == "warm" {
        for index in 0..STEPS * 2 {
            wheel(&mut program, index);
            presenter.present(&mut program).unwrap();
        }
    }
    // All cases now start at the same top scroll position. No parser is
    // running: event/presenter time cannot include Python parser backlog.
    *writer.0.lock().unwrap() = Output::default();
    eprintln!(
        "{}",
        serde_json::json!({
            "fixture": "production_albums_v1", "pages": pages, "albums": pages * 100,
            "mode": mode, "nodes": nodes, "registered_images": if images {keys.len()} else {0},
            "decode_us": decode_us, "initial_prepare_present_us": initial_us,
            "wheel_count": STEPS * 2, "wheel_point": [105, 20],
            "resident_budget_bytes": config.graphics.max_total_bytes,
            "max_entries": config.graphics.max_entries,
            "sink": "in_memory_no_parser", "viewport": [140, 40],
        })
    );
    for index in 0..STEPS * 2 {
        let before = writer.0.lock().unwrap().bytes.len();
        let started = Instant::now();
        wheel(&mut program, index);
        let update_us = started.elapsed().as_micros();
        let started = Instant::now();
        presenter.present(&mut program).unwrap();
        let prepare_present_us = started.elapsed().as_micros();
        let end = writer.0.lock().unwrap().bytes.len();
        eprintln!(
            "{}",
            serde_json::json!({
                "pages": pages, "mode": mode, "frame": index,
                "direction": if index < STEPS {"down"} else {"up"},
                "update_us": update_us, "prepare_present_us": prepare_present_us,
                "byte_range": [before, end],
                "bytes": end - before,
            })
        );
    }
    // Save once after timing; offline analyzer/parser speed is a separate
    // replay measurement over this exact immutable byte stream.
    if let Some(path) = &artifacts {
        fs::write(
            path.join(format!("pages-{pages}-{mode}.bin")),
            &writer.0.lock().unwrap().bytes,
        )
        .unwrap();
    }
    presenter.cleanup().unwrap();
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

/// Small non-timing contract: the same warmed wheel trace must preserve image
/// identities and bounds, independently of the ignored wall-time matrix.
#[test]
fn production_initial_and_warm_wheel_image_contract() {
    use bmux_tui::image::ImageContribution;

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
        snapshots.push(snapshot);
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
}
