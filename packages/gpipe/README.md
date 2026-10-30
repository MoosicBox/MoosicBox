# Generic Pipelines (gpipe)

Native pull-worker pipeline orchestration, plus a separate workflow AST API.
The opt-in `runtime` feature provides a SQLite-backed coordinator, authenticated
HTTP protocol, native workers, resolved JSON plans, dynamic task generation, and
Clippier case import. No Docker is required. This is an implementation under
acceptance testing, **not a production CI cutover or full workflow replacement**.

## Trust and deployment model

- Workers execute trusted commands as their desktop/service account, **not in a
  sandbox**. Commands can access that account's files, network, and credentials.
  Prefer a dedicated account and private state directories; never run unreviewed
  pull-request code on a personal desktop worker.
- The administrator authorizes repository IDs, exact revisions, worker labels,
  and CPU/RAM reservations. Reservations govern admission, **not OS-enforced CPU
  or memory caps**. A worker processes assignments sequentially; do not share its
  state directory between processes.
- The coordinator binds loopback only. For remote workers, terminate HTTPS in a
  controlled TLS reverse proxy forwarding to loopback. Keep bearer authentication
  enabled; forwarded identity headers confer no authority. Do not expose raw HTTP.
- Use distinct administrator and per-worker tokens: 32–4096 printable, non-space
  ASCII characters. Load them through the configured environment variable names,
  not command-line arguments or task environment. Do not print secrets: bounded
  execution output is persisted and transmitted, and redaction is not a general
  secret scanner.

## Minimal local run

These are POSIX-shell examples for a trusted Linux checkout with Git and `sh`
installed. Run from the workspace root; use absolute paths in configuration.
The snippets create example configuration files locally; replace paths and supply
fresh random secrets through your secret manager/environment.

```sh
cargo build -p gpipe --features runtime
# Subsequent examples use this exact built binary:
./target/debug/gpipe --help

# Set these securely in the server shell; the worker also needs its own token.
# Each value must be unique and at least 32 characters. Do not use sample tokens.
# export GPIPE_ADMIN_TOKEN=...
# export GPIPE_DESKTOP_TOKEN=...
```

The worker never fetches remote objects or initializes submodules. Provision a
local source clone yourself and ensure it already contains the approved commit:

```sh
git -C /absolute/source/repository rev-parse HEAD
```

Copy that full **40- or 64-hex SHA** into `plan.json` below, replacing
`REPLACE_WITH_EXACT_SHA`. Branch names and abbreviated SHAs are rejected. The
repository ID `demo` is an authorization key, not a remote URL; it must match both
configurations. Workers clone the mapped source into isolated attempt workspaces
and check out the exact revision, leaving the source checkout unchanged.

`server.json`:

```json
{
  "listen": "127.0.0.1:8787",
  "database": "/absolute/private/gpipe/coordinator.sqlite",
  "admin_token_env": "GPIPE_ADMIN_TOKEN",
  "repositories": {"demo": {"revisions": []}},
  "workers": {
    "desktop": {
      "token_env": "GPIPE_DESKTOP_TOKEN",
      "repositories": ["demo"],
      "labels": {"os": "linux"},
      "cpu": 2,
      "memory_mb": 4096
    }
  }
}
```

Create the private database parent directory before serving. `worker.json`:

```json
{
  "coordinator": "http://127.0.0.1:8787",
  "token_env": "GPIPE_DESKTOP_TOKEN",
  "directory": "/absolute/private/gpipe/desktop",
  "repositories": {"demo": "/absolute/source/repository"},
  "environment": {
    "HOME": "/home/worker",
    "CARGO_HOME": "/home/worker/.cargo",
    "RUSTUP_HOME": "/home/worker/.rustup"
  },
  "poll_secs": 2
}
```

`WorkerConfig.environment` is an explicit allowlist passed to commands. Configure
`HOME`, `CARGO_HOME`, and `RUSTUP_HOME` for Cargo/rustup tools; configure `PATH`
explicitly too if installed tools are not discoverable through the executor's
baseline path. Use real worker-local paths (Windows equivalents on Windows).
Do not put administrator, worker, or provider tokens in this map. Tools/toolchains
must already be installed; the worker does not bootstrap them.

`plan.json`:

```json
{
  "repository": "demo",
  "revision": "REPLACE_WITH_EXACT_SHA",
  "tasks": {
    "hello": {
      "needs": [],
      "steps": [{
        "shell": "sh",
        "run": "printf 'hello from native gpipe\\n'",
        "env": {},
        "timeout_secs": 30
      }],
      "labels": {"os": "linux"},
      "cpu": 1,
      "memory_mb": 128,
      "max_attempts": 1
    }
  }
}
```

Start the coordinator in one terminal, with both secrets available:

```sh
./target/debug/gpipe serve --config server.json
```

In an administrator terminal with `GPIPE_ADMIN_TOKEN` set:

```sh
./target/debug/gpipe validate plan.json
./target/debug/gpipe approve --url http://127.0.0.1:8787 demo REPLACE_WITH_EXACT_SHA
./target/debug/gpipe submit --url http://127.0.0.1:8787 --key demo-first plan.json
# Copy run_id from the SUBMITTED response; use it in place of RUN_ID below.
./target/debug/gpipe inspect --url http://127.0.0.1:8787 RUN_ID
```

In a worker terminal with only its worker token available:

```sh
./target/debug/gpipe worker --config worker.json --once
```

`--once` processes at most one assignment (and can find no eligible work); it does
not drain a multi-task plan. Omit it for polling service operation. Inspect again
to confirm terminal results. Management commands include:

```sh
./target/debug/gpipe list --url http://127.0.0.1:8787
./target/debug/gpipe inspect --url http://127.0.0.1:8787 RUN_ID
# For an unwanted run, including one with active work:
./target/debug/gpipe cancel --url http://127.0.0.1:8787 RUN_ID
```

Submission keys are stable idempotency keys: retry the same key with the same
original plan and dynamic flag; changed content conflicts. Cancellation and lease
loss fence attempts and tell workers to terminate their process trees. A reported
success from an obsolete attempt cannot override the accepted state. This is not
exactly-once external side-effect execution: commands must tolerate retries.

## Resolved plans and dynamic generators

`plan::Plan` contains `repository`, `revision`, and a task-ID map. Each task has
`needs`, ordered `steps`, worker `labels`, `cpu`, `memory_mb`, and `max_attempts`.
Each step explicitly specifies `shell`, `run`, `env`, and `timeout_secs`.
Validation bounds graph/text sizes, rejects unsafe IDs, missing dependencies,
cycles, empty steps, and zero CPU/timeouts/attempts. Memory zero means no minimum.
`Plan::expand` validates atomically: identical task replay is allowed, replacement
is not. Deserialization alone is not validation. HTTP transport limits can be
smaller than the model's in-memory bounds.

Submit with `--dynamic` to keep a graph open. An administrator may use
`expand --url URL RUN_ID additions.json` (a task map) and `seal --url URL RUN_ID`.
An open graph cannot become successful until sealed.

A generator opts in with the explicit step environment entry:

```json
{"GPIPE_TASKS_OUTPUT": "gpipe.tasks.json"}
```

After successful execution, the worker reads that regular, non-symlink file from
its attempt workspace (maximum 2 MiB):

```json
{
  "tasks": {
    "generated-check": {
      "needs": ["generator"],
      "steps": [{"shell":"sh","run":"printf generated","env":{},"timeout_secs":30}],
      "labels": {"os":"linux"},
      "cpu": 1,
      "memory_mb": 128,
      "max_attempts": 1
    }
  },
  "seal": true
}
```

Here `generator` must be the actual generating task ID. Completion, additions,
and sealing are durably spooled together and validated atomically by the
coordinator using the worker's **attempt-scoped** identity—not administrator
credentials. New IDs only; every generated task must directly depend on its
generator, preserve its placement requirements, and stay within its CPU/memory/
attempt ceilings. Generated tasks cannot delegate generation again. The source
repository/revision cannot change. Invalid output fails rather than escalating
privileges or partially publishing a graph. Exact accepted completion replay is
retained across restart. Only an open, authorized dynamic run accepts additions.

## Clippier import

Import consumes the official jobs-array JSON from Clippier `plan-matrix` or
`schedule-matrix`, preserving every case ID/configuration (including duplicates),
OS requirements, and recorded seed. It does not invent a `clippier run-matrix`
CLI: `run-matrix` is a GitHub composite-action entry point.

```sh
# jobs.json is the output of: clippier plan-matrix transformed-cases.json
./target/debug/gpipe rust-recipes jobs.json --seed 42 > recipes.json
./target/debug/gpipe import-clippier jobs.json recipes.json \
  --repository demo --revision REPLACE_WITH_EXACT_SHA --seed 42 > imported-plan.json
./target/debug/gpipe validate imported-plan.json
./target/debug/gpipe verify-clippier jobs.json receipts.json --seed 42
```

Use `--seed` only when that generation seed was actually recorded. Recipes are an
explicit per-case map of executable steps, labels, CPU, memory, and attempts;
caller-authored recipes can replace the conservative generator. The native Rust
subset runs check/clippy/test with explicit stable/nightly toolchains,
`--no-default-features`, required features, and separate feature combinations.
It rejects unsupported metadata/setup instead of silently omitting requirements.
It is **not full `rust-validation.yml` parity**: no coverage, format, machete,
custom setup/dependencies, or arbitrary template/test-command support. Inspect
`clippier::MatrixPlan::rust_validation_recipes` for the exact supported contract.

Each case ID is its task ID; `case_task_mapping()` exposes the mapping. Receipt
verification requires exactly one successful `{ "case": <unchanged full case>,
"exit_code": 0 }` per expected case. Missing, duplicate, unexpected, changed, or
failed cases are rejected. Collect/scoped-authenticate receipts for the actual
run; a task's zero exit or an old receipt is not proof. Receipts do not themselves
bind repository/revision or external seed. Explicit artifact declarations can
transport receipt files, but automatic collection and binding of all receipts to
the final run outcome remains integration work.

## Operations and recovery

- **Backup/restore:** stop the coordinator and all users of its SQLite database
  before an offline backup. Preserve the database and any accompanying `-wal` /
  `-shm` sidecars as one consistent set; never copy a live main file alone or
  discard sidecars to “repair” it. Back up server policy, secrets separately, and
  private worker state (pending/fenced evidence and retained workspaces). Restore
  into a private directory with the same compatible binary/configuration; test
  recovery before reopening submissions. Never run the original and restored
  coordinator as competing authorities.
- **Relocation:** drain work and deliver pending completions before moving. Keep
  the database, approvals/policies, administrator token, and worker token identities
  intact. Start only the new coordinator, then update worker `coordinator` URLs.
  Pending records bind the exact normalized RPC endpoint and token environment
  name: changing the URL with pending evidence fails closed. Prefer retaining the
  stable URL; otherwise reconcile against the old authority before relocation.
  Do not edit/delete pending records to bypass identity checks.
- **Worker crash:** inspect `worker.lock` in its private state directory. Confirm
  the old worker process **and all its command descendants are stopped**, back up
  the directory, and inspect pending/temp/fenced evidence. Only then may an
  administrator remove the stale lock file itself and restart with the same
  configuration/token identity. Never delete the state directory, pending spool,
  or attempt workspace as a shortcut. Conflicts or partial archives need manual
  reconciliation; retained `fenced-<attempt>.json` records are evidence, not success.
- **Upgrade:** there is no supported HA deployment or automatic upgrade/migration
  procedure. Unsupported protocol/schema versions are rejected. Stop, back up,
  check compatibility, and validate restoration before changing binaries. Durable
  state is bounded; retention/archival beyond those limits needs operator planning.

## Scope and acceptance status

The current acceptance targets are repository tests, not a claim of 100% product
completion. Relevant suites include plan/DAG/atomic-expansion tests, coordinator
lease/auth/replay/dynamic-state tests, worker/executor recovery tests, Clippier
identity/receipt/native-recipe tests, local artifact tests, provider mock tests,
and CLI/native/webhook/dynamic/artifact/Clippier integration fixtures in `tests/`.
Run `cargo test -p gpipe --features runtime` and
`cargo clippy -p gpipe --all-targets --features runtime -- -D warnings` for the
current checkout. Native acceptance fixtures execute on the local Linux host;
provider request tests do not establish hosted execution.

Known boundaries:

- Task dependencies can transfer **explicit regular-file artifacts** through the
  authenticated coordinator data plane. Declare `GPIPE_ARTIFACTS` in step environment:
  `{"outputs":[{"name":"binary","path":"out/app"}],"inputs":[]}` on a producer;
  `{"inputs":[{"task":"build","name":"binary","path":"bin/app"}],"outputs":[]}`
  on a direct dependent. Files stream in bounded chunks with digest verification;
  paths reject traversal/symlinks and downloads never overwrite existing files.
  Coordinator files live beside the database under `<database>.artifacts`; retain
  them with backups. Automatic run disposal/retention is not yet implemented.
  Retained execution logs are bounded tails, not full logs.
- GitHub/GitLab provider clients are administrator-side adapters, **not automatic
  capacity orchestration or worker bootstrap**. Eligibility/budget reservations,
  reconciliation, workflow installation, and secret management remain external.
- Optional signed webhook adapters submit configured plans; they do not make the
  generic AST an executable workflow parser. Use explicit trigger configuration
  and revision policy; see `triggers` API docs.
- No live hosted-provider acceptance, native Windows/macOS execution acceptance,
  or full MoosicBox workflow coverage is established here. No production cutover.

## AST API (separate from resolved execution)

The default `ast` feature re-exports `gpipe_ast` through `gpipe::ast`. Existing core
types remain `Workflow`, `Trigger`, `Job`, `Step`, `ActionDef`, and `Expression`,
with serde and `gpipe::ast::serde_yaml` support. The AST describes generic workflows;
there is no automatic AST/YAML-to-runtime lowering or GitHub/GitLab translation
command. `gpipe validate` validates a resolved **JSON Plan**, not workflow YAML.

```toml
[dependencies]
gpipe = { version = "0.5.0", default-features = false, features = ["ast"] }
```

```rust
use gpipe::ast::*;
use gpipe::ast::serde_yaml;
use std::collections::BTreeMap;

let workflow = Workflow {
    version: "1.0".to_string(),
    name: "example".to_string(),
    triggers: vec![],
    actions: BTreeMap::new(),
    jobs: BTreeMap::new(),
};
let yaml = serde_yaml::to_string(&workflow).unwrap();
```

AST jobs retain `needs`, `env`, `strategy`, and `steps`; steps can reference actions
or contain scripts with conditions and environment settings. Expressions represent
conditional syntax, not evaluated runtime authorization. See
[`ast/README.md`](ast/README.md) for the existing workflow schema and examples.
Runtime implementation modules live in this crate's `src/`; no separate runner or
CLI crate is required.

## License

See the repository root for license information.
