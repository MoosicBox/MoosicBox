# Switchy Network

A deterministic simulated network for switchy simulations.

Each simulation run (a `switchy_simulation` context) has one network whose
nodes are named by strings — in a `simvar` simulation, the names of its hosts
and clients. Faults:

- **Link policies**: per directed link latency (uniform between a minimum and a
  maximum), loss probability, duplication probability, and whether messages may
  be reordered. Links without a policy use the default policy.
- **Partitions**: symmetric and one-way partitions between groups of nodes,
  isolation of a node, and healing.
- **Node crashes**: a down node loses every message in flight to or from it, its
  endpoint closes, and it cannot send until it is up again. Each crash starts a
  new incarnation, so transports can reset connections of an earlier life.

Every random decision is drawn from the run's `"switchy_network"` RNG stream,
so a seed replays the same deliveries and drops. Time is the run's simulated
clock.

`switchy_tcp`'s simulator applies the same partitions, crashes and latency to
TCP connections.

## Installation

```toml
[dependencies]
switchy_network = "0.5.0"
```

## Usage

```rust
use std::time::Duration;

use switchy_network::{Endpoint, LinkPolicy};

switchy_network::set_default_policy(
    LinkPolicy::reliable()
        .with_latency(Duration::from_millis(1), Duration::from_millis(20))
        .with_loss(0.01),
);

let a = Endpoint::bind("a").unwrap();
let b = Endpoint::bind("b").unwrap();
a.send("b", b"hello".to_vec()).unwrap();

async fn receive(b: &Endpoint) {
    // Waits on the simulated clock; errors if "b" crashes meanwhile.
    let _ = b.recv_timeout(Duration::from_secs(1)).await;
}

switchy_network::partition(["a"], ["b"]);
switchy_network::node_down("b");
switchy_network::node_up("b");
switchy_network::heal();
```
