use bytes::{BufMut as _, Bytes, BytesMut};

#[must_use]
#[derive(Debug, Clone)]
pub struct EventData {
    event: Option<String>,
    data: String,
}

impl EventData {
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
        }
    }

    /// Sets `event` name field, returning a new data message.
    pub fn event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }
}

impl From<EventData> for Event {
    fn from(data: EventData) -> Self {
        Self::Data(data)
    }
}

/// Server-sent events message containing one or more fields.
#[must_use]
#[derive(Debug, Clone)]
pub enum Event {
    Data(EventData),
}

impl Event {
    /// Splits data into lines and prepend each line with `prefix`.
    fn line_split_with_prefix(buf: &mut BytesMut, prefix: &'static str, data: &str) {
        // initial buffer size guess is len(data) + 10 lines of prefix + EOLs + EOF
        buf.reserve(data.len() + (10 * (prefix.len() + 1)) + 1);

        // append prefix + space + line to buffer
        for line in data.split('\n') {
            buf.put_slice(prefix.as_bytes());
            buf.put_slice(line.as_bytes());
            buf.put_u8(b'\n');
        }
    }

    /// Serializes message into event-stream format.
    pub(crate) fn into_bytes(self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Self::Data(EventData { event, data }) => {
                if let Some(text) = event {
                    buf.put_slice(b"event: ");
                    buf.put_slice(text.as_bytes());
                    buf.put_u8(b'\n');
                }

                Self::line_split_with_prefix(&mut buf, "data: ", &data);
            }
        }

        // final newline to mark end of message
        buf.put_u8(b'\n');

        buf.freeze()
    }
}
