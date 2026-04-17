# HyperChad Testing Renderer

`hyperchad_renderer_testing` provides a deterministic, in-process testing backend for HyperChad.

It focuses on:

- Capturing render/event streams in an ordered transcript
- Applying full and partial `View` updates to a virtual DOM
- Executing typed action semantics without a browser runtime
- Supporting ergonomic assertions and snapshot-friendly output

This crate is intended for integration-style tests that should not require native GUI stacks or browser automation.
