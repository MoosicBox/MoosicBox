# Opus Codec Integration

Opus is a modern, open-source audio codec standardized by the IETF (RFC 6716) that provides superior compression efficiency and low latency compared to traditional codecs. It's widely adopted across streaming platforms, VoIP applications, and modern audio applications.

Currently, MoosicBox lacks Opus support despite its widespread use in modern audio files and streaming scenarios. This limitation affects compatibility with Opus-encoded audio content.

This specification outlines the implementation of Opus codec support in MoosicBox through Symphonia's extensible codec trait system. The implementation will provide a custom Opus decoder that integrates seamlessly with MoosicBox's existing audio pipeline while maintaining compatibility with the official Opus specification (RFC 6716).

The solution leverages Symphonia's `Decoder` trait and `CodecRegistry` to implement Opus decoding without requiring modifications to the core Symphonia library.

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- All technical decisions reference RFC 6716 for specification compliance
- NO COMPROMISES - halt on any deviation from spec
    - Includes comprehensive test coverage for all business logic
    - Tests must be written alongside implementation, not deferred
    - Both success and failure paths must be tested
- Living documents that evolve during implementation
- After having completed a checkbox, 'check' it and add details under it regarding the file/location updated as PROOF

See `opus/plan.md` for the current status of the Opus codec integration and what's next to be done.
