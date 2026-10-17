//! Transport only: producers enqueue owned commands; the UI thread drains and mutates.
//!
//! Queue admission is the ordering boundary, not async task start time. No facade
//! performs layout or waits for a frame. A wake hook can be installed after startup.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use flume::{Receiver, Sender};
use hyperchad_renderer::View;

use crate::canvas::CanvasCommand;
use crate::document::{
    Document, DocumentError, ElementStamp, MutationSummary, RequestTicket, ViewOrigin,
};

/// Commands already supported by the document, not placeholders for future subsystems.
#[derive(Debug)]
pub enum DocumentCommand {
    Apply {
        view: View,
        origin: ViewOrigin,
    },
    Complete {
        ticket: RequestTicket,
        view: View,
    },
    /// Applied synchronously against the document at this command's FIFO position.
    Canvas(CanvasCommand),
    /// Host-owned lifecycle/application boundary in the same admission FIFO.
    RouteCompletion(crate::routing::PreparedCompletion),
}

type WakeCallback = Arc<dyn Fn() + Send + Sync>;

/// Context-independent wake registration shared by command producers.
#[derive(Clone, Default)]
pub struct DocumentWake {
    callback: Arc<Mutex<Option<WakeCallback>>>,
}

impl DocumentWake {
    /// Publish the native/context-only host's repaint hook, then request an initial
    /// frame. This covers every command enqueued before context attachment. The hook
    /// runs outside the mutex and must be nonblocking (e.g. `request_repaint`).
    pub fn attach(&self, callback: impl Fn() + Send + Sync + 'static) {
        let callback: WakeCallback = Arc::new(callback);
        *self
            .callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(callback.clone());
        callback();
    }

    /// Disconnect the context hook. Dropping the inbox, not detaching the hook, closes
    /// command acceptance. A concurrent producer may still call a cloned old hook.
    pub fn detach(&self) {
        self.callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn notify(&self) {
        let callback = self
            .callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

/// Cheap shared producer. Commands accepted before attachment remain queued.
#[derive(Clone)]
pub struct DocumentHandle {
    sender: Sender<DocumentCommand>,
    wake: DocumentWake,
}

impl DocumentHandle {
    /// Enqueue without waiting for UI application, and only then wake the host.
    ///
    /// # Errors
    /// * `Full(command)` when capacity is exhausted; caller owns retry/rejection policy.
    /// * `Disconnected(command)` when the UI inbox has been dropped.
    // Preserve rejected command ownership for retry without changing the frozen API.
    #[allow(clippy::result_large_err)]
    pub fn submit(
        &self,
        command: DocumentCommand,
    ) -> Result<(), flume::TrySendError<DocumentCommand>> {
        self.sender.try_send(command)?;
        self.wake.notify();
        Ok(())
    }

    /// Await bounded admission without blocking the UI thread or an executor thread.
    ///
    /// Wake the host before waiting so a full queue can drain, then wake again after
    /// admission. Commands retain FIFO channel admission order; concurrent task start
    /// order does not reserve a position. No overflow queue is created. Hosts must
    /// bound concurrent producers as well if they need to bound waiting payloads.
    /// Dropping this future before admission cancels the send and drops its command;
    /// dropping it after admission does not retract the queued command.
    ///
    /// # Errors
    /// * Returns `SendError(command)` with the owned command when the inbox disconnects,
    ///   including when disconnection occurs while waiting for capacity.
    // Keep rejected command ownership consistent with submit's frozen transport API.
    #[allow(clippy::result_large_err)]
    pub async fn submit_async(
        &self,
        command: DocumentCommand,
    ) -> Result<(), flume::SendError<DocumentCommand>> {
        self.wake.notify();
        self.sender.send_async(command).await?;
        self.wake.notify();
        Ok(())
    }

    /// Wake registration for native or context-only hosts.
    #[must_use]
    pub fn wake(&self) -> DocumentWake {
        self.wake.clone()
    }
}

/// Application results in exact command admission order. Canvas application already
/// ran inside `drain`; retaining the outcome is for acknowledgement, not deferred work.
#[derive(Debug)]
pub enum CommandOutcome {
    Document(Result<MutationSummary, DocumentError>),
    Canvas(Result<ElementStamp, String>),
    /// Last item in this batch. The host must handle it before draining again.
    RouteCompletion(crate::routing::PreparedCompletion),
}

/// A drain returns every application result; the caller must observe errors and
/// deliver response metadata rather than treating successful enqueue as rendering.
#[derive(Debug)]
pub struct DocumentBatch {
    pub results: Vec<CommandOutcome>,
    /// Advisory: concurrent arrivals after this check also notify the wake hook.
    pub has_more: bool,
}

/// Unique UI-owned receiver. Never clone it: flume receiver clones divide commands.
pub struct DocumentInbox {
    receiver: Receiver<DocumentCommand>,
    wake: DocumentWake,
}

/// Maximum queued commands for the default channel (not a byte-size bound).
pub const DEFAULT_DOCUMENT_CAPACITY: usize = 256;

/// Create one inbox and a cloneable producer with explicit bounded admission.
///
/// A full queue returns the command to the caller; it never blocks the frame or grows
/// an overflow queue. Capacity limits command count, not the size of individual Views.
///
/// # Panics
/// * If the default capacity constant is incorrectly changed to zero.
#[must_use]
pub fn document_channel() -> (DocumentHandle, DocumentInbox) {
    document_channel_with_capacity(
        NonZeroUsize::new(DEFAULT_DOCUMENT_CAPACITY).expect("nonzero capacity"),
    )
}

/// Create a channel with a positive capacity; rendezvous/zero capacity is not allowed.
#[must_use]
pub fn document_channel_with_capacity(capacity: NonZeroUsize) -> (DocumentHandle, DocumentInbox) {
    let (sender, receiver) = flume::bounded(capacity.get());
    let wake = DocumentWake::default();
    (
        DocumentHandle {
            sender,
            wake: wake.clone(),
        },
        DocumentInbox { receiver, wake },
    )
}

impl DocumentInbox {
    /// Apply at most `limit` commands, using only nonblocking receives. Zero applies
    /// nothing. Remaining work schedules another frame; no command is coalesced.
    /// The nonblocking canvas callback runs at the exact dequeue position, with the
    /// current document; it must apply immediately, not append to another queue.
    /// A route completion is yielded without application and ends the batch even if
    /// budget remains. Process it before calling `drain` again so later commands
    /// cannot overtake its lifecycle dispatch and response application.
    #[must_use]
    pub fn drain(
        &mut self,
        document: &mut Document,
        limit: usize,
        mut apply_canvas: impl FnMut(&Document, CanvasCommand) -> Result<ElementStamp, String>,
    ) -> DocumentBatch {
        let mut results = Vec::new();
        for _ in 0..limit {
            let Ok(command) = self.receiver.try_recv() else {
                break;
            };
            results.push(match command {
                DocumentCommand::RouteCompletion(prepared) => {
                    results.push(CommandOutcome::RouteCompletion(prepared));
                    break;
                }
                DocumentCommand::Apply { view, origin } => {
                    CommandOutcome::Document(document.apply_view(view, origin))
                }
                DocumentCommand::Complete { ticket, view } => {
                    CommandOutcome::Document(document.complete_request(&ticket, view))
                }
                DocumentCommand::Canvas(command) => {
                    CommandOutcome::Canvas(apply_canvas(document, command))
                }
            });
        }
        let has_more = !self.receiver.is_empty();
        if has_more {
            self.wake.notify();
        }
        DocumentBatch { results, has_more }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_transformer::Container;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn no_canvas(_: &Document, _: CanvasCommand) -> Result<ElementStamp, String> {
        panic!("unexpected canvas command")
    }

    fn command(id: &str) -> DocumentCommand {
        DocumentCommand::Apply {
            view: View {
                primary: Some(Container {
                    str_id: Some(id.into()),
                    ..Container::default()
                }),
                ..View::default()
            },
            origin: ViewOrigin::Navigation,
        }
    }

    fn prepared_route() -> (crate::routing::Routing, crate::routing::PreparedCompletion) {
        use crate::routing::{RequestContext, Routing};
        use hyperchad_transformer::models::{Selector, SwapStrategy};
        use std::future::Future;
        use std::task::{Context, Waker};

        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container::default()),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let source = document.stamp(document.root_keys()[0]).unwrap();
        let (mut routing, worker) = Routing::new(1, 1024, Arc::new(|| {})).unwrap();
        routing
            .submit(
                &mut document,
                RequestContext::get(
                    source,
                    "/runtime-boundary",
                    Selector::SelfTarget,
                    SwapStrategy::None,
                )
                .unwrap(),
            )
            .unwrap();
        let router = hyperchad_router::Router::new()
            .with_route("/runtime-boundary", |_| async { View::default() });
        let mut work = Box::pin(worker.run(router));
        let mut context = Context::from_waker(Waker::noop());
        assert!(work.as_mut().poll(&mut context).is_pending());
        let prepared = routing.try_take().unwrap();
        (routing, prepared)
    }

    #[test]
    fn route_boundary_yields_after_prior_commands_and_leaves_later_commands_queued() {
        let (_routing, prepared) = prepared_route();
        let (handle, mut inbox) = document_channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let observed = wakes.clone();
        handle.wake().attach(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        });
        handle.submit(command("before")).unwrap();
        handle
            .submit(DocumentCommand::RouteCompletion(prepared))
            .unwrap();
        handle.submit(command("after")).unwrap();
        let before_drain = wakes.load(Ordering::SeqCst);
        let mut document = Document::new();
        let batch = inbox.drain(&mut document, 256, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [
                CommandOutcome::Document(Ok(_)),
                CommandOutcome::RouteCompletion(_)
            ]
        ));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("before"));
        assert!(batch.has_more);
        assert_eq!(wakes.load(Ordering::SeqCst), before_drain + 1);
        // Host handles the yielded boundary before asking for the next batch.
        let batch = inbox.drain(&mut document, 256, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::Document(Ok(_))]
        ));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("after"));
        assert!(!batch.has_more);
    }

    #[test]
    fn route_boundary_respects_capacity_zero_limit_and_single_item_batches() {
        let (_routing, prepared) = prepared_route();
        let (handle, mut inbox) = document_channel_with_capacity(NonZeroUsize::new(1).unwrap());
        handle.submit(command("before")).unwrap();
        let prepared = match handle.submit(DocumentCommand::RouteCompletion(prepared)) {
            Err(flume::TrySendError::Full(DocumentCommand::RouteCompletion(prepared))) => prepared,
            other => panic!("expected owned route completion on full queue: {other:?}"),
        };
        let mut document = Document::new();
        let batch = inbox.drain(&mut document, 0, no_canvas);
        assert_eq!(batch.results.len(), 0);
        assert!(batch.has_more);
        assert_eq!(document.roots(), []);
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::Document(Ok(_))]
        ));
        handle
            .submit(DocumentCommand::RouteCompletion(prepared))
            .unwrap();
        assert!(matches!(
            handle.submit(command("after")),
            Err(flume::TrySendError::Full(_))
        ));
        let batch = inbox.drain(&mut document, 0, no_canvas);
        assert_eq!(batch.results.len(), 0);
        assert!(batch.has_more);
        let batch = inbox.drain(&mut document, 256, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::RouteCompletion(_)]
        ));
        assert!(!batch.has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("before"));
        handle.submit(command("after")).unwrap();
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::Document(Ok(_))]
        ));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("after"));
    }

    #[test]
    fn async_admission_waits_for_drain_then_wakes_host_and_preserves_order() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let (handle, mut inbox) = document_channel_with_capacity(NonZeroUsize::new(1).unwrap());
        let host_wakes = Arc::new(AtomicUsize::new(0));
        let observed_wakes = host_wakes.clone();
        handle.wake().attach(move || {
            observed_wakes.fetch_add(1, Ordering::SeqCst);
        });
        handle.submit(command("first")).unwrap();
        let before_wait = host_wakes.load(Ordering::SeqCst);
        let mut context = Context::from_waker(Waker::noop());
        let mut pending = Box::pin(handle.submit_async(command("second")));
        assert!(pending.as_mut().poll(&mut context).is_pending());
        assert_eq!(host_wakes.load(Ordering::SeqCst), before_wait + 1);
        let mut document = Document::new();
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::Document(Ok(_))]
        ));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("first"));
        let before_admission_wake = host_wakes.load(Ordering::SeqCst);
        assert!(matches!(
            pending.as_mut().poll(&mut context),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(host_wakes.load(Ordering::SeqCst), before_admission_wake + 1);
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert!(matches!(
            batch.results.as_slice(),
            [CommandOutcome::Document(Ok(_))]
        ));
        assert!(!batch.has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("second"));
    }

    #[test]
    fn async_disconnection_returns_owned_command_before_or_during_wait() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        fn assert_owned(error: flume::SendError<DocumentCommand>, expected: &str) {
            let DocumentCommand::Apply {
                view,
                origin: ViewOrigin::Navigation,
            } = error.0
            else {
                panic!("wrong returned command");
            };
            assert_eq!(view.primary.unwrap().str_id.as_deref(), Some(expected));
        }

        let mut context = Context::from_waker(Waker::noop());
        let (handle, inbox) = document_channel_with_capacity(NonZeroUsize::new(1).unwrap());
        handle.submit(command("fills-capacity")).unwrap();
        let mut pending = Box::pin(handle.submit_async(command("waiting-owned")));
        assert!(pending.as_mut().poll(&mut context).is_pending());
        drop(inbox);
        let Poll::Ready(Err(error)) = pending.as_mut().poll(&mut context) else {
            panic!("disconnected admission must return its command");
        };
        assert_owned(error, "waiting-owned");
        let mut disconnected = Box::pin(handle.submit_async(command("already-closed")));
        let Poll::Ready(Err(error)) = disconnected.as_mut().poll(&mut context) else {
            panic!("already disconnected admission must return immediately");
        };
        assert_owned(error, "already-closed");
    }

    #[test]
    fn cancelling_waiting_async_admission_does_not_enqueue_an_overflow_command() {
        use std::future::Future;
        use std::task::{Context, Waker};

        let (handle, mut inbox) = document_channel_with_capacity(NonZeroUsize::new(1).unwrap());
        handle.submit(command("first")).unwrap();
        let mut context = Context::from_waker(Waker::noop());
        let mut pending = Box::pin(handle.submit_async(command("cancelled")));
        assert!(pending.as_mut().poll(&mut context).is_pending());
        drop(pending);
        let mut document = Document::new();
        let batch = inbox.drain(&mut document, 2, no_canvas);
        assert_eq!(batch.results.len(), 1);
        assert!(!batch.has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("first"));
        handle.submit(command("next")).unwrap();
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert_eq!(batch.results.len(), 1);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("next"));
    }

    #[test]
    fn pre_attachment_commands_are_fifo_and_bounded() {
        let (handle, mut inbox) = document_channel();
        handle.submit(command("first")).unwrap();
        handle.submit(command("second")).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let wake_calls = calls.clone();
        handle.wake().attach(move || {
            wake_calls.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut document = Document::new();
        assert!(inbox.drain(&mut document, 0, no_canvas).has_more);
        assert_eq!(document.roots(), []);
        let batch = inbox.drain(&mut document, 1, no_canvas);
        assert!(matches!(batch.results[0], CommandOutcome::Document(Ok(_))));
        assert!(batch.has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("first"));
        assert!(!inbox.drain(&mut document, 1, no_canvas).has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("second"));
    }

    #[test]
    fn dropped_inbox_rejects_and_detachment_keeps_queue() {
        let (handle, mut inbox) = document_channel();
        handle.wake().detach();
        handle.submit(command("queued")).unwrap();
        assert!(matches!(
            inbox.drain(&mut Document::new(), 1, no_canvas).results[0],
            CommandOutcome::Document(Ok(_))
        ));
        drop(inbox);
        assert!(handle.submit(command("closed")).is_err());
    }

    #[test]
    fn full_queue_returns_command_and_retry_preserves_admission_order() {
        let (handle, mut inbox) = document_channel_with_capacity(NonZeroUsize::new(2).unwrap());
        handle.submit(command("first")).unwrap();
        handle.submit(command("second")).unwrap();
        let rejected = match handle.submit(command("third")) {
            Err(flume::TrySendError::Full(command)) => command,
            other => panic!("expected full queue, got {other:?}"),
        };
        let mut document = Document::new();
        assert!(inbox.drain(&mut document, 1, no_canvas).has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("first"));
        handle.submit(rejected).unwrap();
        assert!(inbox.drain(&mut document, 1, no_canvas).has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("second"));
        assert!(!inbox.drain(&mut document, 1, no_canvas).has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("third"));
        drop(inbox);
        assert!(matches!(
            handle.submit(command("closed")),
            Err(flume::TrySendError::Disconnected(_))
        ));
    }

    #[test]
    fn full_admission_does_not_wake_or_mutate_the_queue() {
        let (handle, mut inbox) = document_channel_with_capacity(NonZeroUsize::new(1).unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let wake_calls = calls.clone();
        handle.wake().attach(move || {
            wake_calls.fetch_add(1, Ordering::SeqCst);
        });
        handle.submit(command("accepted")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        for _ in 0..128 {
            assert!(matches!(
                handle.submit(command("rejected")),
                Err(flume::TrySendError::Full(_))
            ));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let mut document = Document::new();
        let batch = inbox.drain(&mut document, 128, no_canvas);
        assert_eq!(batch.results.len(), 1);
        assert!(!batch.has_more);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("accepted"));
    }

    fn canvas_view(id: usize) -> View {
        View {
            primary: Some(Container {
                id,
                str_id: Some("drawing".into()),
                element: hyperchad_transformer::Element::Canvas,
                ..Container::default()
            }),
            ..View::default()
        }
    }

    fn clear_canvas() -> hyperchad_renderer::canvas::CanvasUpdate {
        hyperchad_renderer::canvas::CanvasUpdate {
            target: "drawing".into(),
            canvas_actions: vec![hyperchad_renderer::canvas::CanvasAction::Clear],
        }
    }

    #[test]
    fn document_canvas_document_commands_apply_at_their_fifo_positions() {
        use crate::canvas::CanvasAdapter;

        let (handle, mut inbox) = document_channel();
        handle
            .submit(DocumentCommand::Apply {
                view: canvas_view(1),
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        handle
            .submit(DocumentCommand::Canvas(CanvasCommand::Update(
                clear_canvas(),
            )))
            .unwrap();
        handle
            .submit(DocumentCommand::Apply {
                view: canvas_view(2),
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        handle
            .submit(DocumentCommand::Canvas(CanvasCommand::Update(
                clear_canvas(),
            )))
            .unwrap();
        let mut document = Document::new();
        let mut adapter = CanvasAdapter::new();
        let mut observed = Vec::new();
        let batch = inbox.drain(&mut document, 4, |document, command| {
            let stamp = adapter.apply_command(document, command)?;
            observed.push((document.get(stamp.key).unwrap().id, stamp));
            Ok(stamp)
        });
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].0, 1);
        assert_eq!(observed[1].0, 2);
        assert_ne!(observed[0].1.key, observed[1].1.key);
        assert!(!document.is_current(observed[0].1));
        assert!(document.is_current(observed[1].1));
        assert!(matches!(
            batch.results.as_slice(),
            [
                CommandOutcome::Document(Ok(_)),
                CommandOutcome::Canvas(Ok(_)),
                CommandOutcome::Document(Ok(_)),
                CommandOutcome::Canvas(Ok(_))
            ]
        ));
        // A delayed producer explicitly stamped for the replaced generation must not
        // be silently rebound to the current element bearing the same string ID.
        handle
            .submit(DocumentCommand::Canvas(CanvasCommand::Stamped {
                stamp: observed[0].1,
                update: clear_canvas(),
            }))
            .unwrap();
        handle.submit(command("after-canvas-error")).unwrap();
        let batch = inbox.drain(&mut document, 2, |document, command| {
            adapter.apply_command(document, command)
        });
        assert!(matches!(
            batch.results.as_slice(),
            [
                CommandOutcome::Canvas(Err(_)),
                CommandOutcome::Document(Ok(_))
            ]
        ));
        assert_eq!(
            document.roots()[0].str_id.as_deref(),
            Some("after-canvas-error")
        );
    }

    #[test]
    fn error_does_not_block_later_commands() {
        let (handle, mut inbox) = document_channel();
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container::default()),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let old = document.root_keys()[0];
        handle.submit(command("replacement")).unwrap();
        handle
            .submit(DocumentCommand::Apply {
                view: View::default(),
                origin: ViewOrigin::Element {
                    source: old,
                    target: hyperchad_transformer::models::Selector::SelfTarget,
                    strategy: hyperchad_transformer::models::SwapStrategy::None,
                },
            })
            .unwrap();
        handle.submit(command("last")).unwrap();
        let batch = inbox.drain(&mut document, 3, no_canvas);
        assert!(matches!(batch.results[0], CommandOutcome::Document(Ok(_))));
        assert!(matches!(
            batch.results[1],
            CommandOutcome::Document(Err(DocumentError::StaleSource))
        ));
        assert!(matches!(batch.results[2], CommandOutcome::Document(Ok(_))));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("last"));
    }
}
