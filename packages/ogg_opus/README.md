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
and truncation. It reads one logical stream (stopping at its EOS); multiplexed
pages inside it are rejected. Chained streams following EOS are not consumed.
Each packet or metadata packet is limited to 16 MiB. TOC duration parsing is
not a substitute for codec packet/entropy validation. Channel mapping families
0, 1, and 255 are represented; mapping and multistream decoding belong to the
codec. Non-UTF-8 comments remain losslessly available as byte vectors.

The writer retains one audio packet to produce exact final-page trimming and
paginates large packets with continued-page flags and unknown intermediate
granules. It does not conceal I/O errors or finalize on drop. Call `finish` and
use its returned sink to establish successful completion.
