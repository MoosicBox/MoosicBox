# DST (Deterministic Simulation Testing) - Preamble

The MoosicBox project requires deterministic testing capabilities for reliable simulation and reproducible debugging. Currently, the codebase contains numerous sources of non-determinism: HashMap/HashSet usage, system time dependencies, random number generation, and environment variable access without proper abstractions.

This specification outlines the systematic transformation of non-deterministic patterns into deterministic alternatives through a dual-mode approach. The solution uses "switchy" packages that provide both production (normal) and simulation (deterministic) modes, controlled via compile-time feature flags. This allows maintaining full production functionality while enabling perfect reproducibility in test environments.

The DST system ensures that all tests can run with identical results regardless of when, where, or how many times they execute. This is critical for debugging complex multi-threaded scenarios, reproducing production issues, and maintaining test reliability across different environments.

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

See `dst/plan.md` for the current status of the DST implementation and what's next to be done.
