# moosicbox_log_watch

Generic log watch primitives:

- active rolled log file resolution by prefix
- include/exclude regex filters with per-rule case mode
- optional profile persistence (`persistence-json`)
- optional interactive ratatui watch UI (`tui`)

## Run

From workspace root:

```bash
cargo log:watch
```

Example with filters:

```bash
cargo log:watch -- --since 10m --include-i error --exclude healthcheck
```
