# moosicbox_ogg_opus

Dependency-free, pure Rust Ogg Opus container framing. No audio decoder,
encoder, Symphonia, or application dependency is included.

```rust
use moosicbox_ogg_opus::{OpusHead, OpusTags, OggOpusWriter, OggOpusReader};
use std::io::Cursor;

let head = OpusHead::new(1, 120, 48_000)?;
let mut writer = OggOpusWriter::new(Vec::new(), head, &OpusTags::default(), 42)?;
// A standard 20 ms Opus silence packet; real encoders provide their own packets.
writer.write_packet(&[0xf8, 0xff, 0xfe])?;
let bytes = writer.finish(800)?;
let mut reader = OggOpusReader::new(Cursor::new(bytes))?;
let packet = reader.next_packet()?.unwrap();
assert_eq!(packet.audible_samples(), 800);
# Ok::<(), std::io::Error>(())
```

All sample counts are **per channel at 48 kHz**. Decode each packet normally,
then discard `trim_start` and `trim_end` samples. Apply `OpusHead::output_gain`
(signed Q7.8 dB) in the audio layer, not in the container. Header pre-skip must
come from the encoder's actual lookahead. `finish` takes the actual input length,
not the padded encoded duration.

For seeking, reset the codec after `seek_to`, decode all returned packets for
preroll, and discard samples before `SeekResult::discard_until` on the packet's
`decoded_start` timeline. Seeking performs bounded-memory linear scans, retains
80 ms of available preroll, and clamps beyond-end requests to EOS.

The reader checks CRC, serial, page sequence, continuation, granules, headers,
and truncation. `OggOpusReader` reads one logical stream (stopping at its EOS);
chained streams following EOS are not consumed. `OggOpusChainReader` consumes
sequential logical streams with explicit section boundaries (see below).
Multiplexed/interleaved pages are deliberately **unsupported and rejected**;
this is not a general-purpose multiplexed Ogg demuxer.
Each packet or metadata packet is limited to 16 MiB. TOC duration parsing is
not a substitute for codec packet/entropy validation. Channel mapping families
0, 1, and 255 are represented; mapping and multistream decoding belong to the
codec. Non-UTF-8 comments remain losslessly available as byte vectors.

The writer retains one audio packet to produce exact final-page trimming and
paginates large packets with continued-page flags and unknown intermediate
granules. It does not conceal I/O errors or finalize on drop. Call `finish` and
use its returned sink to establish successful completion.

## Multistream header construction

Use `OpusHead::new_family1(channels, pre_skip, input_sample_rate)` for canonical
family-1 allocation and mapping (1–8 channels). Application PCM is in **Vorbis
order**, not WAV order; the constructor rustdoc lists each channel order. Build
the codec layout from the returned `stream_count`, `coupled_count`, and
`channel_mapping`. If a caller supplies an independent layout, compare all three
fields and reject mismatches rather than mislabeling the encoded audio.

Family 255 requires an explicit layout: initialize the public `OpusHead` fields,
set `mapping_family: 255`, set `channels` to the output map length, and call
`head.encode()?` to validate before use (the writer also validates on creation).
Map entries address coded channels; 255 means silence, and duplicates are valid.
No speaker positions are inferred. Per-stream bitrates are not container header
fields: codec/product configuration must validate their count and values.
Parsing accepts structurally valid family-1 layouts; it does not require the
canonical encoder allocation selected by `new_family1`.

## Sequential logical chains

`OggOpusChainReader::next_event` returns these events in order for every section:

* `SectionStart(ChainSection { index, serial, audible_start, head, tags })`:
  configure a **fresh/reset decoder** using this section's headers, including
  channel mapping and gain. Never carry codec prediction state across sections.
* `Packet(AudioPacket)`: packet duration, `decoded_start`, and trim counts use
  the current section's granule timeline, not the cumulative chain timeline.
* `SectionEnd { index, samples }`: exact audible section duration including its
  own pre-skip and EOS trimming. The following section's `audible_start` is the
  sum of completed section durations. Channel counts may change across sections.

Only clean physical EOF after a complete EOS yields `None`; trailing junk,
truncated later sections, missing EOS, reused serials and interleaving are errors.
A zero-sample section is allowed when represented by valid audio/EOS with all
samples trimmed; empty input and header-only streams are not accepted.
Errors from `next_event` are terminal. After an error, reopen the input rather
than trying to recover/synchronize through damaged pages.

`with_section_limit(source, limit)` bounds serial tracking; `new` defaults to
1024 sections. No chain-wide packet/page index is accumulated. Page storage is
bounded by Ogg lacing, packets by 16 MiB, and metadata by the same packet bound.
This is a memory bound, not a CPU/time budget for arbitrarily long sources.

For seekable sources beginning at byte zero, `total_samples` validates and sums
all sections without consuming events. `seek_to_section(index, sample)` scans
preceding sections, clamps the **section-local audible** sample to that section's
end and returns `ChainSeekResult { section, local }`. It re-emits `SectionStart`
then preroll packets: reset the decoder, decode them all, and discard samples
before `local.discard_until` on the section-local granule timeline in addition
to normal packet trims. Preroll never crosses a logical section boundary.
`SectionEnd.samples` remains the full section duration after seeking, not just
the samples returned since the seek. Later sections are validated on subsequent
reads; use `total_samples` first if complete physical-chain validation is needed.
