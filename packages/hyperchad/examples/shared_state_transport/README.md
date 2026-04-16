# HyperChad Shared State Transport Example

This example shows a focused multiplayer counter built with:

* `/$action` + shared-state route bridge for commands
* shared-state transport dispatcher for replay + live fanout
* vanilla-js `plugin-shared-state` auto channel subscription

No app-level custom JavaScript is required.

## Run

```bash
cargo run -p hyperchad_shared_state_transport_example --features vanilla-js -- serve
```

Or from this directory:

```bash
PORT=8343 cargo run -- serve
```

Then open <http://localhost:8343> in two browser tabs and click `+1` / `-1`.

## What to Look For

* Clicking `+1` or `-1` sends a custom action through `/$action`
  * In the template this example uses `custom("'increment'")` / `custom("'decrement'")` so the rendered JS action value is a string literal.
* The action bridge converts that into `CommandEnvelope` messages
* The runtime appends events, fanout publishes updates, and transport streams them
* The command processor publishes `View::with_fragment(...)` updates, which stream to `/$sse` as `partial_view` events targeting `#counter-value`
* Both tabs receive live partial updates to the counter element (no full-page rerender)

## Relevant APIs

* `AppBuilder::with_shared_state_route_bridge(...)`
* `AppBuilder::with_shared_state_transport_dispatcher(...)`
* `RuntimeFanoutTransportDispatcher`
* `View::builder().with_fragment(...)` for targeted partial updates
