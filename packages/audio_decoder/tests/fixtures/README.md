# MP3 delay regression packet

`delay_frame.mp3` is the first complete MPEG-1 Layer III mono frame (417 bytes,
44.1 kHz, 1152 decoded frames) from this locally generated sine wave:

```sh
ffmpeg -hide_banner -loglevel error -f lavfi \
  -i 'sine=frequency=1000:sample_rate=44100:duration=0.1' \
  -ac 1 -c:a libmp3lame -b:a 128k -write_xing 0 -id3v2_version 0 \
  -f mp3 pipe:1
```

Generated with FFmpeg 9.0.1. The first frame begins `ff fb 90 c4`; its byte length
is `144 * 128000 / 44100` rounded down. SHA-256:
`dd736147eb4ba1ba704ca6aaced9082c4abec8beb63a594bbbcae844d337070c`.

`src/playback_tests.rs` supplies a minimal demuxer with explicit PTS -576 and
576 leading trim frames, then runs the real registered Symphonia MP3 decoder.
Both production playback APIs must retain the resulting 576 valid frames,
retain all 1152 when gapless is disabled without seeking, and trim precisely
for seeks at zero, within the packet, and at its end. The expected PCM is the
same real decoder's untrimmed output suffix, not fabricated sample data.

## Stateful SILK reference

`wb_sequence_0.packet` through `wb_sequence_3.packet` and
`wb_sequence.api48.pcm` are byte copies of the independently frozen native SILK
reference fixtures under `packages/opus_native/src/silk/reference/`. They are
four persistent 20 ms WB mono packets and their raw 48 kHz API output. Original
provenance is `wb_sequence.provenance.json` in that directory: pinned libopus
revision `34bba701ae97c913de719b1f7c10686f62cddb15`, fixed point, scalar, no
floating API. Tests build Ogg pages with valid CRCs around those packets, not
new generated expectations. PCM gain is calculated independently from the
OpusHead Q8 dB field before exact f32 comparison.

`hybrid.packet` is copied from the frozen `hybrid_fb_mono/packet.bin` corpus
case. Hybrid is now supported; runtime selection is checked via the real
product factory and the adapter’s explicit backend identity getter.

Copied reference fixture SHA-256 identities:

```text
c5769ac67e20531256cc006e22152d5803391106bde3fe31bcb9bf887187b721  wb_sequence.api48.pcm
d0d1e64fb63b3042aa12c1db896a34ac8f559fcbd9417b530219a01335ac1c0b  hybrid.packet
11a5cb8ea03471e2ece1dad52cadb3230907736d0fc6c71207a69424498682df  wb_sequence_0.packet
015ee60b9b8c38c0a98338903eaa19d338d7f74621d1250378ee4c86b9e9f888  wb_sequence_1.packet
3f7bb807834127db7c0c4522c265ac6113e66670f351d43f27e690013a684bb0  wb_sequence_2.packet
9932b6085f54ba96e52c3b7430128e58578f4b7da5a3d7938ddf8173ed0744bf  wb_sequence_3.packet
```

## Hybrid persistent product flow

`hybrid_sequence_{0,1,2,3}.packet` and `hybrid_sequence.pcm` are copied from
`packages/opus_native/test_vectors/frozen/hybrid_fb_mono/{.,packet_001,packet_002,packet_003}`,
concatenating the four independently generated `expected.pcm` files. Their
metadata pins fixed-point scalar C revision `34bba701ae97c913de719b1f7c10686f62cddb15`.
Native must match that fixed-point oracle exactly.

The compatibility backend links a different C build (Nix libopus 1.6.1), whose
hybrid synthesis differs from scalar fixed point. `hybrid_sequence.libopus.pcm`
is independently captured by four persistent `opus_decode` calls through ctypes
to that exact library, not through the adapter or native implementation. Its
library identity and output hash are in `hybrid_sequence.libopus.provenance.json`.
Both public sync/channel flows are checked against their corresponding independent
C oracle; no tolerance, zero expectation, or native-generated reference is used.
