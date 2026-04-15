# hyperchad_shared_state

Shared state runtime for synchronizing authoritative state across participants.

Key design goals:

* Domain-neutral API (no game-specific terminology)
* Event journal plus snapshots
* Stateless-server friendly architecture
* Works with `switchy_database` backends via schema/query builders
* Default operation without external infrastructure
