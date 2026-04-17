use hyperchad_renderer::View;

/// Ordered stream frame emitted by the testing backend.
#[derive(Debug, Clone)]
pub enum StreamFrame {
    /// Full view update.
    View { seq: u64, view: View },
    /// Partial view update.
    PartialView { seq: u64, view: View },
    /// Generic event frame.
    Event {
        seq: u64,
        name: String,
        value: Option<String>,
    },
    /// Canvas update frame.
    #[cfg(feature = "canvas")]
    CanvasUpdate {
        seq: u64,
        update: hyperchad_renderer::canvas::CanvasUpdate,
    },
}

impl StreamFrame {
    /// Returns the sequence number of this frame.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        match self {
            Self::View { seq, .. } => *seq,
            Self::PartialView { seq, .. } => *seq,
            Self::Event { seq, .. } => *seq,
            #[cfg(feature = "canvas")]
            Self::CanvasUpdate { seq, .. } => *seq,
        }
    }

    /// Returns a stable kind label for this frame.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::View { .. } => "view",
            Self::PartialView { .. } => "partial_view",
            Self::Event { .. } => "event",
            #[cfg(feature = "canvas")]
            Self::CanvasUpdate { .. } => "canvas_update",
        }
    }
}

/// Ordered collection of stream frames.
#[derive(Debug, Clone, Default)]
pub struct Transcript {
    next_seq: u64,
    frames: Vec<StreamFrame>,
}

impl Transcript {
    /// Appends a view frame and infers full vs partial kind.
    pub fn push_view(&mut self, view: View) {
        self.next_seq = self.next_seq.saturating_add(1);

        let is_partial = view.primary.is_none()
            || !view.fragments.is_empty()
            || !view.delete_selectors.is_empty();

        let frame = if is_partial {
            StreamFrame::PartialView {
                seq: self.next_seq,
                view,
            }
        } else {
            StreamFrame::View {
                seq: self.next_seq,
                view,
            }
        };

        self.frames.push(frame);
    }

    /// Appends an event frame.
    pub fn push_event(&mut self, name: String, value: Option<String>) {
        self.next_seq = self.next_seq.saturating_add(1);
        self.frames.push(StreamFrame::Event {
            seq: self.next_seq,
            name,
            value,
        });
    }

    /// Appends a canvas update frame.
    #[cfg(feature = "canvas")]
    pub fn push_canvas_update(&mut self, update: hyperchad_renderer::canvas::CanvasUpdate) {
        self.next_seq = self.next_seq.saturating_add(1);
        self.frames.push(StreamFrame::CanvasUpdate {
            seq: self.next_seq,
            update,
        });
    }

    /// Returns all frames.
    #[must_use]
    pub fn frames(&self) -> &[StreamFrame] {
        &self.frames
    }

    /// Returns kind labels in order.
    #[must_use]
    pub fn kinds(&self) -> Vec<&'static str> {
        self.frames.iter().map(StreamFrame::kind).collect()
    }

    /// Clears all frames and resets sequence counter.
    pub fn clear(&mut self) {
        self.next_seq = 0;
        self.frames.clear();
    }
}
