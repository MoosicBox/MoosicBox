# Files decoding regression input

`silk.ogg` wraps byte copies of audio_decoder's four frozen `wb_sequence_*.packet`
fixtures in CRC-correct Ogg pages with mono 48 kHz OpusHead and no pre-skip.
It tests public transcoding failure propagation before encoder initialization.
The independent C provenance for these nonzero persistent SILK packets is in
`packages/audio_decoder/tests/fixtures/README.md`.
