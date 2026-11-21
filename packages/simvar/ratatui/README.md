# Simvar Ratatui

simvar's terminal display: a progress gauge per running simulation, with its
worker, run, seed and step, and a count of finished runs. It draws with ratatui
and learns everything from `simvar_observer` events, so it is one observer
among others, not part of the harness.

simvar's `tui` feature (on by default) starts it whenever standard output is a
terminal and `NO_TUI` is unset. `q` or Ctrl-C ends the simulation; once every
run is done, the display stays until you quit it.

To use it with another harness, `spawn` it with what quitting should do,
register it as an observer, and `join` it when the simulation is done.
