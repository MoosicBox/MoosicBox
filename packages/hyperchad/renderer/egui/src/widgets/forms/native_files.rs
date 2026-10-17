//! Native chooser and bounded file reads, never executed on the paint thread.

use std::{
    future::Future,
    io::Read,
    path::PathBuf,
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
};

use eframe::egui;

use super::{FileRequest, FormFile, FormsAdapter};

/// Resource bounds applied to each selection before it can replace existing files.
#[derive(Clone, Copy, Debug)]
pub struct FileLimits {
    pub files: usize,
    pub bytes_per_file: usize,
    pub total_bytes: usize,
}

impl Default for FileLimits {
    fn default() -> Self {
        Self {
            files: 32,
            bytes_per_file: 16 * 1024 * 1024,
            total_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Value-free diagnostics: no selected paths, filenames, bytes or OS error text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileServiceError {
    Busy,
    WorkerUnavailable,
    PickerFailed,
    ReadFailed,
    SelectionTooLarge,
    InvalidCompletion,
}

/// An injectable chooser/read operation. Runs only on the dedicated worker.
pub type FilePicker = Box<
    dyn FnMut(FileRequest, FileLimits) -> Result<Option<Vec<FormFile>>, FileServiceError> + Send,
>;

type Completion = (FileRequest, Result<Option<Vec<FormFile>>, FileServiceError>);

/// One worker, one in-flight selection, bounded request and completion channels.
/// Dropping the service never joins a chooser thread or blocks shutdown/paint.
pub struct NativeFileService {
    requests: mpsc::SyncSender<FileRequest>,
    completions: mpsc::Receiver<Completion>,
    active: Option<FileRequest>,
}

impl NativeFileService {
    /// Install the actual platform `rfd::AsyncFileDialog` picker.
    #[must_use]
    pub fn new(context: egui::Context) -> Self {
        Self::with_picker(context, FileLimits::default(), Box::new(native_pick))
    }

    /// Inject a deterministic chooser/read operation, retaining the same worker,
    /// bounded channels, completion validation and repaint behavior as production.
    #[must_use]
    pub fn with_picker(context: egui::Context, limits: FileLimits, mut picker: FilePicker) -> Self {
        let (requests, receiver) = mpsc::sync_channel::<FileRequest>(1);
        let (sender, completions) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("hyperchad-file-picker".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        picker(request, limits)
                    }))
                    .unwrap_or(Err(FileServiceError::PickerFailed))
                    .and_then(|files| {
                        if let Some(files) = &files {
                            validate_files(files, request, limits)?;
                        }
                        Ok(files)
                    });
                    if sender.send((request, outcome)).is_err() {
                        break;
                    }
                    context.request_repaint();
                }
            });
        // On spawn failure the moved receiver/sender are dropped; dispatch reports
        // WorkerUnavailable and settles the request without changing selection.
        drop(spawned);
        Self {
            requests,
            completions,
            active: None,
        }
    }

    /// Enable file controls only when this concrete service is owned by the host.
    pub const fn attach(&self, forms: &mut FormsAdapter) {
        forms.set_file_service_available(true);
    }

    /// Poll after document reconciliation, before painting. Never waits for I/O.
    #[must_use]
    pub fn poll(&mut self, forms: &mut FormsAdapter) -> Vec<FileServiceError> {
        let mut errors = Vec::new();
        loop {
            match self.completions.try_recv() {
                Ok(completion) => self.apply(forms, completion, &mut errors),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(request) = self.active.take() {
                        settle(forms, request);
                        errors.push(FileServiceError::WorkerUnavailable);
                    }
                    break;
                }
            }
        }
        errors
    }

    /// Dispatch after traversal. A busy picker rejects extra work explicitly,
    /// settling those requests while preserving the last successful selection.
    #[must_use]
    pub fn dispatch(&mut self, forms: &mut FormsAdapter) -> Vec<FileServiceError> {
        let mut errors = Vec::new();
        for request in forms.take_file_requests() {
            if !is_pending(forms, request) {
                continue;
            }
            if self.active.is_some() {
                settle(forms, request);
                errors.push(FileServiceError::Busy);
                continue;
            }
            match self.requests.try_send(request) {
                Ok(()) => self.active = Some(request),
                Err(error) => {
                    settle(forms, request);
                    errors.push(match error {
                        mpsc::TrySendError::Full(_) => FileServiceError::Busy,
                        mpsc::TrySendError::Disconnected(_) => FileServiceError::WorkerUnavailable,
                    });
                }
            }
        }
        errors
    }

    fn apply(
        &mut self,
        forms: &mut FormsAdapter,
        (request, outcome): Completion,
        errors: &mut Vec<FileServiceError>,
    ) {
        if self.active != Some(request) {
            return;
        }
        self.active = None;
        if !is_pending(forms, request) {
            return;
        }
        match outcome {
            Ok(files) => {
                if forms.complete_file(request, files).is_err() {
                    errors.push(FileServiceError::InvalidCompletion);
                }
            }
            Err(error) => {
                settle(forms, request);
                errors.push(error);
            }
        }
    }
}

fn is_pending(forms: &FormsAdapter, request: FileRequest) -> bool {
    forms
        .fields
        .get(&request.source.key)
        .is_some_and(|field| field.stamp == request.source && field.pending == Some(request))
}

fn settle(forms: &mut FormsAdapter, request: FileRequest) {
    // Cancellation cannot fail and retains prior selection.
    let _ = forms.complete_file(request, None);
}

fn native_pick(
    request: FileRequest,
    limits: FileLimits,
) -> Result<Option<Vec<FormFile>>, FileServiceError> {
    let dialog = rfd::AsyncFileDialog::new();
    let handles = if request.multiple {
        wait(dialog.pick_files())
    } else {
        wait(dialog.pick_file()).map(|file| vec![file])
    };
    let Some(handles) = handles else {
        return Ok(None);
    };
    if handles.len() > limits.files {
        return Err(FileServiceError::SelectionTooLarge);
    }
    let paths: Vec<PathBuf> = handles
        .into_iter()
        .map(|handle| handle.path().to_path_buf())
        .collect();
    read_paths(&paths, limits).map(Some)
}

fn validate_files(
    files: &[FormFile],
    request: FileRequest,
    limits: FileLimits,
) -> Result<(), FileServiceError> {
    if (!request.multiple && files.len() > 1) || files.len() > limits.files {
        return Err(FileServiceError::SelectionTooLarge);
    }
    let mut remaining = limits.total_bytes;
    for file in files {
        if file.bytes.len() > limits.bytes_per_file || file.bytes.len() > remaining {
            return Err(FileServiceError::SelectionTooLarge);
        }
        remaining -= file.bytes.len();
    }
    Ok(())
}

fn read_paths(paths: &[PathBuf], limits: FileLimits) -> Result<Vec<FormFile>, FileServiceError> {
    if paths.len() > limits.files {
        return Err(FileServiceError::SelectionTooLarge);
    }
    let mut files = Vec::with_capacity(paths.len());
    let mut remaining = limits.total_bytes;
    for path in paths {
        if !std::fs::metadata(path)
            .map_err(|_| FileServiceError::ReadFailed)?
            .is_file()
        {
            return Err(FileServiceError::ReadFailed);
        }
        let file = std::fs::File::open(path).map_err(|_| FileServiceError::ReadFailed)?;
        if !file
            .metadata()
            .map_err(|_| FileServiceError::ReadFailed)?
            .is_file()
        {
            return Err(FileServiceError::ReadFailed);
        }
        let bound = remaining.min(limits.bytes_per_file);
        let mut bytes = Vec::new();
        file.take(u64::try_from(bound).unwrap_or(u64::MAX).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| FileServiceError::ReadFailed)?;
        if bytes.len() > bound {
            return Err(FileServiceError::SelectionTooLarge);
        }
        remaining -= bytes.len();
        files.push(FormFile {
            name: path
                .file_name()
                .ok_or(FileServiceError::ReadFailed)?
                .to_string_lossy()
                .into_owned(),
            media_type: None,
            bytes: Arc::from(bytes),
        });
    }
    Ok(files)
}

// A tiny std-only executor for the dedicated chooser thread. Wake notifications
// unpark the worker; neither future polling nor file reads touch the paint thread.
struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn wait<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests;
