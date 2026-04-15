# hyperchad_shared_state_transport

Renderer-agnostic transport abstraction layer for shared state.

The transport crate carries protocol messages defined in
`hyperchad_shared_state_models` and provides configuration and fallback behavior
for realtime connections.

Built-in adapters:

* `adapter-ws-json` - JSON-over-WebSocket client adapter
* `adapter-sse-post-json` - SSE receive + HTTP POST send fallback adapter

Both adapters serialize transport messages using `serde_json` and rely on
`TransportOutbound`/`TransportInbound` models from `hyperchad_shared_state_models`.
