# hyperchad_shared_state_models

Domain-neutral shared state synchronization model types for HyperChad.

This crate contains:

* Identifier types (`ChannelId`, `ParticipantId`, `CommandId`, `EventId`)
* Revision and idempotency model types
* Binary payload envelope using `bmux_codec` + `base64` text storage
* Command, event, and snapshot envelopes
* Transport message models

This crate intentionally contains no renderer, routing, or database runtime logic.
