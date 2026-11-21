# Simvar Observer

What a simvar simulation tells its observers, as plain data with no
dependencies beyond `std`: run starts, progress and results (`SimEvent`),
typed telemetry a simulation emits (scheduler and custom snapshots), and a
`ReplayControl` an observer uses to pause, step and resume a run, run it to a
decision, and set breakpoints (a decision, a step, a thread blocking on
something, or any failure the simulation reports).

Observing never changes a run: events are sent at step boundaries and
scheduling decisions, read state only, and draw nothing from the run's random
streams. Pausing only stops real time.

Front ends (simvar's own TUI, or any other) implement `SimObserver`; because this
crate depends on nothing else, a front end can live in a workspace that must
never enable simulator features.
