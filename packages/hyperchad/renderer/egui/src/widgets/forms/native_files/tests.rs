use super::*;
use crate::{
    document::{Document, ViewOrigin},
    widgets::WidgetAdapter,
};
use hyperchad_renderer::View;
use hyperchad_transformer::{Container, Element, Input};
use std::time::{Duration, Instant};

fn fixture() -> (Document, FormsAdapter, FileRequest) {
    let mut document = Document::new();
    document
        .apply_view(
            View {
                primary: Some(Container {
                    id: 1,
                    element: Element::Input {
                        input: Input::File,
                        name: Some("upload".into()),
                        autofocus: None,
                    },
                    ..Container::default()
                }),
                ..View::default()
            },
            ViewOrigin::Navigation,
        )
        .unwrap();
    let source = document.stamp(document.root_keys()[0]).unwrap();
    let mut forms = FormsAdapter::new();
    forms.reconcile(&document, &[]).unwrap();
    let request = FileRequest {
        source,
        sequence: 1,
        multiple: false,
    };
    forms.fields.get_mut(&source.key).unwrap().files = vec![file("prior", &[7])];
    (document, forms, request)
}

fn file(name: &str, bytes: &[u8]) -> FormFile {
    FormFile {
        name: name.into(),
        media_type: None,
        bytes: Arc::from(bytes),
    }
}

fn queue(forms: &mut FormsAdapter, request: FileRequest) {
    forms.fields.get_mut(&request.source.key).unwrap().pending = Some(request);
    forms.file_requests.push(request);
}

fn finish(service: &mut NativeFileService, forms: &mut FormsAdapter) -> Vec<FileServiceError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut errors = Vec::new();
    while service.active.is_some() {
        errors.extend(service.poll(forms));
        assert!(Instant::now() < deadline, "file service did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
    errors
}

#[test]
fn concrete_worker_dispatch_poll_updates_and_forwards_once() {
    let (_document, mut forms, request) = fixture();
    let mut service = NativeFileService::with_picker(
        egui::Context::default(),
        FileLimits::default(),
        Box::new(|_, _| Ok(Some(vec![file("picked", &[0, 255])]))),
    );
    service.attach(&mut forms);
    assert!(forms.file_service);
    queue(&mut forms, request);
    assert!(service.dispatch(&mut forms).is_empty());
    assert!(finish(&mut service, &mut forms).is_empty());
    assert_eq!(forms.fields[&request.source.key].files[0].name, "picked");
    assert_eq!(
        &*forms.fields[&request.source.key].files[0].bytes,
        &[0, 255]
    );
    assert_eq!(WidgetAdapter::take_changes(&mut forms).len(), 1);
    assert!(WidgetAdapter::take_changes(&mut forms).is_empty());
}

#[test]
fn cancellation_read_failure_and_limit_failure_preserve_prior_selection() {
    for outcome in [
        Ok(None),
        Err(FileServiceError::ReadFailed),
        Ok(Some(vec![file("too large", &[1, 2])])),
    ] {
        let (_document, mut forms, request) = fixture();
        let mut outcome = Some(outcome);
        let mut service = NativeFileService::with_picker(
            egui::Context::default(),
            FileLimits {
                files: 1,
                bytes_per_file: 1,
                total_bytes: 1,
            },
            Box::new(move |_, _| outcome.take().unwrap()),
        );
        queue(&mut forms, request);
        assert!(service.dispatch(&mut forms).is_empty());
        let _ = finish(&mut service, &mut forms);
        assert_eq!(forms.fields[&request.source.key].files[0].name, "prior");
        assert_eq!(forms.pending_resources(), 0);
        assert!(WidgetAdapter::take_changes(&mut forms).is_empty());
    }
}

#[test]
fn reset_and_superseded_sequences_reject_late_worker_result() {
    let (_document, mut forms, request) = fixture();
    let (release, gate) = mpsc::sync_channel(1);
    let mut service = NativeFileService::with_picker(
        egui::Context::default(),
        FileLimits::default(),
        Box::new(move |_, _| {
            gate.recv().unwrap();
            Ok(Some(vec![file("late", &[1])]))
        }),
    );
    queue(&mut forms, request);
    assert!(service.dispatch(&mut forms).is_empty());
    forms.reset_control(request.source).unwrap();
    let newer = FileRequest {
        sequence: 2,
        ..request
    };
    queue(&mut forms, newer);
    assert_eq!(service.dispatch(&mut forms), [FileServiceError::Busy]);
    release.send(()).unwrap();
    assert!(finish(&mut service, &mut forms).is_empty());
    assert!(forms.fields[&request.source.key].files.is_empty());
    assert_eq!(forms.pending_resources(), 0);
    assert!(WidgetAdapter::take_changes(&mut forms).is_empty());
}

#[test]
fn bounded_reader_rejects_aggregate_size_and_reads_exact_bytes() {
    let path = std::env::temp_dir().join(format!(
        "hyperchad-file-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, [0_u8, 255, 4]).unwrap();
    let limits = FileLimits {
        files: 2,
        bytes_per_file: 3,
        total_bytes: 5,
    };
    let files = read_paths(std::slice::from_ref(&path), limits).unwrap();
    assert_eq!(&*files[0].bytes, &[0, 255, 4]);
    assert!(matches!(
        read_paths(&[path.clone(), path.clone()], limits),
        Err(FileServiceError::SelectionTooLarge)
    ));
    assert!(matches!(
        read_paths(
            std::slice::from_ref(&path),
            FileLimits {
                bytes_per_file: 2,
                ..limits
            }
        ),
        Err(FileServiceError::SelectionTooLarge)
    ));
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        read_paths(&[path], limits),
        Err(FileServiceError::ReadFailed)
    ));
}
