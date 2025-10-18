# P2P Integration - Preamble

The MoosicBox project currently uses a centralized tunnel server architecture with WebSocket connections for communication between clients and the server. While functional, this approach has several limitations: it requires port forwarding and NAT configuration, depends on centralized infrastructure, and can introduce unnecessary latency for peer-to-peer operations.

This specification outlines the integration of a P2P (peer-to-peer) communication system as an alternative to the existing tunnel server setup. The solution leverages the Iroh library (iroh.computer) to provide direct device-to-device connections with automatic NAT traversal, eliminating the need for centralized infrastructure while improving performance and reliability.

The P2P system will be implemented as a new `switchy_p2p` package that provides a trait-based abstraction over different P2P implementations. This approach allows for both production use (via Iroh) and deterministic testing (via a simulator implementation), while maintaining the flexibility to add other P2P libraries in the future.

The implementation will run alongside the existing tunnel system during migration, allowing for a gradual rollout and easy rollback if needed. The P2P system is designed to be a complete alternative to tunnel functionality, not an extension of it, providing a cleaner separation of concerns and architecture.

## Prerequisites

- All commands must be run within `nix develop --command ...` if using NixOS
- All `cargo` commands assume you're in the nix shell

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- NO COMPROMISES - halt on any deviation from spec
    - Includes comprehensive test coverage for all business logic
    - Tests must be written alongside implementation, not deferred
    - Both success and failure paths must be tested
- Living documents that evolve during implementation
- After having completed a checkbox, 'check' it and add details under it regarding the file/location updated as PROOF

See `p2p/plan.md` for the current status of the P2P integration and what's next to be done.
