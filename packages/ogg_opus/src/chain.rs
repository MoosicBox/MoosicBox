//! Sequential logical sections; interleaved physical streams are deliberately rejected.
use std::collections::BTreeSet;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

use crate::{AudioPacket, OggOpusReader, OpusHead, OpusTags, SeekResult, invalid};

/// Metadata and position of one logical stream in a physical Ogg chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainSection {
    /// Zero-based logical section index.
    pub index: usize,
    /// Ogg logical-stream serial number.
    pub serial: u32,
    /// Audible offset of this section in the chain, at 48 kHz per channel.
    pub audible_start: u64,
    /// Codec configuration; a new decoder is required at each section start.
    pub head: OpusHead,
    /// Section-local comments.
    pub tags: OpusTags,
}

/// Explicit section boundaries. Packet granules and trims remain section-local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainEvent {
    /// Configure/reset the decoder, even if the channel count has not changed.
    SectionStart(ChainSection),
    /// Decode using the current section's configuration.
    Packet(AudioPacket),
    /// All packets of this section have been returned, including exact EOS trimming.
    SectionEnd {
        /// Zero-based section index.
        index: usize,
        /// Audible duration of this section, at 48 kHz per channel.
        samples: u64,
    },
}

/// Section-aware seek result. Decode preroll and discard before the local cutoff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainSeekResult {
    /// Selected section and its cumulative audible offset.
    pub section: ChainSection,
    /// Section-local sample position and granule cutoff.
    pub local: SeekResult,
}

/// Streaming sequential Ogg Opus chains with explicit decoder reset boundaries.
///
/// Rejects interleaving/multiplexing and reused serial numbers. At most 1024
/// sections are accepted by default; packet limits are those of `OggOpusReader`.
/// Errors are terminal: recover the source or create a new reader after an error.
pub struct OggOpusChainReader<R> {
    current: Option<OggOpusReader<BufReader<R>>>,
    source: Option<BufReader<R>>,
    section: ChainSection,
    serials: BTreeSet<u32>,
    max_sections: usize,
    samples: u64,
    start_pending: bool,
    end_returned: bool,
    failed: bool,
}

impl<R: Read> OggOpusChainReader<R> {
    /// Current section's absolute granule offset of decoded sample zero.
    ///
    /// Returns `None` until its first audio page is read, or when no section
    /// reader remains after physical EOF or a terminal error. Section transitions
    /// reset the origin; seeking exposes the selected section's preroll origin.
    /// This accessor neither consumes pending boundary events nor queued packets.
    pub fn granule_origin(&self) -> Option<u64> {
        self.current
            .as_ref()
            .and_then(OggOpusReader::granule_origin)
    }

    /// Recover the buffered source without dropping prefetched unread bytes.
    ///
    /// Returns `None` if a prior terminal error consumed the source while opening
    /// a section or seeking. Unreturned packet/event queues and partial packets
    /// are discarded; reading resumes at the next unparsed byte, not at the next
    /// queued audio packet. Keep the returned `BufReader` intact: its own
    /// `into_inner` would discard unread bytes. For seekable sources,
    /// [`Self::try_into_inner`] safely removes buffering with error reporting.
    pub fn into_inner(self) -> Option<BufReader<R>> {
        self.current.map(OggOpusReader::into_inner).or(self.source)
    }

    /// Read the first section's headers. Empty input is not an Ogg Opus chain.
    ///
    /// # Errors
    /// * Propagates I/O errors and rejects corrupt or truncated headers.
    pub fn new(source: R) -> io::Result<Self> {
        Self::with_section_limit(source, 1024)
    }

    /// Construct with an explicit nonzero bound on sections and serial tracking.
    ///
    /// # Errors
    /// * Rejects a zero limit and invalid first-section headers; propagates I/O errors.
    pub fn with_section_limit(source: R, max_sections: usize) -> io::Result<Self> {
        if max_sections == 0 {
            return Err(invalid("section limit must be nonzero"));
        }
        let current = OggOpusReader::new(BufReader::new(source))?;
        let serial = current
            .serial
            .ok_or_else(|| invalid("missing section serial"))?;
        let section = ChainSection {
            index: 0,
            serial,
            audible_start: 0,
            head: current.head().clone(),
            tags: current.tags().clone(),
        };
        Ok(Self {
            current: Some(current),
            source: None,
            section,
            serials: BTreeSet::from([serial]),
            max_sections,
            samples: 0,
            start_pending: true,
            end_returned: false,
            failed: false,
        })
    }

    /// Return a section boundary or packet. `None` means clean physical EOF after EOS.
    ///
    /// # Errors
    /// * Rejects corruption, truncation, interleaving, reused serials, section-limit
    ///   exhaustion, sample overflow and further calls following an error.
    /// * Propagates source I/O errors. Trailing non-Ogg bytes are errors, not EOF.
    pub fn next_event(&mut self) -> io::Result<Option<ChainEvent>> {
        if self.failed {
            return Err(invalid("chain reader failed"));
        }
        let result = self.read_event();
        self.failed = result.is_err();
        result
    }

    fn read_event(&mut self) -> io::Result<Option<ChainEvent>> {
        if self.start_pending {
            self.start_pending = false;
            return Ok(Some(ChainEvent::SectionStart(self.section.clone())));
        }
        if self.end_returned {
            if let Some(current) = self.current.take() {
                self.source = Some(current.into_inner());
            }
            let source = self
                .source
                .as_mut()
                .ok_or_else(|| invalid("chain source unavailable"))?;
            if source.fill_buf()?.is_empty() {
                return Ok(None);
            }
            if self.serials.len() >= self.max_sections {
                return Err(invalid("chain section limit exceeded"));
            }
            let current = OggOpusReader::new(
                self.source
                    .take()
                    .ok_or_else(|| invalid("chain source unavailable"))?,
            )?;
            let serial = current
                .serial
                .ok_or_else(|| invalid("missing section serial"))?;
            if !self.serials.insert(serial) {
                return Err(invalid("reused logical stream serial"));
            }
            self.section = ChainSection {
                index: self.section.index + 1,
                serial,
                audible_start: self
                    .section
                    .audible_start
                    .checked_add(self.samples)
                    .ok_or_else(|| invalid("chain sample overflow"))?,
                head: current.head().clone(),
                tags: current.tags().clone(),
            };
            self.current = Some(current);
            self.samples = 0;
            self.end_returned = false;
            return Ok(Some(ChainEvent::SectionStart(self.section.clone())));
        }
        if let Some(packet) = self
            .current
            .as_mut()
            .ok_or_else(|| invalid("chain section unavailable"))?
            .next_packet()?
        {
            self.samples = self
                .samples
                .checked_add(u64::from(packet.audible_samples()))
                .ok_or_else(|| invalid("section sample overflow"))?;
            return Ok(Some(ChainEvent::Packet(packet)));
        }
        self.end_returned = true;
        Ok(Some(ChainEvent::SectionEnd {
            index: self.section.index,
            samples: self.samples,
        }))
    }
}

impl<R: Read + Seek> OggOpusChainReader<R> {
    /// Recover the bare source at the next unparsed byte, retaining unread bytes
    /// by seeking backwards over the buffered read-ahead before unwrapping.
    /// Queued packets/events and partial packet assembly are not replayed.
    ///
    /// # Errors
    /// * Returns an error if a prior terminal failure consumed the source, or
    ///   propagates failure to restore its logical byte position. The source
    ///   cannot be recovered from this consuming error result.
    pub fn try_into_inner(self) -> io::Result<R> {
        let mut source = self
            .into_inner()
            .ok_or_else(|| invalid("chain source unavailable"))?;
        let position = source.stream_position()?;
        // stream_position alone does not discard BufReader's read-ahead or
        // reposition the underlying source. Seek explicitly before unwrapping.
        source.seek(SeekFrom::Start(position))?;
        Ok(source.into_inner())
    }

    /// Return the sum of all section durations without changing event position.
    /// Uses a bounded-memory scan and validates the complete physical chain.
    ///
    /// # Errors
    /// * Propagates I/O, framing, serial, limit and sample overflow errors.
    /// * If restoring the byte position fails, the reader is terminally failed.
    pub fn total_samples(&mut self) -> io::Result<u64> {
        if self.failed {
            return Err(invalid("chain reader failed"));
        }
        let source = if let Some(current) = self.current.as_mut() {
            &mut current.source
        } else {
            self.source
                .as_mut()
                .ok_or_else(|| invalid("chain source unavailable"))?
        };
        let offset = source.stream_position()?;
        let result = (|| {
            source.seek(SeekFrom::Start(0))?;
            let mut scan = OggOpusChainReader::with_section_limit(&mut *source, self.max_sections)?;
            let mut total = 0_u64;
            while let Some(event) = scan.next_event()? {
                if let ChainEvent::SectionEnd { samples, .. } = event {
                    total = total
                        .checked_add(samples)
                        .ok_or_else(|| invalid("chain sample overflow"))?;
                }
            }
            Ok(total)
        })();
        if let Err(error) = source.seek(SeekFrom::Start(offset)) {
            self.failed = true;
            return Err(error);
        }
        result
    }

    /// Seek within a zero-based section, clamping the local sample to its EOS.
    ///
    /// Scans preceding sections with bounded memory. The next event is always
    /// `SectionStart`, followed by packets including up to 80 ms available preroll.
    /// Reset the decoder and discard samples before `local.discard_until` on the
    /// packet granule timeline (not the cumulative chain timeline). A source must
    /// begin at byte zero. Later sections are validated when subsequently read.
    ///
    /// # Errors
    /// * Rejects nonexistent sections, corrupt preceding/selected sections, limits,
    ///   overflow and I/O failures. Errors leave this reader terminally failed.
    pub fn seek_to_section(&mut self, index: usize, sample: u64) -> io::Result<ChainSeekResult> {
        self.failed = true;
        let mut source = if let Some(current) = self.current.take() {
            current.into_inner()
        } else {
            self.source
                .take()
                .ok_or_else(|| invalid("chain source unavailable"))?
        };
        source.seek(SeekFrom::Start(0))?;
        // Reuse the source's existing buffer rather than layering another buffer.
        let mut scan = Self::with_section_limit(source.into_inner(), self.max_sections)?;
        let mut offset = 0;
        while scan.section.index < index {
            match scan.next_event()? {
                Some(ChainEvent::SectionEnd { .. }) => {
                    offset = scan
                        .current
                        .as_mut()
                        .ok_or_else(|| invalid("chain section unavailable"))?
                        .source
                        .stream_position()?;
                }
                None => return Err(invalid("section does not exist")),
                _ => {}
            }
        }
        let local = scan
            .current
            .as_mut()
            .ok_or_else(|| invalid("chain section unavailable"))?
            .seek_to_offset(sample, offset)?;
        // Include the skipped portion when reporting SectionEnd after seeking.
        let current = scan
            .current
            .as_ref()
            .ok_or_else(|| invalid("chain section unavailable"))?;
        let retained = current
            .queue
            .front()
            .map_or(local.discard_until, |packet| packet.decoded_start);
        scan.samples = retained
            .saturating_sub(current.granule_origin)
            .saturating_sub(u64::from(current.head.pre_skip));
        scan.start_pending = true;
        scan.end_returned = false;
        let result = ChainSeekResult {
            section: scan.section.clone(),
            local,
        };
        *self = scan;
        Ok(result)
    }
}
