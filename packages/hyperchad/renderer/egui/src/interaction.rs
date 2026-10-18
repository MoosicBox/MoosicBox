//! Version-one native interaction observation and capture protocol.
//!
//! No input injection or document mutation is exposed. The caller provides production
//! geometry and the actual XID; the worker alone reads/writes protocol files and PNGs.
//! All strings must already be safe for diagnostics/export. Secret input values must
//! be constructed with `ExportValue::read`, which never reads a secret field.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_JSON: usize = 1024 * 1024;
const MAX_REQUEST: usize = 4096;
const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_STRING: usize = 16 * 1024;
const MAX_SEQUENCE: u64 = 9999;

/// Exported value with redaction enforced before invoking the caller's value reader.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ExportValue(Option<String>);
impl ExportValue {
    /// Secret classification must come from the control specification, not its value.
    #[must_use]
    pub fn read(secret: bool, reader: impl FnOnce() -> Option<String>) -> Self {
        if secret { Self(None) } else { Self(reader()) }
    }
}

/// Pending production work, not protocol worker queue occupancy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Pending {
    pub routes: usize,
    pub resources: usize,
}

/// One generation-qualified semantic node. Keys/parents are supplied by the host
/// from opaque element identity AND generation, never egui paint order or numeric ID.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Node {
    pub generation: u64,
    pub parent: Option<String>,
    pub id: Option<String>,
    pub tag: String,
    pub name: Option<String>,
    pub href: Option<String>,
    pub text: String,
    pub visible: bool,
    pub enabled: bool,
    /// Clipped physical client rectangle: x, y, width, height.
    pub rect_pixels: Option<[f32; 4]>,
    pub value: ExportValue,
    pub scroll_pixels: [f32; 2],
    pub scroll_max_pixels: [f32; 2],
}

/// Complete production snapshot. Version, nonce and PID are stamped by `poll`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct State {
    pub version: u32,
    pub nonce: String,
    pub pid: u32,
    pub window_id: u64,
    pub revision: u64,
    pub frame: u64,
    pub route: String,
    pub ready: bool,
    pub diagnostics: Vec<String>,
    pub pending: Pending,
    pub client_pixels: [u32; 2],
    pub focused_key: Option<String>,
    pub nodes: BTreeMap<String, Node>,
}
impl State {
    const fn settled(&self) -> bool {
        self.ready
            && self.diagnostics.is_empty()
            && self.pending.routes == 0
            && self.pending.resources == 0
    }
    fn stable_with(&self, old: &Self) -> bool {
        self.window_id == old.window_id
            && self.revision == old.revision
            && self.route == old.route
            && self.client_pixels == old.client_pixels
            && self.focused_key == old.focused_key
            && self.nodes == old.nodes
            && self.settled()
            && old.settled()
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.pid == 0
            || self.window_id == 0
            || self.nodes.len() > 4096
            || self.client_pixels.contains(&0)
            || self.client_pixels.iter().any(|n| *n > 16384)
            || self.diagnostics.len() > 32
        {
            return Err("invalid interaction snapshot bounds".into());
        }
        let mut bytes = self.route.len() + self.nonce.len();
        let mut count = |value: &str| -> Result<(), String> {
            if value.len() > MAX_STRING {
                return Err("interaction string exceeds budget".into());
            }
            bytes += value.len();
            if bytes > MAX_JSON / 8 {
                return Err("interaction snapshot exceeds budget".into());
            }
            Ok(())
        };
        count(&self.route)?;
        for diagnostic in &self.diagnostics {
            count(diagnostic)?;
        }
        if self
            .focused_key
            .as_ref()
            .is_some_and(|key| !self.nodes.contains_key(key))
        {
            return Err("missing focused identity".into());
        }
        for (key, node) in &self.nodes {
            if key.is_empty() || key.len() > 256 {
                return Err("invalid semantic key".into());
            }
            count(key)?;
            count(&node.tag)?;
            count(&node.text)?;
            for value in [
                &node.parent,
                &node.id,
                &node.name,
                &node.href,
                &node.value.0,
            ]
            .into_iter()
            .flatten()
            {
                count(value)?;
            }
            let mut seen = BTreeSet::new();
            seen.insert(key.as_str());
            let mut parent = node.parent.as_deref();
            while let Some(key) = parent {
                if !seen.insert(key) {
                    return Err("cyclic semantic tree".into());
                }
                parent = self
                    .nodes
                    .get(key)
                    .ok_or("missing semantic parent")?
                    .parent
                    .as_deref();
            }
            for (position, maximum) in node.scroll_pixels.iter().zip(node.scroll_max_pixels) {
                if !position.is_finite()
                    || !maximum.is_finite()
                    || *position < 0.0
                    || maximum < 0.0
                    || *position > maximum + 1.0
                {
                    return Err("invalid semantic scroll".into());
                }
            }
            if let Some([x, y, w, h]) = node.rect_pixels {
                #[allow(clippy::cast_precision_loss)]
                let size = [self.client_pixels[0] as f32, self.client_pixels[1] as f32];
                if [x, y, w, h].iter().any(|n| !n.is_finite() || *n < 0.0)
                    || x + w > size[0] + 1.0
                    || y + h > size[1] + 1.0
                    || (node.visible && (w <= 0.0 || h <= 0.0))
                {
                    return Err("invalid clipped rectangle".into());
                }
            } else if node.visible {
                return Err("visible node lacks rectangle".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Capture,
    Finish,
}

/// Only observation/capture requests are accepted; unknown fields reject input RPCs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub nonce: String,
    pub seq: u64,
    pub op: Operation,
    pub after_frame: u64,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Reply {
    version: u32,
    nonce: String,
    seq: u64,
    status: &'static str,
    error: Option<String>,
    state: Option<State>,
}
impl Reply {
    fn error(nonce: &str, seq: u64, error: &str) -> Self {
        Self {
            version: 1,
            nonce: nonce.into(),
            seq,
            status: "error",
            error: Some(error.chars().take(256).collect()),
            state: None,
        }
    }
    fn ok(seq: u64, state: State) -> Self {
        Self {
            version: 1,
            nonce: state.nonce.clone(),
            seq,
            status: "ok",
            error: None,
            state: Some(state),
        }
    }
}

/// Exact state associated with a native screenshot command issued this frame.
/// Return pixels from THAT screenshot event, not a new snapshot taken at delivery.
#[derive(Debug)]
pub struct CaptureRequest {
    pub seq: u64,
    pub state: State,
}

#[derive(Debug)]
struct ScreenshotMarker {
    nonce: String,
    seq: u64,
    frame: u64,
    revision: u64,
    window: u64,
    viewport: eframe::egui::ViewportId,
}

impl CaptureRequest {
    /// Issue only at the end of the frame represented by `state`. The host must not
    /// defer issuing this command to another rendered frame. egui returns this private
    /// marker with the screenshot event; unrelated screenshot events cannot match it.
    #[must_use]
    pub fn screenshot_command(
        &self,
        viewport: eframe::egui::ViewportId,
    ) -> eframe::egui::ViewportCommand {
        eframe::egui::ViewportCommand::Screenshot(eframe::egui::UserData::new(ScreenshotMarker {
            nonce: self.state.nonce.clone(),
            seq: self.seq,
            frame: self.state.frame,
            revision: self.state.revision,
            window: self.state.window_id,
            viewport,
        }))
    }
}

/// Pure protocol sequencing. Rejects duplicate/skipped sequence and stale revisions.
pub struct Protocol {
    nonce: String,
    sequence: u64,
    pending: Option<Request>,
    previous: Option<State>,
}
impl Protocol {
    #[must_use]
    pub const fn new(nonce: String) -> Self {
        Self {
            nonce,
            sequence: 0,
            pending: None,
            previous: None,
        }
    }
    fn accept(&mut self, request: Request, state: &State) -> Result<(), String> {
        if request.version != 1 || request.nonce != self.nonce {
            return Err("request provenance mismatch".into());
        }
        if self.pending.is_some() || request.seq != self.sequence + 1 || request.seq > MAX_SEQUENCE
        {
            return Err("invalid request sequence or request busy".into());
        }
        // Consume correctly sequenced stale requests so the client can diagnose and
        // continue with the next sequence; replay never creates a second capture.
        self.sequence = request.seq;
        if request.expected_revision != state.revision || request.after_frame > state.frame {
            return Err("stale request revision/frame".into());
        }
        self.pending = Some(request);
        Ok(())
    }
    fn observe(&mut self, state: State) -> Option<(Operation, CaptureRequest)> {
        let stable = self
            .previous
            .as_ref()
            .is_some_and(|old| state.frame > old.frame && state.stable_with(old));
        self.previous = Some(state.clone());
        let request = self.pending.as_ref()?;
        if request.expected_revision != state.revision {
            return None;
        }
        if !stable || state.frame <= request.after_frame {
            return None;
        }
        let request = self.pending.take()?;
        Some((
            request.op,
            CaptureRequest {
                seq: request.seq,
                state,
            },
        ))
    }
}

enum Write {
    Reply(Reply),
    Capture {
        request: CaptureRequest,
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
    },
}
enum Event {
    Request(Request),
    Error(String),
    Written { seq: u64, finish: bool },
}

/// UI-facing bounded producer. One capture may be outstanding; no worker joins or
/// filesystem operations run in `poll`/`complete_screenshot`. Drop requests worker shutdown.
pub struct Interaction {
    nonce: String,
    protocol: Protocol,
    state: Arc<Mutex<Option<State>>>,
    writes: flume::Sender<Write>,
    events: flume::Receiver<Event>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    awaiting: Option<u64>,
    capture_state: Option<State>,
    capture_submitted: bool,
    finish_sequence: Option<u64>,
    finished: bool,
    last_provenance: Option<(u64, u64, u64)>,
}
impl Interaction {
    /// Opt in only when both environment values are present. Startup path checks run
    /// on the worker; failures surface through `poll`. Directory must be fresh, owned
    /// by this UID and private, with no symlink components. Linux only, fail closed.
    /// Descriptor-relative output survives ancestor renames; atomic rename never
    /// follows a replaced output symlink. This is not protection against malicious
    /// code running as the same UID that can modify the private directory itself.
    ///
    /// # Errors
    /// * Invalid environment, nonce, unsupported platform, or worker spawn failure.
    pub fn from_env(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Option<Self>, String> {
        let directory = std::env::var_os("HYPERCHAD_EGUI_INTERACTION_DIR");
        let nonce = std::env::var_os("HYPERCHAD_EGUI_INTERACTION_NONCE");
        match (directory, nonce) {
            (None, None) => Ok(None),
            (Some(directory), Some(nonce)) => {
                let nonce = nonce
                    .into_string()
                    .map_err(|_| "invalid interaction nonce")?;
                if nonce.len() != 64 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("interaction nonce must be 64 hexadecimal characters".into());
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = (directory, wake);
                    Err("interaction protocol requires Linux directory pinning".into())
                }
                #[cfg(target_os = "linux")]
                {
                    let state = Arc::new(Mutex::new(None));
                    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let (writes, incoming) = flume::bounded(2);
                    let (outgoing, events) = flume::bounded(4);
                    let worker_state = state.clone();
                    let worker_stop = stop.clone();
                    std::thread::Builder::new()
                        .name("egui-interaction-io".into())
                        .spawn(move || {
                            let result = io::Directory::open(std::path::Path::new(&directory))
                                .and_then(|directory| {
                                    worker(
                                        &directory,
                                        &worker_state,
                                        &incoming,
                                        &outgoing,
                                        &worker_stop,
                                        &wake,
                                    )
                                });
                            if let Err(error) = result {
                                let _ = outgoing.try_send(Event::Error(error));
                                wake();
                            }
                        })
                        .map_err(|_| "cannot spawn interaction I/O worker")?;
                    Ok(Some(Self {
                        protocol: Protocol::new(nonce.clone()),
                        nonce,
                        state,
                        writes,
                        events,
                        stop,
                        awaiting: None,
                        capture_state: None,
                        capture_submitted: false,
                        finish_sequence: None,
                        finished: false,
                        last_provenance: None,
                    }))
                }
            }
            _ => Err("both interaction directory and nonce are required".into()),
        }
    }
    /// Observe one actually rendered frame and poll bounded worker messages. The host
    /// must request another frame while a capture/finish handshake is outstanding.
    ///
    /// # Errors
    /// * Invalid/oversized snapshot, worker/path failure, or capture acknowledgement
    ///   failure. Such errors must fail the host interaction run, not be ignored.
    pub fn poll(&mut self, mut state: State) -> Result<Option<CaptureRequest>, String> {
        state.version = 1;
        state.nonce.clone_from(&self.nonce);
        state.pid = std::process::id();
        state.validate()?;
        if self
            .last_provenance
            .is_some_and(|(window, frame, revision)| {
                state.window_id != window || state.frame < frame || state.revision < revision
            })
        {
            return Err("interaction window/frame/revision provenance regressed".into());
        }
        self.last_provenance = Some((state.window_id, state.frame, state.revision));
        match self.state.try_lock() {
            Ok(mut pending) => *pending = Some(state.clone()),
            // Latest-state publication may skip a contended frame, never block UI.
            Err(std::sync::TryLockError::WouldBlock) => {}
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("interaction snapshot lock poisoned".into());
            }
        }
        for _ in 0..4 {
            match self.events.try_recv() {
                Ok(Event::Error(error)) => return Err(error),
                Ok(Event::Written { seq, finish }) => {
                    if self.awaiting == Some(seq) {
                        self.awaiting = None;
                        self.capture_state = None;
                        self.capture_submitted = false;
                    }
                    if finish && self.finish_sequence == Some(seq) {
                        self.finished = true;
                    }
                }
                Ok(Event::Request(request)) => {
                    let seq = request.seq;
                    if self.awaiting.is_some() || self.finished {
                        self.send(Write::Reply(Reply::error(
                            &self.nonce,
                            seq,
                            "interaction busy or finished",
                        )))?;
                    } else if let Err(error) = self.protocol.accept(request, &state) {
                        self.send(Write::Reply(Reply::error(&self.nonce, seq, &error)))?;
                    }
                }
                Err(flume::TryRecvError::Empty) => break,
                Err(flume::TryRecvError::Disconnected) => {
                    return Err("interaction I/O worker disconnected".into());
                }
            }
        }
        if self
            .protocol
            .pending
            .as_ref()
            .is_some_and(|request| request.expected_revision != state.revision)
            && let Some(request) = self.protocol.pending.take()
        {
            self.send(Write::Reply(Reply::error(
                &self.nonce,
                request.seq,
                "revision changed while awaiting stable frame",
            )))?;
        }
        if self.awaiting.is_some() {
            return Ok(None);
        }
        if let Some((operation, request)) = self.protocol.observe(state) {
            self.awaiting = Some(request.seq);
            match operation {
                Operation::Capture => {
                    self.capture_state = Some(request.state.clone());
                    self.capture_submitted = false;
                    return Ok(Some(request));
                }
                Operation::Finish => {
                    self.finish_sequence = Some(request.seq);
                    self.send(Write::Reply(Reply::ok(request.seq, request.state)))?;
                }
            }
        }
        Ok(None)
    }
    /// Match an actual egui screenshot response to the issued request marker. Returns
    /// false for unrelated screenshot/input events, without consuming the request.
    /// The host must call `request.screenshot_command` during the snapshot's frame.
    /// A later delivery event is expected; its private marker identifies the source
    /// frame rather than falsely substituting the delivery frame's state.
    ///
    /// # Errors
    /// * Matching responses with changed viewport, dimensions, outstanding authority,
    ///   excessive pixels, or a full/stopped worker queue reject the interaction run.
    pub fn complete_screenshot(
        &mut self,
        request: &CaptureRequest,
        event: &eframe::egui::Event,
    ) -> Result<bool, String> {
        let eframe::egui::Event::Screenshot {
            viewport_id,
            user_data,
            image,
        } = event
        else {
            return Ok(false);
        };
        let Some(marker) = user_data
            .data
            .as_ref()
            .and_then(|data| data.downcast_ref::<ScreenshotMarker>())
        else {
            return Ok(false);
        };
        if marker.nonce != request.state.nonce
            || marker.seq != request.seq
            || marker.frame != request.state.frame
            || marker.revision != request.state.revision
            || marker.window != request.state.window_id
        {
            return Ok(false);
        }
        if marker.viewport != *viewport_id {
            return Err("native screenshot viewport mismatch".into());
        }
        let width =
            u32::try_from(image.size[0]).map_err(|_| "native screenshot width exceeds budget")?;
        let height =
            u32::try_from(image.size[1]).map_err(|_| "native screenshot height exceeds budget")?;
        if request.state.client_pixels != [width, height]
            || image.pixels.len() > MAX_PIXELS
            || u64::from(width) * u64::from(height) != image.pixels.len() as u64
        {
            return Err("native screenshot dimensions mismatch".into());
        }
        let rgba: Arc<[u8]> = image
            .pixels
            .iter()
            .flat_map(eframe::egui::Color32::to_array)
            .collect::<Vec<_>>()
            .into();
        self.complete_capture(
            CaptureRequest {
                seq: request.seq,
                state: request.state.clone(),
            },
            width,
            height,
            rgba,
        )?;
        Ok(true)
    }

    /// Deliver verified screenshot pixels to the worker. Only `complete_screenshot`
    /// exposes production completion; raw pixels have no public admission API.
    fn complete_capture(
        &mut self,
        request: CaptureRequest,
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
    ) -> Result<(), String> {
        let count = u64::from(width) * u64::from(height);
        if self.awaiting != Some(request.seq)
            || self.finish_sequence == Some(request.seq)
            || self.capture_submitted
            || self.capture_state.as_ref() != Some(&request.state)
            || request.state.client_pixels != [width, height]
            || count > MAX_PIXELS as u64
            || count * 4 != rgba.len() as u64
        {
            return Err("invalid native capture delivery".into());
        }
        self.send(Write::Capture {
            request,
            width,
            height,
            rgba,
        })?;
        self.capture_submitted = true;
        Ok(())
    }
    fn send(&self, write: Write) -> Result<(), String> {
        self.writes
            .try_send(write)
            .map_err(|_| "interaction write queue full/disconnected".into())
    }
    /// Whether the host should schedule another frame to advance a handshake.
    /// Incoming requests independently wake the host through the worker callback.
    #[must_use]
    pub const fn needs_frame(&self) -> bool {
        self.protocol.pending.is_some() || self.awaiting.is_some()
    }

    /// Returns true only after the worker has atomically published a finish reply.
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.finished
    }
}
impl Drop for Interaction {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(target_os = "linux")]
fn worker(
    directory: &io::Directory,
    state: &Arc<Mutex<Option<State>>>,
    writes: &flume::Receiver<Write>,
    events: &flume::Sender<Event>,
    stop: &std::sync::atomic::AtomicBool,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut last_request = None;
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        let snapshot = state
            .lock()
            .map_err(|_| "interaction snapshot lock poisoned")?
            .take();
        if let Some(snapshot) = snapshot {
            directory.json("state.json", &snapshot)?;
        }
        for _ in 0..2 {
            let Ok(write) = writes.try_recv() else {
                break;
            };
            let (reply, finish) = match write {
                Write::Reply(reply) => {
                    let finish = reply.status == "ok";
                    (reply, finish)
                }
                Write::Capture {
                    request,
                    width,
                    height,
                    rgba,
                } => {
                    use image::ImageEncoder;
                    let mut png = Vec::new();
                    image::codecs::png::PngEncoder::new(&mut png)
                        .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
                        .map_err(|_| "native capture encoding failed")?;
                    if png.len() > 64 * 1024 * 1024 {
                        return Err("native capture PNG exceeds budget".into());
                    }
                    directory.atomic(&format!("capture-{:04}.png", request.seq), &png, false)?;
                    (Reply::ok(request.seq, request.state), false)
                }
            };
            if reply.seq == 0 || reply.seq > MAX_SEQUENCE {
                return Err("invalid reply sequence".into());
            }
            directory.json_new(&format!("reply-{:04}.json", reply.seq), &reply)?;
            events
                .try_send(Event::Written {
                    seq: reply.seq,
                    finish,
                })
                .map_err(|_| "interaction event queue full")?;
            wake();
        }
        if let Some(request_file) = directory.request()?
            && last_request.as_ref() != Some(&request_file)
        {
            let request = serde_json::from_slice::<Request>(&request_file.bytes)
                .map_err(|_| "malformed interaction request")?;
            last_request = Some(request_file);
            events
                .try_send(Event::Request(request))
                .map_err(|_| "interaction event queue full")?;
            wake();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

/// Linux descriptor-relative access without unsafe code or caller-controlled filenames.
#[cfg(target_os = "linux")]
mod io {
    use super::{MAX_JSON, MAX_REQUEST};
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Write},
        os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
        path::{Component, Path, PathBuf},
    };
    const NOFOLLOW: i32 = 0x20000;
    const DIRECTORY: i32 = 0x10000;
    #[derive(PartialEq, Eq)]
    pub(super) struct RequestFile {
        pub(super) bytes: Vec<u8>,
        inode: u64,
        modified: std::time::SystemTime,
    }
    pub(super) struct Directory {
        file: File,
    }
    impl Directory {
        pub(super) fn open(path: &Path) -> Result<Self, String> {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
            {
                return Err("interaction directory must be absolute without traversal".into());
            }
            // Start from the filesystem root and pin each component before opening
            // the next through /proc/self/fd. Swapped ancestors cannot redirect I/O.
            let mut file = File::open("/").map_err(|_| "cannot open root directory")?;
            for component in path.components() {
                if let Component::Normal(name) = component {
                    let next =
                        PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd())).join(name);
                    file = OpenOptions::new()
                        .read(true)
                        .custom_flags(NOFOLLOW | DIRECTORY)
                        .open(next)
                        .map_err(
                            |_| "interaction directory component is inaccessible or a symlink",
                        )?;
                }
            }
            let metadata = file
                .metadata()
                .map_err(|_| "cannot inspect interaction directory")?;
            let uid = std::fs::metadata("/proc/self")
                .map_err(|_| "cannot inspect process owner")?
                .uid();
            if metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
                return Err("interaction directory must be owned and private".into());
            }
            let result = Self { file };
            if std::fs::read_dir(result.path())
                .map_err(|_| "cannot inspect interaction directory")?
                .next()
                .is_some()
            {
                return Err("interaction directory must be fresh and empty".into());
            }
            Ok(result)
        }
        fn path(&self) -> PathBuf {
            PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()))
        }
        pub(super) fn request(&self) -> Result<Option<RequestFile>, String> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(NOFOLLOW | 0x800)
                .open(self.path().join("request.json"));
            let file = match file {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("cannot open interaction request safely".into()),
            };
            let metadata = file.metadata().map_err(|_| "cannot inspect request")?;
            if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_REQUEST as u64 {
                return Err("unsafe/oversized interaction request".into());
            }
            let mut bytes = Vec::new();
            file.take(MAX_REQUEST as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read interaction request")?;
            if bytes.len() > MAX_REQUEST {
                return Err("interaction request exceeds budget".into());
            }
            Ok(Some(RequestFile {
                bytes,
                inode: metadata.ino(),
                modified: metadata
                    .modified()
                    .map_err(|_| "cannot inspect request timestamp")?,
            }))
        }
        pub(super) fn json(&self, name: &str, value: &impl serde::Serialize) -> Result<(), String> {
            self.serialized(name, value, true)
        }
        pub(super) fn json_new(
            &self,
            name: &str,
            value: &impl serde::Serialize,
        ) -> Result<(), String> {
            self.serialized(name, value, false)
        }
        fn serialized(
            &self,
            name: &str,
            value: &impl serde::Serialize,
            replace: bool,
        ) -> Result<(), String> {
            let bytes = serde_json::to_vec(value).map_err(|_| "cannot encode interaction JSON")?;
            if bytes.len() > MAX_JSON {
                return Err("interaction JSON exceeds budget".into());
            }
            self.atomic(name, &bytes, replace)
        }
        pub(super) fn atomic(&self, name: &str, bytes: &[u8], replace: bool) -> Result<(), String> {
            if name.contains('/') || name.starts_with('.') {
                return Err("invalid internal interaction filename".into());
            }
            let target = self.path().join(name);
            if let Ok(metadata) = std::fs::symlink_metadata(&target)
                && (!replace
                    || !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.nlink() != 1)
            {
                return Err("interaction output already exists or is unsafe".into());
            }
            let temporary = self.path().join(format!(".{name}.tmp"));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(NOFOLLOW)
                .open(&temporary)
                .map_err(|_| "cannot create interaction temporary file")?;
            file.write_all(bytes)
                .map_err(|_| "cannot write interaction artifact")?;
            file.sync_all()
                .map_err(|_| "cannot sync interaction artifact")?;
            if replace {
                std::fs::rename(&temporary, &target)
                    .map_err(|_| "cannot publish interaction artifact")?;
            } else {
                // Atomic no-clobber publication. Existing/symlink targets reject.
                std::fs::hard_link(&temporary, &target)
                    .map_err(|_| "cannot publish fresh interaction artifact")?;
                std::fs::remove_file(&temporary)
                    .map_err(|_| "cannot retire interaction temporary file")?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(frame: u64) -> State {
        State {
            version: 1,
            nonce: "a".repeat(64),
            pid: 1,
            window_id: 42,
            revision: 7,
            frame,
            route: "/test".into(),
            ready: true,
            diagnostics: Vec::new(),
            pending: Pending::default(),
            client_pixels: [2, 2],
            focused_key: Some("epoch:1/node:1/generation:0".into()),
            nodes: BTreeMap::from([(
                "epoch:1/node:1/generation:0".into(),
                Node {
                    generation: 0,
                    parent: None,
                    id: Some("root".into()),
                    tag: "div".into(),
                    name: None,
                    href: None,
                    text: "safe text".into(),
                    visible: true,
                    enabled: true,
                    rect_pixels: Some([0.0, 0.0, 2.0, 2.0]),
                    value: ExportValue::default(),
                    scroll_pixels: [0.0, 0.0],
                    scroll_max_pixels: [0.0, 0.0],
                },
            )]),
        }
    }
    fn request(op: Operation, seq: u64) -> Request {
        Request {
            version: 1,
            nonce: "a".repeat(64),
            seq,
            op,
            after_frame: 10,
            expected_revision: 7,
        }
    }
    fn harness() -> (Interaction, flume::Receiver<Write>, flume::Sender<Event>) {
        let (writes, receiver) = flume::bounded(2);
        let (sender, events) = flume::bounded(4);
        (
            Interaction {
                nonce: "a".repeat(64),
                protocol: Protocol::new("a".repeat(64)),
                state: Arc::new(Mutex::new(None)),
                writes,
                events,
                stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                awaiting: None,
                capture_state: None,
                capture_submitted: false,
                finish_sequence: None,
                finished: false,
                last_provenance: None,
            },
            receiver,
            sender,
        )
    }

    #[test]
    fn given_secret_control_when_exporting_then_reader_is_never_called() {
        let value = ExportValue::read(true, || panic!("secret value read"));
        assert_eq!(serde_json::to_string(&value).unwrap(), "null");
        let value = ExportValue::read(false, || Some("ordinary".into()));
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"ordinary\"");
    }

    #[test]
    fn given_capture_when_later_stable_frame_then_exact_state_is_returned_once() {
        let mut protocol = Protocol::new("a".repeat(64));
        assert!(protocol.observe(state(10)).is_none());
        protocol
            .accept(request(Operation::Capture, 1), &state(10))
            .unwrap();
        assert!(protocol.observe(state(10)).is_none());
        let (op, capture) = protocol.observe(state(11)).unwrap();
        assert_eq!(op, Operation::Capture);
        assert_eq!(capture.state, state(11));
        assert!(protocol.observe(state(12)).is_none());
        assert!(
            protocol
                .accept(request(Operation::Capture, 1), &state(12))
                .is_err()
        );
        assert!(
            protocol
                .accept(request(Operation::Capture, 3), &state(12))
                .is_err()
        );
    }

    #[test]
    fn given_stale_or_foreign_request_then_no_capture_authority_is_created() {
        let mut protocol = Protocol::new("a".repeat(64));
        let mut foreign = request(Operation::Capture, 1);
        foreign.nonce = "b".repeat(64);
        assert!(protocol.accept(foreign, &state(10)).is_err());
        let mut stale = request(Operation::Capture, 1);
        stale.expected_revision = 6;
        assert!(protocol.accept(stale, &state(10)).is_err());
        assert!(protocol.pending.is_none());
        protocol
            .accept(request(Operation::Finish, 2), &state(10))
            .unwrap();
        assert!(
            protocol
                .accept(request(Operation::Capture, 3), &state(10))
                .is_err()
        );
    }

    #[test]
    fn given_diagnostics_pending_work_or_movement_then_capture_waits() {
        let mut protocol = Protocol::new("a".repeat(64));
        protocol.observe(state(10));
        protocol
            .accept(request(Operation::Capture, 1), &state(10))
            .unwrap();
        let mut busy = state(11);
        busy.pending.routes = 1;
        assert!(protocol.observe(busy).is_none());
        let mut diagnostic = state(12);
        diagnostic.diagnostics.push("safe error".into());
        assert!(protocol.observe(diagnostic).is_none());
        let mut moving = state(13);
        moving.nodes.values_mut().next().unwrap().text = "changed".into();
        assert!(protocol.observe(moving).is_none());
        assert!(protocol.observe(state(14)).is_none());
        assert_eq!(protocol.observe(state(15)).unwrap().1.state.frame, 15);
    }

    #[test]
    fn given_snapshot_budget_or_cycle_then_validation_fails_closed() {
        let mut snapshot = state(10);
        snapshot.validate().unwrap();
        snapshot.nodes.values_mut().next().unwrap().parent = snapshot.focused_key.clone();
        assert!(snapshot.validate().is_err());
        let mut snapshot = state(10);
        snapshot.nodes.values_mut().next().unwrap().text = "x".repeat(MAX_STRING + 1);
        assert!(snapshot.validate().is_err());
        let mut snapshot = state(10);
        snapshot.nodes.values_mut().next().unwrap().rect_pixels = Some([0.0, 0.0, f32::NAN, 2.0]);
        assert!(snapshot.validate().is_err());
        assert!(serde_json::from_str::<Request>(r#"{"version":1,"nonce":"a","seq":1,"op":"capture","after_frame":0,"expected_revision":0,"path":"/tmp/escape"}"#).is_err());
    }

    #[test]
    fn given_changed_revision_or_window_then_pending_capture_cannot_succeed() {
        let (mut interaction, writes, events) = harness();
        interaction.poll(state(10)).unwrap();
        events
            .send(Event::Request(request(Operation::Capture, 1)))
            .unwrap();
        let mut changed = state(11);
        changed.revision = 8;
        assert!(interaction.poll(changed).unwrap().is_none());
        assert!(matches!(
            writes.recv().unwrap(),
            Write::Reply(Reply {
                status: "error",
                ..
            })
        ));
        assert!(!interaction.needs_frame());
        let mut changed_window = state(12);
        changed_window.revision = 8;
        changed_window.window_id = 43;
        assert!(interaction.poll(changed_window).is_err());
    }

    #[test]
    fn given_busy_snapshot_lock_then_ui_poll_does_not_wait_for_worker_io() {
        let (mut interaction, _writes, _events) = harness();
        let pending = interaction.state.clone();
        let _guard = pending.lock().unwrap();
        assert!(interaction.poll(state(10)).unwrap().is_none());
    }

    #[test]
    fn given_finish_then_finished_requires_successful_write_ack() {
        let (mut interaction, writes, events) = harness();
        interaction.poll(state(10)).unwrap();
        events
            .send(Event::Request(request(Operation::Finish, 1)))
            .unwrap();
        assert!(interaction.poll(state(11)).unwrap().is_none());
        assert!(interaction.needs_frame());
        assert!(!interaction.finished());
        let Write::Reply(reply) = writes.recv().unwrap() else {
            panic!("expected finish reply");
        };
        assert_eq!(reply.status, "ok");
        assert_eq!(reply.state.unwrap().frame, 11);
        events
            .send(Event::Written {
                seq: 1,
                finish: true,
            })
            .unwrap();
        interaction.poll(state(12)).unwrap();
        assert!(interaction.finished());
        assert!(!interaction.needs_frame());
    }

    #[test]
    fn given_capture_then_wrong_frame_duplicate_delivery_and_queue_overflow_reject() {
        let (mut interaction, writes, events) = harness();
        interaction.poll(state(10)).unwrap();
        events
            .send(Event::Request(request(Operation::Capture, 1)))
            .unwrap();
        let capture = interaction.poll(state(11)).unwrap().unwrap();
        let pixels: Arc<[u8]> = vec![255; 16].into();
        let invalid = CaptureRequest {
            seq: capture.seq,
            state: state(12),
        };
        assert!(
            interaction
                .complete_capture(invalid, 2, 2, pixels.clone())
                .is_err()
        );
        let duplicate = CaptureRequest {
            seq: capture.seq,
            state: capture.state.clone(),
        };
        interaction
            .complete_capture(capture, 2, 2, pixels.clone())
            .unwrap();
        assert!(
            interaction
                .complete_capture(duplicate, 2, 2, pixels)
                .is_err()
        );
        assert!(matches!(writes.recv().unwrap(), Write::Capture { .. }));
        interaction
            .send(Write::Reply(Reply::error("a", 2, "error")))
            .unwrap();
        interaction
            .send(Write::Reply(Reply::error("a", 3, "error")))
            .unwrap();
        assert!(
            interaction
                .send(Write::Reply(Reply::error("a", 4, "error")))
                .is_err()
        );
    }

    #[test]
    fn given_unrelated_or_wrong_viewport_screenshot_then_capture_state_is_not_mislabelled() {
        use eframe::egui::{Color32, ColorImage, Event, UserData, ViewportCommand, ViewportId};
        let (mut interaction, writes, events) = harness();
        interaction.poll(state(10)).unwrap();
        events
            .send(super::Event::Request(request(Operation::Capture, 1)))
            .unwrap();
        let capture = interaction.poll(state(11)).unwrap().unwrap();
        let image = Arc::new(ColorImage::new([2, 2], vec![Color32::WHITE; 4]));
        let unrelated = Event::Screenshot {
            viewport_id: ViewportId::ROOT,
            user_data: UserData::default(),
            image: image.clone(),
        };
        assert!(
            !interaction
                .complete_screenshot(&capture, &unrelated)
                .unwrap()
        );
        let ViewportCommand::Screenshot(marker) = capture.screenshot_command(ViewportId::ROOT)
        else {
            panic!("expected screenshot command");
        };
        let wrong_viewport = Event::Screenshot {
            viewport_id: ViewportId::from_hash_of("other"),
            user_data: marker.clone(),
            image: image.clone(),
        };
        assert!(
            interaction
                .complete_screenshot(&capture, &wrong_viewport)
                .is_err()
        );
        // Delivery happens in a later frame; only the marker's requested frame is
        // eligible for the reply. A fresh screenshot of frame 12 cannot stand in.
        interaction.poll(state(12)).unwrap();
        let matching = Event::Screenshot {
            viewport_id: ViewportId::ROOT,
            user_data: marker,
            image,
        };
        assert!(
            interaction
                .complete_screenshot(&capture, &matching)
                .unwrap()
        );
        let Write::Capture { request, .. } = writes.recv().unwrap() else {
            panic!("expected captured frame");
        };
        assert_eq!(request.state.frame, 11);
        assert!(
            interaction
                .complete_screenshot(&capture, &matching)
                .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    mod filesystem {
        use super::*;
        use std::{
            os::unix::fs::{PermissionsExt, symlink},
            path::PathBuf,
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        struct Temporary(PathBuf);
        impl Temporary {
            fn new() -> Self {
                let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = PathBuf::from(format!(
                    "/tmp/hyperchad-interaction-{}-{sequence}",
                    std::process::id()
                ));
                std::fs::create_dir(&path).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
                Self(path)
            }
        }
        impl Drop for Temporary {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn given_symlink_reused_or_public_directory_then_startup_rejects() {
            let temporary = Temporary::new();
            let actual = temporary.0.join("actual");
            std::fs::create_dir(&actual).unwrap();
            std::fs::set_permissions(&actual, std::fs::Permissions::from_mode(0o700)).unwrap();
            let alias = temporary.0.join("alias");
            symlink(&actual, &alias).unwrap();
            assert!(io::Directory::open(&alias).is_err());
            assert!(io::Directory::open(&temporary.0).is_err());
            std::fs::set_permissions(&actual, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(io::Directory::open(&actual).is_err());
        }
        #[test]
        fn given_pinned_directory_then_atomic_files_do_not_follow_request_or_output_symlinks() {
            let temporary = Temporary::new();
            let directory = io::Directory::open(&temporary.0).unwrap();
            directory.json("state.json", &state(10)).unwrap();
            directory.json("state.json", &state(11)).unwrap();
            let actual: serde_json::Value =
                serde_json::from_slice(&std::fs::read(temporary.0.join("state.json")).unwrap())
                    .unwrap();
            assert_eq!(actual["frame"], 11);
            assert!(!temporary.0.join(".state.json.tmp").exists());
            symlink("state.json", temporary.0.join("request.json")).unwrap();
            assert!(directory.request().is_err());
            symlink("state.json", temporary.0.join("reply-0001.json")).unwrap();
            assert!(directory.json_new("reply-0001.json", &state(12)).is_err());
            assert!(directory.atomic("../escape", b"", false).is_err());
        }
        #[test]
        fn given_directory_path_replaced_after_open_then_writes_stay_on_pinned_directory() {
            let temporary = Temporary::new();
            let original = temporary.0.join("original");
            std::fs::create_dir(&original).unwrap();
            std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).unwrap();
            let directory = io::Directory::open(&original).unwrap();
            let pinned = temporary.0.join("pinned");
            std::fs::rename(&original, &pinned).unwrap();
            std::fs::create_dir(&original).unwrap();
            directory.json("state.json", &state(10)).unwrap();
            assert!(pinned.join("state.json").is_file());
            assert!(!original.join("state.json").exists());
        }

        #[test]
        fn given_worker_capture_then_png_precedes_reply_with_exact_frame_state() {
            let temporary = Temporary::new();
            let directory = io::Directory::open(&temporary.0).unwrap();
            let snapshots = Arc::new(Mutex::new(Some(state(10))));
            let (send, writes) = flume::bounded(2);
            let (events, receive) = flume::bounded(4);
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_stop = stop.clone();
            let thread = std::thread::spawn(move || {
                let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
                worker(
                    &directory,
                    &snapshots,
                    &writes,
                    &events,
                    &worker_stop,
                    &wake,
                )
            });
            send.send(Write::Capture {
                request: CaptureRequest {
                    seq: 1,
                    state: state(11),
                },
                width: 2,
                height: 2,
                rgba: vec![255; 16].into(),
            })
            .unwrap();
            assert!(matches!(
                receive.recv_timeout(Duration::from_secs(5)).unwrap(),
                Event::Written {
                    seq: 1,
                    finish: false
                }
            ));
            let png = std::fs::read(temporary.0.join("capture-0001.png")).unwrap();
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            let reply: serde_json::Value = serde_json::from_slice(
                &std::fs::read(temporary.0.join("reply-0001.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(reply["state"]["frame"], 11);
            assert_eq!(reply["status"], "ok");
            stop.store(true, std::sync::atomic::Ordering::Release);
            thread.join().unwrap().unwrap();
        }
    }
}
