# switchy_age

SSH recipient byte wrapping with compile-time backend selection:

* Default `native`: real age ciphertext and SSH private-key unwrapping via age.
* `simulator`: deterministic, versioned simulation envelopes with synthetic identities
  (`sim-age:<name>`). Simulator takes precedence if both features are enabled.

Simulation envelopes contain plaintext. They provide **no confidentiality or adversarial
integrity**. Checksums model accidental corruption only. Never use real credentials in
simulation. Native ciphertext and real SSH identity strings are rejected by the simulator;
native age rejects simulation envelopes. No automatic cross-format fallback occurs.

This narrow API does not support interactive encrypted-key passwords, age plugins, streaming,
or filesystem acquisition. Callers own input size limits and secret-buffer custody.
